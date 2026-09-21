//! 出站 Webhooks（Outbound）：订阅管理 + 事件广播。
//! 与既有 automation（入站触发）互补：这里把站内事件（评论/会员/订阅/文章发布）主动
//! POST 到外部 URL，带 HMAC-SHA256 签名（X-Webhook-Signature），失败重试并记录投递。

use axum::{extract::{Path, State}, Json};
use hmac::{Hmac, Mac};
use sea_orm::Value as SqlValue;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::Sha256;
use uuid::Uuid;

use crate::{
    auth::Auth,
    db::now_iso,
    error::{ok, ok_list, ApiError, ApiResult},
    state::AppState,
};

fn sval(s: String) -> SqlValue { SqlValue::String(Some(s)) }
type HmacSha256 = Hmac<Sha256>;

#[derive(Deserialize)]
pub struct SubCreate {
    pub event: String,
    pub url: String,
    #[serde(default)]
    pub secret: Option<String>,
}

/// POST /api/webhooks —— 新建订阅
pub async fn create(State(st): State<AppState>, auth: Auth, Json(req): Json<SubCreate>) -> ApiResult {
    let _ = auth;
    if req.event.trim().is_empty() || req.url.trim().is_empty() {
        return Err(ApiError::bad("event 与 url 必填"));
    }
    let id = Uuid::new_v4().to_string();
    let now = now_iso();
    st.db
        .execute("INSERT INTO webhook_subscriptions (id, tenant_id, event, url, secret, active, created_at, updated_at) \
                 VALUES (?, ?, ?, ?, ?, 1, ?, ?)",
            vec![
                sval(id.clone()), sval(st.tenant.clone()), sval(req.event.clone()),
                sval(req.url.clone()), sval(req.secret.clone().unwrap_or_default()),
                sval(now.clone()), sval(now.clone()),
            ])
        .await
        .map_err(|e| ApiError::bad(format!("创建失败：{e}")))?;
    ok(json!({ "id": id, "event": req.event, "url": req.url }))
}

/// GET /api/webhooks —— 列表（含最近投递数）
pub async fn list(State(st): State<AppState>, auth: Auth) -> ApiResult {
    let _ = auth;
    let rows = st
        .db
        .query_all("SELECT id, event, url, active, created_at FROM webhook_subscriptions WHERE tenant_id = ? ORDER BY created_at DESC",
            vec![sval(st.tenant.clone())])
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    let mut items: Vec<Value> = Vec::new();
    for r in &rows {
        let id: String = r.try_get("", "id").map_err(internal)?;
        let deliveries: i64 = st
            .db
            .query_one("SELECT COUNT(*) AS n FROM webhook_deliveries WHERE sub_id = ?", vec![sval(id.clone())])
            .await
            .ok()
            .flatten()
            .and_then(|d| d.try_get::<i64>("", "n").ok())
            .unwrap_or(0);
        items.push(json!({
            "id": id,
            "event": r.try_get::<String>("", "event").unwrap_or_default(),
            "url": r.try_get::<String>("", "url").unwrap_or_default(),
            "active": r.try_get::<i64>("", "active").unwrap_or(1),
            "deliveries": deliveries,
            "createdAt": r.try_get::<String>("", "created_at").unwrap_or_default(),
        }));
    }
    let n = items.len();
    ok_list(items, n)
}

/// DELETE /api/webhooks/{id}
pub async fn remove(State(st): State<AppState>, auth: Auth, Path(id): Path<String>) -> ApiResult {
    let _ = auth;
    let n = st
        .db
        .execute("DELETE FROM webhook_subscriptions WHERE id = ? AND tenant_id = ?",
            vec![sval(id.clone()), sval(st.tenant.clone())])
        .await
        .map_err(|e| ApiError::bad(format!("删除失败：{e}")))?;
    if n == 0 {
        return Err(ApiError::not_found("订阅不存在"));
    }
    ok(json!({ "deleted": true }))
}

