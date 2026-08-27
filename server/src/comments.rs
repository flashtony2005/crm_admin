//! 评论（Comments）：公开列表 / 发布（支持嵌套 parent_id）；Admin 审核与状态管理。

use axum::{extract::{Path, Query, State}, Json};
use sea_orm::Value as SqlValue;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use uuid::Uuid;

use crate::{
    auth::Auth,
    db::now_iso,
    error::{ok, ok_list, ApiError, ApiResult},
    state::AppState,
};

fn sval(s: String) -> SqlValue { SqlValue::String(Some(s)) }

#[derive(Deserialize)]
pub struct CommentCreate {
    pub article_id: String,
    #[serde(default)]
    pub parent_id: Option<String>,
    pub author_name: String,
    #[serde(default)]
    pub author_email: Option<String>,
    pub content: String,
}

/// POST /api/public/comments —— 公开发布评论（默认 approved；可接审核流）
pub async fn public_create(State(st): State<AppState>, Json(req): Json<CommentCreate>) -> ApiResult {
    let name = req.author_name.trim().to_string();
    let content = req.content.trim().to_string();
    if name.is_empty() || content.is_empty() {
        return Err(ApiError::bad("昵称与内容必填"));
    }
    if content.len() > 2000 {
        return Err(ApiError::bad("评论过长（上限 2000 字）"));
    }
    let id = Uuid::new_v4().to_string();
    let now = now_iso();
    let auto = std::env::var("COMMENTS_AUTO_APPROVE").map(|v| v == "1" || v.eq_ignore_ascii_case("true")).unwrap_or(true);
    let status = if auto { "approved" } else { "pending" };
    st.db
        .execute(
            "INSERT INTO comments (id, tenant_id, article_id, parent_id, author_name, author_email, member_id, content, status, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, '', ?, ?, ?)",
            vec![
                sval(id.clone()),
                sval(st.tenant.clone()),
                sval(req.article_id.clone()),
                sval(req.parent_id.clone().unwrap_or_default()),
                sval(name.clone()),
                sval(req.author_email.clone().unwrap_or_default()),
                sval(content.clone()),
                sval(status.to_string()),
                sval(now.clone()),
            ],
        )
        .await
        .map_err(|e| ApiError::bad(format!("发布失败：{e}")))?;
    // 出站 Webhook
    crate::webhooks_out::emit(&st, "comment.created", json!({
        "id": id, "articleId": req.article_id, "author": name, "status": status
    }));
    ok(json!({ "id": id, "status": status }))
}

/// GET /api/public/comments?article=xxx —— 公开已审核评论（嵌套）
pub async fn public_list(
    State(st): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> ApiResult {
    let article = params.get("article").cloned().unwrap_or_default();
    if article.is_empty() {
        return Err(ApiError::bad("article 必填"));
    }
    let rows = st
        .db
        .query_all(
            "SELECT id, article_id, parent_id, author_name, content, status, created_at \
             FROM comments WHERE tenant_id = ? AND article_id = ? AND status = 'approved' \
             ORDER BY created_at ASC",
            vec![sval(st.tenant.clone()), sval(article)],
        )
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    let mut items: Vec<Value> = Vec::new();
    for r in &rows {
        let c = json!({
            "id": r.try_get::<String>("", "id").map_err(internal)?,
            "articleId": r.try_get::<String>("", "article_id").unwrap_or_default(),
            "parentId": r.try_get::<String>("", "parent_id").unwrap_or_default(),
            "authorName": r.try_get::<String>("", "author_name").unwrap_or_default(),
            "content": r.try_get::<String>("", "content").unwrap_or_default(),
            "status": r.try_get::<String>("", "status").unwrap_or_default(),
            "createdAt": r.try_get::<String>("", "created_at").unwrap_or_default(),
        });
        items.push(c);
    }
    ok(json!(items))
}

/// GET /api/comments?status=pending —— Admin 审核列表
pub async fn admin_list(
    State(st): State<AppState>,
    auth: Auth,
    Query(params): Query<HashMap<String, String>>,
) -> ApiResult {
    let _ = auth;
    let status = params.get("status").cloned().unwrap_or_default();
    let (sql, args) = if status.is_empty() {
        ("SELECT id, article_id, parent_id, author_name, author_email, content, status, created_at \
          FROM comments WHERE tenant_id = ? ORDER BY created_at DESC LIMIT 200"
            .to_string(),
         vec![sval(st.tenant.clone())])
    } else {
        ("SELECT id, article_id, parent_id, author_name, author_email, content, status, created_at \
          FROM comments WHERE tenant_id = ? AND status = ? ORDER BY created_at DESC LIMIT 200"
            .to_string(),
         vec![sval(st.tenant.clone()), sval(status)])
    };
    let rows = st
        .db
        .query_all(&sql, args)
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    let mut items: Vec<Value> = Vec::new();
    for r in &rows {
        items.push(json!({
            "id": r.try_get::<String>("", "id").map_err(internal)?,
            "articleId": r.try_get::<String>("", "article_id").unwrap_or_default(),
            "parentId": r.try_get::<String>("", "parent_id").unwrap_or_default(),
            "authorName": r.try_get::<String>("", "author_name").unwrap_or_default(),
            "authorEmail": r.try_get::<String>("", "author_email").unwrap_or_default(),
            "content": r.try_get::<String>("", "content").unwrap_or_default(),
            "status": r.try_get::<String>("", "status").unwrap_or_default(),
            "createdAt": r.try_get::<String>("", "created_at").unwrap_or_default(),
        }));
    }
    let n = items.len();
    ok_list(items, n)
}

#[derive(Deserialize)]
pub struct ModerateReq { pub status: String }

/// POST /api/comments/{id}/status —— Admin 审核（approve/reject/spam）
pub async fn moderate(
    State(st): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
    Json(req): Json<ModerateReq>,
) -> ApiResult {
    let _ = auth;
    let valid = ["approved", "pending", "rejected", "spam"];
    if !valid.contains(&req.status.as_str()) {
        return Err(ApiError::bad("非法状态"));
    }
    let n = st
        .db
        .execute(
            "UPDATE comments SET status = ? WHERE id = ? AND tenant_id = ?",
            vec![sval(req.status.clone()), sval(id.clone()), sval(st.tenant.clone())],
        )
        .await
        .map_err(|e| ApiError::bad(format!("更新失败：{e}")))?;
    if n == 0 {
        return Err(ApiError::not_found("评论不存在"));
    }
    crate::webhooks_out::emit(&st, "comment.moderated", json!({ "id": id, "status": req.status }));
    ok(json!({ "updated": true }))
}

fn internal(e: impl std::fmt::Display) -> ApiError {
    ApiError::bad(format!("行解析失败：{e}"))
}
