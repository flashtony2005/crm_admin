//! Template API —— 决定「怎么组合」。
//!
//! 真正的页面结构在 `template_versions.definition_json`
//! （schema `admin.template.v1`：`{ schema, type, sections[] }`）。
//!
//! **版本一旦发布即不可变**：改结构必须新增版本。
//! 否则外部 Renderer 会在无感知的情况下被换掉页面结构——
//! 这正是设计里把结构单独拆成 template_versions 的原因。

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::{
    api::{b_js, b_str, js, must_get, now, ok, ok_empty, ok_id, ok_list, opt, s, so, txt},
    auth::{ensure, Auth},
    error::{ApiError, ApiResult},
    state::AppState,
};

#[derive(Deserialize, Default)]
pub struct TemplateQuery {
    pub r#type: Option<String>,
    pub status: Option<String>,
}

fn template_json(r: &crate::cmsdb::Row) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("id".into(), txt(r, "id").into());
    m.insert("name".into(), txt(r, "name").into());
    m.insert("slug".into(), txt(r, "slug").into());
    m.insert("type".into(), txt(r, "type").into());
    m.insert("description".into(), opt(r, "description").into());
    m.insert("status".into(), txt(r, "status").into());
    m.insert("metadata".into(), js(r, "metadata_json"));
    m.insert("createdAt".into(), txt(r, "created_at").into());
    m.insert("updatedAt".into(), txt(r, "updated_at").into());
    m
}

/// GET /api/v1/templates
pub async fn list(
    State(st): State<AppState>,
    auth: Auth,
    Query(q): Query<TemplateQuery>,
) -> ApiResult {
    ensure(&auth, "domain.template.view")?;
    let mut wheres: Vec<String> = vec!["1=1".into()];
    let mut vals: Vec<sea_orm::Value> = vec![];
    if let Some(t) = &q.r#type {
        wheres.push("type = ?".into());
        vals.push(s(t.clone()));
    }
    if let Some(stt) = &q.status {
        wheres.push("status = ?".into());
        vals.push(s(stt.clone()));
    }

    let sql = format!(
        "SELECT t.*,
                (SELECT MAX(v.version) FROM template_versions v WHERE v.template_id = t.id) AS latest_version
           FROM templates t
          WHERE {}
          ORDER BY t.type ASC, t.name ASC",
        wheres.join(" AND ")
    );
    let rows = st
        .db
        .query_all(&sql, vals)
        .await
        .map_err(crate::api::db_err("查询模板失败"))?;
    let items = rows
        .iter()
        .map(|r| {
            let mut m = template_json(r);
            m.insert("latestVersion".into(), crate::api::int(r, "latest_version").into());
            Value::Object(m)
        })
        .collect::<Vec<_>>();
    let total = items.len();
    ok_list(items, total)
}

/// GET /api/v1/templates/:id
pub async fn get_one(State(st): State<AppState>, auth: Auth, Path(id): Path<String>) -> ApiResult {
    ensure(&auth, "domain.template.view")?;
    let r = must_get(&st, "templates", &id).await?;
    ok(Value::Object(template_json(&r)))
}

