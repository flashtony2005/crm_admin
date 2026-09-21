//! 审计与请求追踪（P2）：每个请求响应携带 X-Request-Id（优先沿用上游传入）；
//! /api/ 下全部写操作（POST/PUT/PATCH/DELETE）落 audit_log 留痕 ——
//! 覆盖登录/注册/下单/确认收款/微信回调等敏感操作，供安全审计与问题回溯。

use axum::{
    extract::{Query, Request, State},
    http::{header, HeaderName, HeaderValue, Method},
    middleware::Next,
    response::Response,
};
use sea_orm::Value as SqlValue;
use serde_json::{json, Value};

use crate::{
    auth::{self, Auth}, db::now_iso,
    error::{ok_list, ApiError, ApiResult},
    state::AppState,
};

fn sval(s: String) -> SqlValue {
    SqlValue::String(Some(s))
}
fn ival(v: i64) -> SqlValue {
    SqlValue::BigInt(Some(v))
}

/// 复用上游 request-id（便于网关串联追踪），否则生成 UUID；长度上限 64。
fn request_id(req: &Request) -> String {
    req.headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s.chars().count() <= 64)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string())
}

/// 中间件：分配 request-id + 写操作审计留痕。
/// 身份仅做「尽力解析」（验签成功才归因，失败即为匿名），不影响请求本身。
pub async fn middleware(State(st): State<AppState>, req: Request, next: Next) -> Response {
    let rid = request_id(&req);
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let (user_id, username) = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .and_then(auth::verify)
        .map(|c| (c.sub, c.username))
        .unwrap_or_default();

    let start = std::time::Instant::now();
    let mut resp = next.run(req).await;
    let duration_ms = start.elapsed().as_millis() as i64;

    if let Ok(hv) = HeaderValue::from_str(&rid) {
        resp.headers_mut().insert(HeaderName::from_static("x-request-id"), hv);
    }

    let is_mutation =
        matches!(method, Method::POST | Method::PUT | Method::PATCH | Method::DELETE);
    if is_mutation && path.starts_with("/api/") {
        let status = resp.status().as_u16() as i64;
        let r = st
            .db
            .execute(
                "INSERT INTO audit_log (id, tenant_id, user_id, username, method, path, status, duration_ms, request_id, created_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                vec![
                    sval(uuid::Uuid::new_v4().to_string()),
                    sval(st.tenant.clone()),
                    sval(user_id),
                    sval(username),
                    sval(method.to_string()),
                    sval(path),
                    ival(status),
                    ival(duration_ms),
                    sval(rid),
                    sval(now_iso()),
                ],
            )
            .await;
        if let Err(e) = r {
            eprintln!("[audit] 写入失败：{e}");
        }
    }
    resp
}

/// GET /api/admin/audit?page=&pageSize= —— 审计日志查询（team.users.update 权限，通常仅 Owner）
pub async fn list(
    State(st): State<AppState>,
    auth: Auth,
    Query(filters): Query<std::collections::HashMap<String, String>>,
) -> ApiResult {
    auth::ensure(&auth, "team.users.update")?;
    let mut filters = filters;
    let page = filters
        .remove("page")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(1)
        .max(1) as i64;
    let page_size = filters
        .remove("pageSize")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(50)
        .clamp(1, 200) as i64;

    let total: i64 = st
        .db
        .query_one(
            "SELECT COUNT(*) AS n FROM audit_log WHERE tenant_id = ?",
            vec![sval(st.tenant.clone())],
        )
        .await
        .ok()
        .flatten()
        .and_then(|r| r.try_get::<i64>("", "n").ok())
        .unwrap_or(0);
    let rows = st
        .db
        .query_all(
            "SELECT user_id, username, method, path, status, duration_ms, request_id, created_at \
             FROM audit_log WHERE tenant_id = ? ORDER BY created_at DESC LIMIT ? OFFSET ?",
            vec![
                sval(st.tenant.clone()),
                ival(page_size),
                ival((page - 1) * page_size),
            ],
        )
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;

    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            Ok(json!({
                "userId": r.try_get::<String>("", "user_id").unwrap_or_default(),
                "username": r.try_get::<String>("", "username").unwrap_or_default(),
                "method": r.try_get::<String>("", "method").unwrap_or_default(),
                "path": r.try_get::<String>("", "path").unwrap_or_default(),
                "status": r.try_get::<i64>("", "status").unwrap_or(0),
                "durationMs": r.try_get::<i64>("", "duration_ms").unwrap_or(0),
                "requestId": r.try_get::<String>("", "request_id").unwrap_or_default(),
                "createdAt": r.try_get::<String>("", "created_at").unwrap_or_default(),
            }))
        })
        .collect::<Result<_, ApiError>>()?;
    ok_list(items, total as usize)
}
