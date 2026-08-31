//! Theme API —— 决定「怎么呈现」。
//!
//! 与 Template 的分工：Template 管结构怎么组合，Theme 管长什么样。
//! 真正的载荷在 `theme_versions.tokens_json`（schema `admin.theme.v1`：
//! color / typography / spacing / layout / radius）+ `components_json`。
//!
//! 令牌以 JSON 存储而非拆列，是因为设计令牌的层级与键名会随设计系统演进；
//! 拆列意味着每加一个令牌就要改一次表。版本化 + JSON 让演进停机为零。

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
pub struct ThemeQuery {
    pub status: Option<String>,
}

fn theme_json(r: &crate::cmsdb::Row) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("id".into(), txt(r, "id").into());
    m.insert("name".into(), txt(r, "name").into());
    m.insert("slug".into(), txt(r, "slug").into());
    m.insert("description".into(), opt(r, "description").into());
    m.insert("status".into(), txt(r, "status").into());
    m.insert("metadata".into(), js(r, "metadata_json"));
    m.insert("createdAt".into(), txt(r, "created_at").into());
    m.insert("updatedAt".into(), txt(r, "updated_at").into());
    m
}

/// GET /api/v1/themes
pub async fn list(
    State(st): State<AppState>,
    auth: Auth,
    Query(q): Query<ThemeQuery>,
) -> ApiResult {
    ensure(&auth, "domain.theme.view")?;
    let mut wheres: Vec<String> = vec!["1=1".into()];
    let mut vals: Vec<sea_orm::Value> = vec![];
    if let Some(stt) = &q.status {
        wheres.push("status = ?".into());
        vals.push(s(stt.clone()));
    }
    let sql = format!(
        "SELECT th.*,
                (SELECT MAX(v.version) FROM theme_versions v WHERE v.theme_id = th.id) AS latest_version
           FROM themes th
          WHERE {}
          ORDER BY th.name ASC",
        wheres.join(" AND ")
    );
    let rows = st
        .db
        .query_all(&sql, vals)
        .await
        .map_err(crate::api::db_err("查询主题失败"))?;
    let items = rows
        .iter()
        .map(|r| {
            let mut m = theme_json(r);
            m.insert("latestVersion".into(), crate::api::int(r, "latest_version").into());
            Value::Object(m)
        })
        .collect::<Vec<_>>();
    let total = items.len();
    ok_list(items, total)
}

/// GET /api/v1/themes/:id
pub async fn get_one(State(st): State<AppState>, auth: Auth, Path(id): Path<String>) -> ApiResult {
    ensure(&auth, "domain.theme.view")?;
    let r = must_get(&st, "themes", &id).await?;
    ok(Value::Object(theme_json(&r)))
}

/// POST /api/v1/themes
pub async fn create(State(st): State<AppState>, auth: Auth, Json(body): Json<Value>) -> ApiResult {
    ensure(&auth, "domain.theme.create")?;
    let name = b_str(&body, "name").ok_or_else(|| ApiError::bad("缺少 name"))?;
    let slug = b_str(&body, "slug").ok_or_else(|| ApiError::bad("缺少 slug"))?;
    let id = uuid::Uuid::new_v4().to_string();
    let ts = now();

    if st
        .db
        .query_one("SELECT id FROM themes WHERE slug = ? LIMIT 1", vec![s(slug.clone())])
        .await
        .map_err(crate::api::db_err("检查 slug 失败"))?
        .is_some()
    {
        return Err(ApiError::bad(format!("slug 已存在：{slug}")));
    }

    st.db
        .execute(
            "INSERT INTO themes (id, name, slug, description, status, metadata_json, created_at, updated_at)
             VALUES (?,?,?,?,?,?,?,?)",
            vec![
                s(id.clone()),
                s(name),
                s(slug),
                so(b_str(&body, "description")),
                s(b_str(&body, "status").unwrap_or_else(|| "draft".into())),
                s(crate::api::to_json_text(b_js(&body, "metadata").as_ref())),
                s(ts.clone()),
                s(ts),
            ],
        )
        .await
        .map_err(crate::api::db_err("创建主题失败"))?;

    // 可选：创建时直接带上首版令牌
    if let Some(tokens) = b_js(&body, "tokens") {
        st.db
            .execute(
                "INSERT INTO theme_versions (id, theme_id, version, tokens_json, components_json, status, created_at)
                 VALUES (?,?,?,?,?,?,?)",
                vec![
                    s(uuid::Uuid::new_v4().to_string()),
                    s(id.clone()),
                    sea_orm::Value::BigInt(Some(1)),
                    s(crate::api::to_json_text(Some(&tokens))),
                    s(crate::api::to_json_text(b_js(&body, "components").as_ref())),
                    s(b_str(&body, "status").unwrap_or_else(|| "draft".into())),
                    s(now()),
                ],
            )
            .await
            .map_err(crate::api::db_err("创建首版令牌失败"))?;
    }

    ok_id(id)
}

