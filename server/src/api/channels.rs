//! Channel API —— 分发出口管理。
//!
//! Channel 表示「内容往哪儿去」，**不是网站的一部分**。
//! `provider` + `external_id` 唯一约束保证同一外部账号不会被登记两次；
//! 凭据类配置放 `config_json`，本层不回显敏感字段明文。

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
pub struct ChannelQuery {
    pub r#type: Option<String>,
    pub status: Option<String>,
}

fn channel_json(r: &crate::cmsdb::Row) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("id".into(), txt(r, "id").into());
    m.insert("name".into(), txt(r, "name").into());
    m.insert("type".into(), txt(r, "type").into());
    m.insert("provider".into(), opt(r, "provider").into());
    m.insert("externalId".into(), opt(r, "external_id").into());
    m.insert("url".into(), opt(r, "url").into());
    m.insert("status".into(), txt(r, "status").into());
    m.insert("metadata".into(), js(r, "metadata_json"));
    m.insert("createdAt".into(), txt(r, "created_at").into());
    m.insert("updatedAt".into(), txt(r, "updated_at").into());
    m
}

/// GET /api/v1/channels
pub async fn list(
    State(st): State<AppState>,
    auth: Auth,
    Query(q): Query<ChannelQuery>,
) -> ApiResult {
    ensure(&auth, "domain.channel.view")?;

    let mut wheres: Vec<String> = vec!["1=1".into()];
    let mut vals: Vec<sea_orm::Value> = vec![];
    if let Some(t) = &q.r#type {
        wheres.push("type = ?".into());
        vals.push(s(t.clone()));
    }
    if let Some(stt) = &q.status {
        wheres.push("status = ?".into());
        vals.push(s(stt.clone()));
    } else {
        wheres.push("status = 'active'".into());
    }

    let sql = format!(
        "SELECT c.*,
                (SELECT COUNT(*) FROM content_channels cc WHERE cc.channel_id = c.id) AS content_count
           FROM channels c
          WHERE {}
          ORDER BY c.type ASC, c.name ASC",
        wheres.join(" AND ")
    );
    let rows = st
        .db
        .query_all(&sql, vals)
        .await
        .map_err(crate::api::db_err("查询渠道失败"))?;

    let items = rows
        .iter()
        .map(|r| {
            let mut m = channel_json(r);
            m.insert("contentCount".into(), crate::api::int(r, "content_count").into());
            Value::Object(m)
        })
        .collect::<Vec<_>>();
    let total = items.len();
    ok_list(items, total)
}

/// GET /api/v1/channels/:id
pub async fn get_one(State(st): State<AppState>, auth: Auth, Path(id): Path<String>) -> ApiResult {
    ensure(&auth, "domain.channel.view")?;
    let r = must_get(&st, "channels", &id).await?;
    let mut m = channel_json(&r);
    // 单条详情才回显配置（含凭据），列表不回显
    m.insert("config".into(), js(&r, "config_json"));
    ok(Value::Object(m))
}

/// GET /api/v1/channels/:id/contents —— 这个渠道发过什么
pub async fn contents(
    State(st): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
) -> ApiResult {
    ensure(&auth, "domain.channel.view")?;
    let rows = st
        .db
        .query_all(
            "SELECT c.id AS id, c.type AS type, c.title AS title,
                    cc.status AS status, cc.external_url AS external_url,
                    cc.published_at AS published_at
               FROM content_channels cc
               JOIN contents c ON c.id = cc.content_id
              WHERE cc.channel_id = ?
              ORDER BY COALESCE(cc.published_at, '') DESC",
            vec![s(id)],
        )
        .await
        .map_err(crate::api::db_err("查询渠道内容失败"))?;
    let items = rows
        .iter()
        .map(|r| {
            json!({
                "id": txt(r, "id"), "type": txt(r, "type"), "title": opt(r, "title"),
                "status": txt(r, "status"), "externalUrl": opt(r, "external_url"),
                "publishedAt": opt(r, "published_at"),
            })
        })
        .collect::<Vec<_>>();
    let total = items.len();
    ok_list(items, total)
}