/// POST /api/v1/templates
pub async fn create(State(st): State<AppState>, auth: Auth, Json(body): Json<Value>) -> ApiResult {
    ensure(&auth, "domain.template.create")?;
    let name = b_str(&body, "name").ok_or_else(|| ApiError::bad("缺少 name"))?;
    let slug = b_str(&body, "slug").ok_or_else(|| ApiError::bad("缺少 slug"))?;
    let ttype = b_str(&body, "type").unwrap_or_else(|| "custom".into());
    if !crate::entity::TEMPLATE_TYPES.contains(&ttype.as_str()) {
        return Err(ApiError::bad(format!(
            "type 必须是 {} 之一",
            crate::entity::TEMPLATE_TYPES.join(" / ")
        )));
    }
    let id = uuid::Uuid::new_v4().to_string();
    let ts = now();

    if st
        .db
        .query_one("SELECT id FROM templates WHERE slug = ? LIMIT 1", vec![s(slug.clone())])
        .await
        .map_err(crate::api::db_err("检查 slug 失败"))?
        .is_some()
    {
        return Err(ApiError::bad(format!("slug 已存在：{slug}")));
    }

    st.db
        .execute(
            "INSERT INTO templates (id, name, slug, type, description, status, metadata_json, created_at, updated_at)
             VALUES (?,?,?,?,?,?,?,?,?)",
            vec![
                s(id.clone()),
                s(name),
                s(slug),
                s(ttype),
                so(b_str(&body, "description")),
                s(b_str(&body, "status").unwrap_or_else(|| "draft".into())),
                s(crate::api::to_json_text(b_js(&body, "metadata").as_ref())),
                s(ts.clone()),
                s(ts),
            ],
        )
        .await
        .map_err(crate::api::db_err("创建模板失败"))?;

    // 可选：创建时直接带上首版定义，省掉一次往返
    if let Some(def) = b_js(&body, "definition") {
        st.db
            .execute(
                "INSERT INTO template_versions (id, template_id, version, definition_json, status, created_at)
                 VALUES (?,?,?,?,?,?)",
                vec![
                    s(uuid::Uuid::new_v4().to_string()),
                    s(id.clone()),
                    sea_orm::Value::BigInt(Some(1)),
                    s(crate::api::to_json_text(Some(&def))),
                    s(b_str(&body, "status").unwrap_or_else(|| "draft".into())),
                    s(now()),
                ],
            )
            .await
            .map_err(crate::api::db_err("创建首版定义失败"))?;
    }

    ok_id(id)
}

/// PATCH /api/v1/templates/:id —— 只改元数据，不改结构
pub async fn update(
    State(st): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> ApiResult {
    ensure(&auth, "domain.template.update")?;
    must_get(&st, "templates", &id).await?;

    let mut sets: Vec<String> = vec![];
    let mut vals: Vec<sea_orm::Value> = vec![];
    if let Some(v) = b_str(&body, "name") {
        sets.push("name = ?".into());
        vals.push(s(v));
    }
    if let Some(v) = b_str(&body, "type") {
        if !crate::entity::TEMPLATE_TYPES.contains(&v.as_str()) {
            return Err(ApiError::bad(format!(
                "type 必须是 {} 之一",
                crate::entity::TEMPLATE_TYPES.join(" / ")
            )));
        }
        sets.push("type = ?".into());
        vals.push(s(v));
    }
    if let Some(v) = b_js(&body, "description") {
        sets.push("description = ?".into());
        vals.push(so(v.as_str().map(|x| x.to_string())));
    }
    if let Some(v) = b_str(&body, "status") {
        sets.push("status = ?".into());
        vals.push(s(v));
    }
    if let Some(v) = b_js(&body, "metadata") {
        sets.push("metadata_json = ?".into());
        vals.push(s(crate::api::to_json_text(Some(&v))));
    }

    if sets.is_empty() {
        return ok_id(id);
    }
    sets.push("updated_at = ?".into());
    vals.push(s(now()));
    vals.push(s(id.clone()));

    let sql = format!("UPDATE templates SET {} WHERE id = ?", sets.join(", "));
    st.db
        .execute(&sql, vals)
        .await
        .map_err(crate::api::db_err("更新模板失败"))?;

    ok_id(id)
}

// ── 版本 ──────────────────────────────────────────────────────────────────

/// GET /api/v1/templates/:id/versions
pub async fn list_versions(
    State(st): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
) -> ApiResult {
    ensure(&auth, "domain.template.view")?;
    let rows = st
        .db
        .query_all(
            "SELECT * FROM template_versions WHERE template_id = ? ORDER BY version DESC",
            vec![s(id)],
        )
        .await
        .map_err(crate::api::db_err("查询模板版本失败"))?;
    let items = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": txt(r, "id"),
                "templateId": txt(r, "template_id"),
                "version": crate::api::int(r, "version"),
                "definition": js(r, "definition_json"),
                "status": txt(r, "status"),
                "createdAt": txt(r, "created_at"),
            })
        })
        .collect::<Vec<_>>();
    let total = items.len();
    ok_list(items, total)
}

