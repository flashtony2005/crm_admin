//! 公开内容 API（免认证，只读）：
//! - GET /api/public/articles           已发布文章列表（不含正文，便于列表聚合）
//! - GET /api/public/articles?tag=xxx   按标签过滤（tags 字段包含匹配）
//! - GET /api/public/articles/{id}      单篇详情（含正文；id 或 slug 均可解析）
//! - GET /api/public/tags               标签列表（独立 Tag 管理表）
//!
//! 仅返回 status='published' 且属于当前租户的文章；供公开站点前端 /
//! Jamstack / 第三方应用消费（headless 用法）。响应沿用统一信封
//! {ok:true,data}，与前端 api()/apiList() 约定一致。
//!
//! 部署：新增模块，本地 `cargo build` 后随新二进制生效。

use axum::extract::{Path, Query, State};
use crate::cmsdb::Row;
use sea_orm::Value as SqlValue;
use serde_json::{json, Value};
use std::collections::HashMap;

use crate::error::{ok, ApiError, ApiResult};
use crate::state::AppState;

/// 读 TEXT 列（NULL→空串）
fn s(r: &Row, col: &str) -> String {
    r.try_get::<String>("", col).unwrap_or_default()
}

/// 读可空 TEXT 列
fn so(r: &Row, col: &str) -> Option<String> {
    r.try_get::<Option<String>>("", col).ok().flatten()
}

/// 列表行 → JSON（白名单字段，不暴露正文以外的内部列）
fn row_json(r: &Row, with_content: bool) -> Value {
    let mut v = json!({
        "id": s(r, "id"),
        "title": s(r, "title"),
        "slug": s(r, "slug"),
        "summary": s(r, "summary"),
        "author": s(r, "author"),
        "tags": s(r, "tags"),
        "featured_image": so(r, "featured_image"),
        "published_at": so(r, "published_at"),
        "meta_title": so(r, "meta_title"),
        "meta_description": so(r, "meta_description"),
        "updated_at": s(r, "updated_at"),
    });
    if with_content {
        v["content"] = json!(s(r, "content"));
    }
    v
}

/// GET /api/public/articles —— 已发布文章列表（不含正文；可选 ?tag= 过滤）
pub async fn articles(
    State(st): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> ApiResult {
    let tag = params.get("tag").map(|s| s.trim()).filter(|s| !s.is_empty());
    let mut sql = String::from(
        "SELECT id, title, slug, summary, author, tags, featured_image, \
         published_at, meta_title, meta_description, updated_at FROM articles \
         WHERE tenant_id = ? AND status = 'published'",
    );
    let mut args = vec![SqlValue::String(Some(st.tenant.clone()))];
    if let Some(t) = tag {
        sql.push_str(" AND tags LIKE ?");
        args.push(SqlValue::String(Some(format!("%{}%", t))));
    }
    sql.push_str(" ORDER BY COALESCE(published_at, updated_at) DESC LIMIT 100");
    let rows = st
        .db
        .query_all(&sql, args)
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    let items: Vec<Value> = rows.iter().map(|r| row_json(r, false)).collect();
    ok(json!(items))
}

/// GET /api/public/articles/{id} —— 单篇详情（含正文；id 或 slug 均可解析）
pub async fn article_detail(
    State(st): State<AppState>,
    Path(key): Path<String>,
) -> ApiResult {
    let sql = "SELECT id, title, slug, summary, author, tags, featured_image, \
               published_at, meta_title, meta_description, updated_at, content FROM articles \
               WHERE tenant_id = ? AND status = 'published' AND (id = ? OR slug = ?) LIMIT 1";
    let rows = st
        .db
        .query_all(
            sql,
            vec![
                SqlValue::String(Some(st.tenant.clone())),
                SqlValue::String(Some(key.clone())),
                SqlValue::String(Some(key)),
            ],
        )
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    match rows.first() {
        Some(r) => ok(row_json(r, true)),
        None => Err(ApiError::not_found("文章不存在或未发布")),
    }
}

/// GET /api/public/tags —— 标签列表（独立 Tag 管理表，含文章计数）
pub async fn tags(State(st): State<AppState>) -> ApiResult {
    let sql = "SELECT t.id, t.name, t.slug, t.description, t.cover_image, \
               (SELECT COUNT(*) FROM articles a WHERE a.tenant_id = t.tenant_id \
                AND a.status = 'published' AND a.tags LIKE '%' || t.name || '%') AS post_count \
               FROM tags t WHERE t.tenant_id = ? ORDER BY t.name COLLATE NOCASE LIMIT 200";
    let rows = st
        .db
        .query_all(sql, vec![SqlValue::String(Some(st.tenant.clone()))])
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id": s(r, "id"),
                "name": s(r, "name"),
                "slug": s(r, "slug"),
                "description": s(r, "description"),
                "cover_image": so(r, "cover_image"),
                "post_count": r.try_get::<i64>("", "post_count").unwrap_or(0),
            })
        })
        .collect();
    ok(json!(items))
}
