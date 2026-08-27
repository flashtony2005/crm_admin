//! 邮件订阅（Newsletter）：公开订阅 / 退订；Admin 订阅者管理与真实 SMTP 群发。
//! SMTP 通过环境变量配置（SMTP_HOST/PORT/USER/PASS/FROM），未配置时进入测试模式并返回提示，
//! 配置后即为真实集成（lettre + rustls-tls）。

use axum::{extract::{Query, State}, Json};
use lettre::message::{header, Mailbox, MultiPart, SinglePart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SmtpTransport, Transport};
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
pub struct SubscribeReq {
    pub email: String,
    #[serde(default)]
    pub name: Option<String>,
}

/// POST /api/public/newsletter/subscribe
pub async fn subscribe(State(st): State<AppState>, Json(req): Json<SubscribeReq>) -> ApiResult {
    let email = req.email.trim().to_lowercase();
    if !email.contains('@') {
        return Err(ApiError::bad("邮箱格式不正确"));
    }
    let name = req.name.clone().unwrap_or_default();
    let exist = st
        .db
        .query_one("SELECT id FROM subscribers WHERE tenant_id = ? AND email = ? LIMIT 1",
            vec![sval(st.tenant.clone()), sval(email.clone())])
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    if let Some(r) = exist {
        let id: String = r.try_get("", "id").map_err(internal)?;
        st.db
            .execute("UPDATE subscribers SET status = 'active' WHERE id = ?", vec![sval(id)])
            .await
            .ok();
    } else {
        st.db
            .execute("INSERT INTO subscribers (id, tenant_id, email, name, status, created_at) VALUES (?, ?, ?, ?, 'active', ?)",
                vec![sval(Uuid::new_v4().to_string()), sval(st.tenant.clone()), sval(email.clone()), sval(name.clone()), sval(now_iso())])
            .await
            .map_err(|e| ApiError::bad(format!("订阅失败：{e}")))?;
    }
    crate::webhooks_out::emit(&st, "newsletter.subscribed", json!({ "email": email, "name": name }));
    ok(json!({ "subscribed": true }))
}

/// GET /api/public/newsletter/unsubscribe?email=
pub async fn unsubscribe(
    State(st): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> ApiResult {
    let email = params.get("email").cloned().unwrap_or_default().trim().to_lowercase();
    if email.is_empty() {
        return Err(ApiError::bad("email 必填"));
    }
    st.db
        .execute("UPDATE subscribers SET status = 'unsubscribed' WHERE tenant_id = ? AND email = ?",
            vec![sval(st.tenant.clone()), sval(email)])
        .await
        .map_err(|e| ApiError::bad(format!("退订失败：{e}")))?;
    ok(json!({ "unsubscribed": true }))
}

/// GET /api/newsletter/subscribers —— Admin 列表
pub async fn admin_list(State(st): State<AppState>, auth: Auth) -> ApiResult {
    let _ = auth;
    let rows = st
        .db
        .query_all("SELECT id, email, name, status, created_at FROM subscribers WHERE tenant_id = ? ORDER BY created_at DESC LIMIT 500",
            vec![sval(st.tenant.clone())])
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    let mut items: Vec<Value> = Vec::new();
    for r in &rows {
        items.push(json!({
            "id": r.try_get::<String>("", "id").map_err(internal)?,
            "email": r.try_get::<String>("", "email").unwrap_or_default(),
            "name": r.try_get::<String>("", "name").unwrap_or_default(),
            "status": r.try_get::<String>("", "status").unwrap_or_default(),
            "createdAt": r.try_get::<String>("", "created_at").unwrap_or_default(),
        }));
    }
    let n = items.len();
    ok_list(items, n)
}

#[derive(Deserialize)]
pub struct CampaignReq {
    pub subject: String,
    pub body: String,
}

/// POST /api/newsletter/send —— Admin 群发（真实 SMTP）
pub async fn send(State(st): State<AppState>, auth: Auth, Json(req): Json<CampaignReq>) -> ApiResult {
    let _ = auth;
    if req.subject.trim().is_empty() || req.body.trim().is_empty() {
        return Err(ApiError::bad("主题与正文必填"));
    }
    let rows = st
        .db
        .query_all("SELECT email, name FROM subscribers WHERE tenant_id = ? AND status = 'active'",
            vec![sval(st.tenant.clone())])
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    let from = std::env::var("SMTP_FROM").unwrap_or_else(|_| "noreply@example.com".into());
    let mut delivered = 0u32;
    let mut failed = 0u32;
    for r in &rows {
        let email: String = r.try_get("", "email").unwrap_or_default();
        let name: String = r.try_get("", "name").unwrap_or_default();
        let html = render_campaign(&req.body, &name);
        match deliver_smtp(&from, &email, &req.subject, &html) {
            Ok(_) => delivered += 1,
            Err(e) => {
                failed += 1;
                eprintln!("[newsletter] 发送至 {email} 失败：{e}");
            }
        }
    }
    ok(json!({
        "total": (delivered + failed),
        "delivered": delivered,
        "failed": failed,
        "testMode": std::env::var("SMTP_HOST").is_err(),
    }))
}

fn render_campaign(body: &str, name: &str) -> String {
    format!(
        "<div style=\"font-family:system-ui,sans-serif;max-width:640px;margin:0 auto\">\
         <p>你好，{name}：</p>{body}\
         <hr/><p style=\"color:#888;font-size:12px\">你收到此邮件是因为订阅了我们的资讯。\
         <a href=\"{{unsubscribe_url}}\">退订</a></p></div>"
    )
}

/// 真实 SMTP 发送（未配置 SMTP_HOST 时返回测试模式错误）
fn deliver_smtp(from: &str, to: &str, subject: &str, html: &str) -> Result<(), String> {
    let host = std::env::var("SMTP_HOST").map_err(|_| "SMTP 未配置（测试模式）".to_string())?;
    let port: u16 = std::env::var("SMTP_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(587);
    let user = std::env::var("SMTP_USER").unwrap_or_default();
    let pass = std::env::var("SMTP_PASS").unwrap_or_default();
    let from_mb: Mailbox = from.parse().map_err(|e| format!("from 解析失败:{e}"))?;
    let to_mb: Mailbox = to.parse().map_err(|e| format!("to 解析失败:{e}"))?;
    let msg = Message::builder()
        .from(from_mb)
        .to(to_mb)
        .subject(subject)
        .multipart(MultiPart::alternative().singlepart(SinglePart::html(html.to_string())))
        .map_err(|e| format!("构建邮件失败:{e}"))?;
    let creds = Credentials::new(user, pass);
    let builder = if port == 465 {
        SmtpTransport::relay(&host).map_err(|e| e.to_string())?
    } else {
        SmtpTransport::starttls_relay(&host).map_err(|e| e.to_string())?
    };
    let mailer = builder.port(port).credentials(creds).build();
    mailer.send(&msg).map(|_| ()).map_err(|e| format!("发送失败:{e}"))
}

fn internal(e: impl std::fmt::Display) -> ApiError {
    ApiError::bad(format!("行解析失败：{e}"))
}
