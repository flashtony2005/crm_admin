//! Context API —— Agent 的语义上下文。
//!
//! **不是 Tag 管理页**。每个 Context 自带 audience / intent / industry / region
//! 等语义（存在 `data_json` 里），因此列表会带上内容计数，
//! 让「哪个语义方向有货、哪个是空的」一眼可见——这是 Analyst 选方向的依据。

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::{
    api::{b_js, b_str, js, must_get, now, ok, ok_empty, ok_id, ok_list, opt, s, so, txt},
    auth::{ensure, Auth},
    error::{ApiError, ApiResult},
    state::AppState,
};

#[derive(Deserialize, Default)]
pub struct ContextQuery {
    pub r#type: Option<String>,
    pub parent: Option<String>,
    pub status: Option<String>,
}

fn context_json(r: &crate::cmsdb::Row) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("id".into(), txt(r, "id").into());
    m.insert("type".into(), txt(r, "type").into());
    m.insert("name".into(), txt(r, "name").into());
    m.insert("slug".into(), txt(r, "slug").into());
    m.insert("description".into(), opt(r, "description").into());
    m.insert("data".into(), js(r, "data_json"));
    m.insert("metadata".into(), js(r, "metadata_json"));
    m.insert("parentId".into(), opt(r, "parent_id").into());
    m.insert("status".into(), txt(r, "status").into());
    m.insert("createdAt".into(), txt(r, "created_at").into());
    m.insert("updatedAt".into(), txt(r, "updated_at").into());
    m
}

/// GET /api/v1/contexts
pub async fn list(
    State(st): State<AppState>,
    auth: Auth,
    Query(q): Query<ContextQuery>,
) -> ApiResult {
    ensure(&auth, "domain.context.view")?;

    let mut wheres: Vec<String> = vec!["1=1".into()];
    let mut vals: Vec<sea_orm::Value> = vec![];
    if let Some(t) = &q.r#type {
        wheres.push("type = ?".into());
        vals.push(s(t.clone()));
    }
    if let Some(p) = &q.parent {
        wheres.push("parent_id = ?".into());
        vals.push(s(p.clone()));
    }
    if let Some(stt) = &q.status {
        wheres.push("status = ?".into());
        vals.push(s(stt.clone()));
    } else {
        wheres.push("status = 'active'".into());
    }

    let sql = format!(
        "SELECT x.*,
                (SELECT COUNT(*) FROM content_contexts cc WHERE cc.context_id = x.id) AS content_count
           FROM contexts x
          WHERE {}
          ORDER BY content_count DESC, x.name ASC",
        wheres.join(" AND ")
    );
    let rows = st
        .db
        .query_all(&sql, vals)
        .await
        .map_err(crate::api::db_err("查询上下文失败"))?;

    let items = rows
        .iter()
        .map(|r| {
            let mut m = context_json(r);
            m.insert("contentCount".into(), crate::api::int(r, "content_count").into());
            Value::Object(m)
        })
        .collect::<Vec<_>>();
    let total = items.len();
    ok_list(items, total)
}

/// GET /api/v1/contexts/:id
pub async fn get_one(State(st): State<AppState>, auth: Auth, Path(id): Path<String>) -> ApiResult {
    ensure(&auth, "domain.context.view")?;
    let r = must_get(&st, "contexts", &id).await?;
    ok(Value::Object(context_json(&r)))
}

/// GET /api/v1/contexts/:id/contents —— 该语义下挂了哪些内容
pub async fn contents(
    State(st): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
) -> ApiResult {
    ensure(&auth, "domain.context.view")?;
    let rows = st
        .db
        .query_all(
            "SELECT c.id AS id, c.type AS type, c.title AS title, c.status AS status,
                    cc.role AS role, cc.weight AS weight, c.updated_at AS updated_at
               FROM content_contexts cc
               JOIN contents c ON c.id = cc.content_id
              WHERE cc.context_id = ?
              ORDER BY cc.weight DESC, c.updated_at DESC",
            vec![s(id)],
        )
        .await
        .map_err(crate::api::db_err("查询上下文内容失败"))?;
    let items = rows
        .iter()
        .map(|r| {
            json!({
                "id": txt(r, "id"), "type": txt(r, "type"), "title": opt(r, "title"),
                "status": txt(r, "status"), "role": txt(r, "role"),
                "weight": crate::api::real(r, "weight"), "updatedAt": txt(r, "updated_at"),
            })
        })
        .collect::<Vec<_>>();
    let total = items.len();
    ok_list(items, total)
}