/// PATCH /api/v1/themes/:id —— 只改元数据，不改令牌
pub async fn update(
    State(st): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> ApiResult {
    ensure(&auth, "domain.theme.update")?;
    must_get(&st, "themes", &id).await?;

    let mut sets: Vec<String> = vec![];
    let mut vals: Vec<sea_orm::Value> = vec![];
    if let Some(v) = b_str(&body, "name") {
        sets.push("name = ?".into());
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

    let sql = format!("UPDATE themes SET {} WHERE id = ?", sets.join(", "));
    st.db
        .execute(&sql, vals)
        .await
        .map_err(crate::api::db_err("更新主题失败"))?;

    ok_id(id)
}

// ── 版本 ──────────────────────────────────────────────────────────────────

/// GET /api/v1/themes/:id/versions
pub async fn list_versions(
    State(st): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
) -> ApiResult {
    ensure(&auth, "domain.theme.view")?;
    let rows = st
        .db
        .query_all(
            "SELECT * FROM theme_versions WHERE theme_id = ? ORDER BY version DESC",
            vec![s(id)],
        )
        .await
        .map_err(crate::api::db_err("查询主题版本失败"))?;
    let items = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": txt(r, "id"),
                "themeId": txt(r, "theme_id"),
                "version": crate::api::int(r, "version"),
                "tokens": js(r, "tokens_json"),
                "components": js(r, "components_json"),
                "status": txt(r, "status"),
                "createdAt": txt(r, "created_at"),
            })
        })
        .collect::<Vec<_>>();
    let total = items.len();
    ok_list(items, total)
}

/// GET /api/v1/themes/:id/versions/:version
pub async fn get_version(
    State(st): State<AppState>,
    auth: Auth,
    Path((id, version)): Path<(String, i64)>,
) -> ApiResult {
    ensure(&auth, "domain.theme.view")?;
    let r = st
        .db
        .query_one(
            "SELECT * FROM theme_versions WHERE theme_id = ? AND version = ? LIMIT 1",
            vec![s(id), sea_orm::Value::BigInt(Some(version))],
        )
        .await
        .map_err(crate::api::db_err("查询主题版本失败"))?
        .ok_or_else(|| ApiError::not_found("版本不存在"))?;
    ok(serde_json::json!({
        "id": txt(&r, "id"),
        "themeId": txt(&r, "theme_id"),
        "version": version,
        "tokens": js(&r, "tokens_json"),
        "components": js(&r, "components_json"),
        "status": txt(&r, "status"),
        "createdAt": txt(&r, "created_at"),
    }))
}

#[derive(Deserialize)]
pub struct CreateVersionReq {
    /// 设计令牌；缺省则继承上一版
    pub tokens: Option<Value>,
    pub components: Option<Value>,
    pub status: Option<String>,
}

/// POST /api/v1/themes/:id/versions —— 追加新版本（版本号自动递增）
pub async fn create_version(
    State(st): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
    Json(body): Json<CreateVersionReq>,
) -> ApiResult {
    ensure(&auth, "domain.theme.update")?;
    must_get(&st, "themes", &id).await?;

    let prev = st
        .db
        .query_one(
            "SELECT * FROM theme_versions WHERE theme_id = ? ORDER BY version DESC LIMIT 1",
            vec![s(id.clone())],
        )
        .await
        .map_err(crate::api::db_err("查询最新版本失败"))?;

    let next_version = match &prev {
        Some(r) => crate::api::int(r, "version") + 1,
        None => 1,
    };

    let (tokens, components) = match (&body.tokens, &prev) {
        (Some(t), _) => (t.clone(), body.components.clone().unwrap_or_else(|| serde_json::json!({}))),
        (None, Some(r)) => (js(r, "tokens_json"), js(r, "components_json")),
        (None, None) => (
            serde_json::json!({ "schema": "admin.theme.v1", "tokens": {} }),
            serde_json::json!({}),
        ),
    };

    let vid = uuid::Uuid::new_v4().to_string();
    st.db
        .execute(
            "INSERT INTO theme_versions (id, theme_id, version, tokens_json, components_json, status, created_at)
             VALUES (?,?,?,?,?,?,?)",
            vec![
                s(vid.clone()),
                s(id.clone()),
                sea_orm::Value::BigInt(Some(next_version)),
                s(crate::api::to_json_text(Some(&tokens))),
                s(crate::api::to_json_text(Some(&components))),
                s(body.status.unwrap_or_else(|| "draft".into())),
                s(now()),
            ],
        )
        .await
        .map_err(crate::api::db_err("创建主题版本失败"))?;

    ok(serde_json::json!({ "id": vid, "themeId": id, "version": next_version }))
}

/// DELETE /api/v1/themes/:id —— 级联删除其所有版本
pub async fn remove(State(st): State<AppState>, auth: Auth, Path(id): Path<String>) -> ApiResult {
    ensure(&auth, "domain.theme.update")?;
    must_get(&st, "themes", &id).await?;
    st.db
        .execute("DELETE FROM themes WHERE id = ?", vec![s(id)])
        .await
        .map_err(crate::api::db_err("删除主题失败"))?;
    ok_empty()
}
