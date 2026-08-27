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
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
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

/// 由托管上传 URL（形如 /uploads/<uuid>.<ext> 或 <PUBLIC_BASE_URL>/uploads/<uuid>.<ext>）
/// 推导响应式 srcset 字符串（与 upload.rs 生成的 480/960/1600 变体命名一致）。
/// 非托管上传（外链 / data URL）返回 None。
fn build_srcset(url: &str) -> Option<String> {
    let fname = url.rsplit('/').next()?;
    let (stem, ext) = fname.rsplit_once('.')?;
    if !matches!(ext.to_ascii_lowercase().as_str(), "jpg" | "jpeg" | "png" | "gif" | "webp") {
        return None;
    }
    let head = &url[..url.len() - fname.len()];
    let widths = [480u32, 960, 1600];
    let parts: Vec<String> = widths.iter().map(|w| format!("{head}{stem}_{w}.{ext} {w}w")).collect();
    Some(parts.join(", "))
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
        "locale": so(r, "locale"),
        "visibility": so(r, "visibility"),
        "featured": r.try_get::<i64>("", "featured").unwrap_or(0) != 0,
        "scheduled_at": so(r, "scheduled_at"),
        "canonical_url": so(r, "canonical_url"),
        "updated_at": s(r, "updated_at"),
    });
    if with_content {
        v["content"] = json!(s(r, "content"));
    }
    // 封面图若为本站托管上传，附带响应式 srcset（供前端 <img srcSet>）
    if let Some(img) = v.get("featured_image").and_then(|x| x.as_str()) {
        if !img.is_empty() {
            if let Some(ss) = build_srcset(img) {
                v["featured_image_srcset"] = json!(ss);
            }
        }
    }
    v
}

/// 付费墙拦截：返回 HTTP 402 + 结构化 `locked` 体（含可见元数据预览，不含正文），
/// 供前端渲染「解锁」区块。`visibility` 标明门槛类型（members / paid）。
fn locked(visibility: &str, message: impl Into<String>, preview: Value) -> ApiResult {
    Ok((
        StatusCode::PAYMENT_REQUIRED,
        Json(json!({
            "ok": false,
            "locked": true,
            "visibility": visibility,
            "error": message.into(),
            "preview": preview,
        })),
    )
        .into_response())
}

/// GET /api/public/articles —— 已发布文章列表（不含正文；可选 ?tag= / ?locale= 过滤）
pub async fn articles(
    State(st): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> ApiResult {
    let tag = params.get("tag").map(|s| s.trim()).filter(|s| !s.is_empty());
    let locale = params.get("locale").map(|s| s.trim()).filter(|s| !s.is_empty() && *s != "all");
    let author = params.get("author").map(|s| s.trim()).filter(|s| !s.is_empty());
    let featured_only = params.get("featured").map(|s| s == "1").unwrap_or(false);
    let mut sql = String::from(
        "SELECT id, title, slug, summary, author, tags, featured_image, \
         published_at, meta_title, meta_description, locale, visibility, featured, scheduled_at, \
         canonical_url, updated_at FROM articles \
         WHERE tenant_id = ? AND status = 'published'",
    );
    let mut args = vec![SqlValue::String(Some(st.tenant.clone()))];
    if let Some(t) = tag {
        sql.push_str(" AND tags LIKE ?");
        args.push(SqlValue::String(Some(format!("%{}%", t))));
    }
    if let Some(l) = locale {
        sql.push_str(" AND locale = ?");
        args.push(SqlValue::String(Some(l.to_string())));
    }
    if let Some(a) = author {
        sql.push_str(" AND author = ?");
        args.push(SqlValue::String(Some(a.to_string())));
    }
    if featured_only {
        sql.push_str(" AND featured = 1");
    }
    sql.push_str(" ORDER BY featured DESC, COALESCE(published_at, updated_at) DESC LIMIT 100");
    let rows = st
        .db
        .query_all(&sql, args)
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    let items: Vec<Value> = rows.iter().map(|r| row_json(r, false)).collect();
    ok(json!(items))
}

/// GET /api/public/articles/{id} —— 单篇详情（含正文；id 或 slug 均可解析）
/// 含可见性门槛：visibility='members' 需会员令牌；'paid' 需已订阅会员。
pub async fn article_detail(
    State(st): State<AppState>,
    headers: HeaderMap,
    Path(key): Path<String>,
) -> ApiResult {
    let sql = "SELECT id, title, slug, summary, author, tags, featured_image, \
               published_at, meta_title, meta_description, updated_at, content, visibility, locale, \
               featured, scheduled_at, canonical_url FROM articles \
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
    let Some(r) = rows.first() else {
        return Err(ApiError::not_found("文章不存在或未发布"));
    };
    let visibility: String = r.try_get("", "visibility").unwrap_or_else(|_| "public".into());
    if visibility != "public" {
        // 解析可选会员令牌（Bearer header，与管理员 auth_token 区分）
        let token = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "));
        let claims = token.and_then(|t| crate::auth::verify(t));
        // 会员（role=member）或管理员（role=admin，作者预览自有内容）可越过会员门槛
        let role_ok = claims.as_ref().map(|c| c.role == "member" || c.role == "admin").unwrap_or(false);
        if !role_ok {
            return locked(&visibility, "该内容需要会员登录后访问", row_json(r, false));
        }
        if visibility == "paid" {
            // 付费门槛：管理员可预览；会员需已订阅（plan != free）
            let is_admin = claims.as_ref().map(|c| c.role == "admin").unwrap_or(false);
            if !is_admin {
                let m = st
                    .db
                    .query_one(
                        "SELECT plan FROM members WHERE id = ? AND tenant_id = ? LIMIT 1",
                        vec![
                            SqlValue::String(Some(claims.as_ref().unwrap().sub.clone())),
                            SqlValue::String(Some(st.tenant.clone())),
                        ],
                    )
                    .await
                    .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
                let plan: String = m
                    .and_then(|x| x.try_get::<String>("", "plan").ok())
                    .unwrap_or_else(|| "free".into());
                if plan == "free" {
                    return locked(&visibility, "该内容需要付费会员订阅才能解锁", row_json(r, false));
                }
            }
        }
    }
    ok(row_json(r, true))
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