/// POST /api/v1/contexts
pub async fn create(State(st): State<AppState>, auth: Auth, Json(body): Json<Value>) -> ApiResult {
    ensure(&auth, "domain.context.create")?;
    let name = b_str(&body, "name").ok_or_else(|| ApiError::bad("缺少 name"))?;
    let slug = b_str(&body, "slug").unwrap_or_else(|| slugify(&name));
    if slug.is_empty() {
        return Err(ApiError::bad("slug 不能为空"));
    }
    let id = uuid::Uuid::new_v4().to_string();
    let ts = now();

    let dup = st
        .db
        .query_one(
            "SELECT id FROM contexts WHERE slug = ? LIMIT 1",
            vec![s(slug.clone())],
        )
        .await
        .map_err(crate::api::db_err("检查 slug 失败"))?;
    if dup.is_some() {
        return Err(ApiError::bad(format!("slug 已存在：{slug}")));
    }

    st.db
        .execute(
            "INSERT INTO contexts
             (id, type, name, slug, description, data_json, metadata_json, parent_id, status, created_at, updated_at)
             VALUES (?,?,?,?,?,?,?,?,?,?,?)",
            vec![
                s(id.clone()),
                s(b_str(&body, "type").unwrap_or_else(|| "topic".into())),
                s(name),
                s(slug),
                so(b_str(&body, "description")),
                s(crate::api::to_json_text(b_js(&body, "data").as_ref())),
                s(crate::api::to_json_text(b_js(&body, "metadata").as_ref())),
                so(b_str(&body, "parentId").filter(|v| !v.is_empty())),
                s(b_str(&body, "status").unwrap_or_else(|| "active".into())),
                s(ts.clone()),
                s(ts),
            ],
        )
        .await
        .map_err(crate::api::db_err("创建上下文失败"))?;

    ok_id(id)
}

/// PATCH /api/v1/contexts/:id
pub async fn update(
    State(st): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> ApiResult {
    ensure(&auth, "domain.context.update")?;
    must_get(&st, "contexts", &id).await?;

    let mut sets: Vec<String> = vec![];
    let mut vals: Vec<sea_orm::Value> = vec![];
    if let Some(v) = b_str(&body, "type") {
        sets.push("type = ?".into());
        vals.push(s(v));
    }
    if let Some(v) = b_str(&body, "name") {
        sets.push("name = ?".into());
        vals.push(s(v));
    }
    if let Some(v) = b_str(&body, "slug") {
        sets.push("slug = ?".into());
        vals.push(s(v));
    }
    if let Some(v) = b_js(&body, "description") {
        sets.push("description = ?".into());
        vals.push(so(v.as_str().map(|x| x.to_string())));
    }
    if let Some(v) = b_js(&body, "data") {
        sets.push("data_json = ?".into());
        vals.push(s(crate::api::to_json_text(Some(&v))));
    }
    if let Some(v) = b_js(&body, "metadata") {
        sets.push("metadata_json = ?".into());
        vals.push(s(crate::api::to_json_text(Some(&v))));
    }
    if let Some(v) = b_js(&body, "parentId") {
        sets.push("parent_id = ?".into());
        vals.push(so(v.as_str().map(|x| x.to_string())));
    }
    if let Some(v) = b_str(&body, "status") {
        sets.push("status = ?".into());
        vals.push(s(v));
    }

    if sets.is_empty() {
        return ok_id(id);
    }
    sets.push("updated_at = ?".into());
    vals.push(s(now()));
    vals.push(s(id.clone()));

    let sql = format!("UPDATE contexts SET {} WHERE id = ?", sets.join(", "));
    st.db
        .execute(&sql, vals)
        .await
        .map_err(crate::api::db_err("更新上下文失败"))?;

    ok_id(id)
}

/// DELETE /api/v1/contexts/:id
pub async fn remove(State(st): State<AppState>, auth: Auth, Path(id): Path<String>) -> ApiResult {
    ensure(&auth, "domain.context.delete")?;
    must_get(&st, "contexts", &id).await?;
    st.db
        .execute("DELETE FROM contexts WHERE id = ?", vec![s(id)])
        .await
        .map_err(crate::api::db_err("删除上下文失败"))?;
    ok_empty()
}

/// 极简 slug 化：小写、非字母数字转连字符。够用即可，不做 Unicode 全角处理。
fn slugify(s: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for c in s.trim().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}
