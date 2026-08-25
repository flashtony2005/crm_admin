//! 公开内容 API（免认证，只读）：
//! - GET /api/public/articles       已发布文章列表（不含正文，便于列表聚合）
//! - GET /api/public/articles/{id}  单篇详情（含正文）
//!
//! 仅返回 status='published' 且属于当前租户的文章；供公开站点前端 /
//! Jamstack / 第三方应用消费（headless 用法）。响应沿用统一信封
//! {ok:true,data}，与前端 api()/apiList() 约定一致。
//!
//! 部署：新增模块，本地 `cargo build` 后随新二进制生效。

use axum::extract::{Path, State};
use sea_orm::Value as SqlValue;
use serde_json::{json, Value};

use crate::error::{ok, ApiError, ApiResult};
use crate::state::AppState;

/// 读 TEXT 列（NULL→空串）
fn s(r: &sea_orm::QueryResult, col: &str) -> String {
    r.try_get::<String>("", col).unwrap_or_default()
}

/// 读可空 TEXT 列
fn so(r: &sea_orm::QueryResult, col: &str) -> Option<String> {
    r.try_get::<Option<String>>("", col).ok().flatten()
}

/// 列表行 → JSON（白名单字段，不暴露正文以外的内部列）
fn row_json(r: &sea_orm::QueryResult, with_content: bool) -> Value {
    let mut v = json!({
        "id": s(r, "id"),
        "title": s(r, "title"),
        "summary": s(r, "summary"),
        "author": s(r, "author"),
        "tags": s(r, "tags"),
        "featured_image": so(r, "featured_image"),
        "published_at": so(r, "published_at"),
        "updated_at": s(r, "updated_at"),
    });
    if with_content {
        v["content"] = json!(s(r, "content"));
    }
    v
}

/// GET /api/public/articles —— 已发布文章列表（不含正文）
pub async fn articles(State(st): State<AppState>) -> ApiResult {
    let sql = "SELECT id, title, summary, author, tags, featured_image, \
               published_at, updated_at FROM articles \
               WHERE tenant_id = ? AND status = 'published' \
               ORDER BY COALESCE(published_at, updated_at) DESC LIMIT 100";
    let rows = st
        .db
        .query_all(sql, vec![SqlValue::String(Some(st.tenant.clone()))])
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    let items: Vec<Value> = rows.iter().map(|r| row_json(r, false)).collect();
    Ok(ok(json!(items)))
}

/// GET /api/public/articles/{id} —— 单篇详情（含正文）
pub async fn article_detail(
    State(st): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult {
    let sql = "SELECT id, title, summary, author, tags, featured_image, \
               published_at, updated_at, content FROM articles \
               WHERE tenant_id = ? AND status = 'published' AND id = ? LIMIT 1";
    let rows = st
        .db
        .query_all(
            sql,
            vec![
                SqlValue::String(Some(st.tenant.clone())),
                SqlValue::String(Some(id)),
            ],
        )
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    match rows.first() {
        Some(r) => Ok(ok(row_json(r, true))),
        None => Err(ApiError::not_found("文章不存在或未发布")),
    }
}