#[derive(Deserialize)]
pub struct TestReq { #[serde(default)] pub event: Option<String> }

/// POST /api/webhooks/test —— 触发一次示例事件（向匹配订阅广播）
pub async fn test(State(st): State<AppState>, auth: Auth, Json(req): Json<TestReq>) -> ApiResult {
    let _ = auth;
    let event = req.event.clone().unwrap_or_else(|| "ping".to_string());
    let sample = json!({ "source": "manual-test", "event": event, "ts": now_iso() });
    emit(&st, &event, sample.clone());
    ok(json!({ "event": event, "dispatched": true }))
}

/// 单条投递最大尝试次数（首次 + 4 次重试），超过置终态 failed
pub const MAX_ATTEMPTS: i64 = 5;

/// 广播事件：先把投递任务落库（pending outbox），再异步逐条投递。
/// 进程重启后由 scheduler 扫描 pending 行继续投递（指数退避），事件不因重启丢失。
pub fn emit(st: &AppState, event: &str, payload: Value) {
    let st = st.clone();
    let event = event.to_string();
    tokio::spawn(async move {
        let subs = st
            .db
            .query_all("SELECT id, url FROM webhook_subscriptions WHERE tenant_id = ? AND active = 1 AND (event = ? OR event = '*')",
                vec![sval(st.tenant.clone()), sval(event.clone())])
            .await;
        let Ok(subs) = subs else { return };
        let body = payload.to_string();
        for r in &subs {
            let id: String = match r.try_get("", "id") { Ok(v) => v, Err(_) => continue };
            let url: String = r.try_get("", "url").unwrap_or_default();
            if url.is_empty() { continue }
            let now = now_iso();
            let did = Uuid::new_v4().to_string();
            let inserted = st
                .db
                .execute("INSERT INTO webhook_deliveries (id, sub_id, tenant_id, event, payload, status, attempts, last_error, created_at, updated_at) \
                         VALUES (?, ?, ?, ?, ?, 'pending', 0, '', ?, ?)",
                    vec![
                        sval(did.clone()), sval(id.clone()), sval(st.tenant.clone()),
                        sval(event.clone()), sval(body.clone()),
                        sval(now.clone()), sval(now),
                    ])
                .await;
            if inserted.is_err() { continue }
            deliver_record(&st, &did).await;
        }
    });
}

/// 投递单条 outbox 记录：读行 → 按订阅 secret 签名 → POST → 回写结果。
/// 失败未达 MAX_ATTEMPTS 保持 pending，等待 scheduler 按退避重投。
pub async fn deliver_record(st: &AppState, delivery_id: &str) {
    let row = st
        .db
        .query_one(
            "SELECT d.event, d.payload, d.attempts, s.url, s.secret \
             FROM webhook_deliveries d LEFT JOIN webhook_subscriptions s ON s.id = d.sub_id \
             WHERE d.id = ?",
            vec![sval(delivery_id.to_string())],
        )
        .await
        .ok()
        .flatten();
    let Some(r) = row else { return };
    let event: String = r.try_get("", "event").unwrap_or_default();
    let body: String = r.try_get("", "payload").unwrap_or_default();
    let secret: String = r.try_get("", "secret").unwrap_or_default();
    let attempts: i64 = r.try_get("", "attempts").unwrap_or(0);
    let url: String = r.try_get("", "url").unwrap_or_default();
    let now = now_iso();

    if url.is_empty() {
        // 订阅已被删除 / URL 为空：无法投递，置终态
        let _ = st
            .db
            .execute(
                "UPDATE webhook_deliveries SET status = 'failed', last_error = ?, updated_at = ? WHERE id = ?",
                vec![
                    sval("订阅不存在或 URL 为空".into()),
                    sval(now),
                    sval(delivery_id.to_string()),
                ],
            )
            .await;
        return;
    }

    let sig = sign_payload(&secret, &body);
    let (status, last_error) = match deliver_once(&url, &event, &body, &sig).await {
        Ok(()) => ("success", String::new()),
        Err(e) if attempts + 1 >= MAX_ATTEMPTS => ("failed", e),
        Err(e) => ("pending", e),
    };
    let _ = st
        .db
        .execute(
            "UPDATE webhook_deliveries SET status = ?, attempts = ?, last_error = ?, updated_at = ? WHERE id = ?",
            vec![
                sval(status.to_string()),
                SqlValue::BigInt(Some(attempts + 1)),
                sval(last_error),
                sval(now_iso()),
                sval(delivery_id.to_string()),
            ],
        )
        .await;
}

/// 退避间隔：第 n 次重试前等待 2^(n-1) 分钟（1/2/4/8 分钟）。
fn backoff_secs(attempts: i64) -> i64 {
    60 * (1i64 << (attempts - 1).clamp(0, 3))
}

/// scheduler 每分钟调用：重投到期的 pending 记录。
/// attempts=0 是 emit 的在途任务（进程崩溃遗留 5 分钟后接管）；attempts>0 按退避判定。
pub async fn retry_due(st: &AppState) -> usize {
    let rows = st
        .db
        .query_all(
            "SELECT id, attempts, created_at, updated_at FROM webhook_deliveries \
             WHERE status = 'pending' AND attempts < 5",
            vec![],
        )
        .await
        .unwrap_or_default();
    let now = chrono::Utc::now();
    let mut n = 0usize;
    for r in &rows {
        let id: String = r.try_get("", "id").unwrap_or_default();
        if id.is_empty() { continue }
        let attempts: i64 = r.try_get("", "attempts").unwrap_or(0);
        let stamp: String = if attempts > 0 {
            r.try_get("", "updated_at").unwrap_or_default()
        } else {
            r.try_get("", "created_at").unwrap_or_default()
        };
        let wait = if attempts > 0 { backoff_secs(attempts) } else { 300 };
        let due = chrono::DateTime::parse_from_rfc3339(&stamp)
            .map(|t| now - t.with_timezone(&chrono::Utc) >= chrono::Duration::seconds(wait))
            .unwrap_or(true); // 时间戳异常视为到期
        if !due { continue }
        deliver_record(st, &id).await;
        n += 1;
    }
    n
}

async fn deliver_once(url: &str, event: &str, body: &str, sig: &str) -> Result<(), String> {
    // 10s 总超时 + 5s 连接超时：避免慢端点拖垮投递循环
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .connect_timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap_or_default();
    let mut last_err = String::new();
    for _ in 0..3 {
        let resp = client
            .post(url)
            .header("Content-Type", "application/json")
            .header("X-Webhook-Signature", format!("sha256={sig}"))
            .header("X-Webhook-Event", event)
            .body(body.to_string())
            .send()
            .await;
        match resp {
            Ok(r) if r.status().is_success() => return Ok(()),
            Ok(r) => last_err = format!("HTTP {}", r.status()),
            Err(e) => last_err = format!("{e}"),
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    Err(last_err)
}

fn sign_payload(secret: &str, body: &str) -> String {
    if secret.is_empty() {
        return String::new();
    }
    let mut mac = match HmacSha256::new_from_slice(secret.as_bytes()) {
        Ok(m) => m,
        Err(_) => return String::new(),
    };
    mac.update(body.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

fn internal(e: impl std::fmt::Display) -> ApiError {
    ApiError::bad(format!("行解析失败：{e}"))
}