/// POST /api/v1/channels
pub async fn create(State(st): State<AppState>, auth: Auth, Json(body): Json<Value>) -> ApiResult {
    ensure(&auth, "domain.channel.create")?;
    let name = b_str(&body, "name").ok_or_else(|| ApiError::bad("缺少 name"))?;
    let ctype = b_str(&body, "type").ok_or_else(|| ApiError::bad("缺少 type"))?;
    if !crate::entity::CHANNEL_TYPES.contains(&ctype.as_str()) {
        return Err(ApiError::bad(format!(
            "type 必须是 {} 之一",
            crate::entity::CHANNEL_TYPES.join(" / ")
        )));
    }
    let id = uuid::Uuid::new_v4().to_string();
    let ts = now();

    st.db
        .execute(
            "INSERT INTO channels
             (id, name, type, provider, external_id, url, config_json, metadata_json, status, created_at, updated_at)
             VALUES (?,?,?,?,?,?,?,?,?,?,?)",
            vec![
                s(id.clone()),
                s(name),
                s(ctype),
                so(b_str(&body, "provider").filter(|v| !v.is_empty())),
                so(b_str(&body, "externalId").filter(|v| !v.is_empty())),
                so(b_str(&body, "url").filter(|v| !v.is_empty())),
                s(crate::api::to_json_text(b_js(&body, "config").as_ref())),
                s(crate::api::to_json_text(b_js(&body, "metadata").as_ref())),
                s(b_str(&body, "status").unwrap_or_else(|| "active".into())),
                s(ts.clone()),
                s(ts),
            ],
        )
        .await
        .map_err(crate::api::db_err("创建渠道失败"))?;

    ok_id(id)
}

/// PATCH /api/v1/channels/:id
pub async fn update(
    State(st): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> ApiResult {
    ensure(&auth, "domain.channel.update")?;
    must_get(&st, "channels", &id).await?;

    let mut sets: Vec<String> = vec![];
    let mut vals: Vec<sea_orm::Value> = vec![];
    if let Some(v) = b_str(&body, "name") {
        sets.push("name = ?".into());
        vals.push(s(v));
    }
    if let Some(v) = b_str(&body, "type") {
        if !crate::entity::CHANNEL_TYPES.contains(&v.as_str()) {
            return Err(ApiError::bad(format!(
                "type 必须是 {} 之一",
                crate::entity::CHANNEL_TYPES.join(" / ")
            )));
        }
        sets.push("type = ?".into());
        vals.push(s(v));
    }
    if let Some(v) = b_str(&body, "url") {
        sets.push("url = ?".into());
        vals.push(s(v));
    }
    if let Some(v) = b_js(&body, "config") {
        sets.push("config_json = ?".into());
        vals.push(s(crate::api::to_json_text(Some(&v))));
    }
    if let Some(v) = b_js(&body, "metadata") {
        sets.push("metadata_json = ?".into());
        vals.push(s(crate::api::to_json_text(Some(&v))));
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

    let sql = format!("UPDATE channels SET {} WHERE id = ?", sets.join(", "));
    st.db
        .execute(&sql, vals)
        .await
        .map_err(crate::api::db_err("更新渠道失败"))?;

    ok_id(id)
}

/// DELETE /api/v1/channels/:id
pub async fn remove(State(st): State<AppState>, auth: Auth, Path(id): Path<String>) -> ApiResult {
    ensure(&auth, "domain.channel.update")?;
    must_get(&st, "channels", &id).await?;
    st.db
        .execute("DELETE FROM channels WHERE id = ?", vec![s(id)])
        .await
        .map_err(crate::api::db_err("删除渠道失败"))?;
    ok_empty()
}