/// GET /api/v1/templates/:id/versions/:version
pub async fn get_version(
    State(st): State<AppState>,
    auth: Auth,
    Path((id, version)): Path<(String, i64)>,
) -> ApiResult {
    ensure(&auth, "domain.template.view")?;
    let r = st
        .db
        .query_one(
            "SELECT * FROM template_versions WHERE template_id = ? AND version = ? LIMIT 1",
            vec![s(id), sea_orm::Value::BigInt(Some(version))],
        )
        .await
        .map_err(crate::api::db_err("查询模板版本失败"))?
        .ok_or_else(|| ApiError::not_found("版本不存在"))?;
    ok(serde_json::json!({
        "id": txt(&r, "id"),
        "templateId": txt(&r, "template_id"),
        "version": version,
        "definition": js(&r, "definition_json"),
        "status": txt(&r, "status"),
        "createdAt": txt(&r, "created_at"),
    }))
}

#[derive(Deserialize)]
pub struct CreateVersionReq {
    /// 页面结构定义；缺省则复制上一版（作为新草稿的起点）
    pub definition: Option<Value>,
    pub status: Option<String>,
}

/// POST /api/v1/templates/:id/versions —— 追加新版本（版本号自动递增）
///
/// 已发布的版本**不允许在原版本上改**，只能追加新版本。
pub async fn create_version(
    State(st): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
    Json(body): Json<CreateVersionReq>,
) -> ApiResult {
    ensure(&auth, "domain.template.update")?;
    must_get(&st, "templates", &id).await?;

    let prev = st
        .db
        .query_one(
            "SELECT * FROM template_versions WHERE template_id = ? ORDER BY version DESC LIMIT 1",
            vec![s(id.clone())],
        )
        .await
        .map_err(crate::api::db_err("查询最新版本失败"))?;

    let next_version = match &prev {
        Some(r) => crate::api::int(r, "version") + 1,
        None => 1,
    };

    let definition = match body.definition {
        Some(d) => d,
        // 未给定义时继承上一版，便于「基于当前线上版起一版草稿」
        None => match &prev {
            Some(r) => js(r, "definition_json"),
            None => serde_json::json!({ "schema": "admin.template.v1", "type": "page", "sections": [] }),
        },
    };

    let vid = uuid::Uuid::new_v4().to_string();
    st.db
        .execute(
            "INSERT INTO template_versions (id, template_id, version, definition_json, status, created_at)
             VALUES (?,?,?,?,?,?)",
            vec![
                s(vid.clone()),
                s(id.clone()),
                sea_orm::Value::BigInt(Some(next_version)),
                s(crate::api::to_json_text(Some(&definition))),
                s(body.status.unwrap_or_else(|| "draft".into())),
                s(now()),
            ],
        )
        .await
        .map_err(crate::api::db_err("创建模板版本失败"))?;

    ok(serde_json::json!({ "id": vid, "templateId": id, "version": next_version }))
}

/// DELETE /api/v1/templates/:id —— 级联删除其所有版本
pub async fn remove(State(st): State<AppState>, auth: Auth, Path(id): Path<String>) -> ApiResult {
    ensure(&auth, "domain.template.update")?;
    must_get(&st, "templates", &id).await?;
    st.db
        .execute("DELETE FROM templates WHERE id = ?", vec![s(id)])
        .await
        .map_err(crate::api::db_err("删除模板失败"))?;
    ok_empty()
}
