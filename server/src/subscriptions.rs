//! 付费订阅（Subscriptions）：Stripe Checkout 接入 + Webhook 落库 + 内容门槛。
//! Stripe 通过环境变量 STRIPE_SECRET_KEY / STRIPE_WEBHOOK_SECRET 配置；未配置进入测试模式。

use axum::{extract::{Path, State}, Json};
use hmac::{Hmac, Mac};
use sea_orm::Value as SqlValue;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::Sha256;

use crate::{
    members::MemberAuth,
    error::{ok, ApiError, ApiResult},
    state::AppState,
};

fn sval(s: String) -> SqlValue { SqlValue::String(Some(s)) }

type HmacSha256 = Hmac<Sha256>;

/// GET /api/public/tiers —— 公开套餐列表
pub async fn tiers(State(st): State<AppState>) -> ApiResult {
    let rows = st
        .db
        .query_all(
            "SELECT id, name, slug, description, price_monthly, price_yearly, features, active \
             FROM tiers WHERE tenant_id = ? AND active = 1 ORDER BY price_monthly ASC",
            vec![sval(st.tenant.clone())],
        )
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    let mut items: Vec<Value> = Vec::new();
    for r in &rows {
        items.push(json!({
            "id": r.try_get::<String>("", "id").map_err(internal)?,
            "name": r.try_get::<String>("", "name").unwrap_or_default(),
            "slug": r.try_get::<String>("", "slug").unwrap_or_default(),
            "description": r.try_get::<String>("", "description").unwrap_or_default(),
            "priceMonthly": r.try_get::<f64>("", "price_monthly").unwrap_or(0.0),
            "priceYearly": r.try_get::<f64>("", "price_yearly").unwrap_or(0.0),
            "features": r.try_get::<String>("", "features").unwrap_or_else(|_| "[]".into()),
            "active": r.try_get::<i64>("", "active").unwrap_or(1),
        }));
    }
    ok(json!(items))
}

#[derive(Deserialize)]
pub struct CheckoutReq {
    pub tier_id: String,
    #[serde(default)]
    pub interval: Option<String>,
}

/// POST /api/public/checkout —— 会员创建 Stripe Checkout 会话
pub async fn checkout(State(st): State<AppState>, auth: MemberAuth, Json(req): Json<CheckoutReq>) -> ApiResult {
    let row = st
        .db
        .query_one("SELECT id, name, slug, stripe_price_id, price_monthly FROM tiers WHERE id = ? AND tenant_id = ? AND active = 1 LIMIT 1",
            vec![sval(req.tier_id.clone()), sval(st.tenant.clone())])
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    let Some(r) = row else { return Err(ApiError::not_found("套餐不存在")) };
    let tier_id: String = r.try_get("", "id").map_err(internal)?;
    let tier_slug: String = r.try_get("", "slug").unwrap_or_default();
    let price_id: String = r.try_get("", "stripe_price_id").unwrap_or_default();

    let sk = std::env::var("STRIPE_SECRET_KEY");
    match sk {
        Ok(key) if !key.is_empty() && !price_id.is_empty() => {
            let client = reqwest::Client::new();
            let success = format!("{}/membership?thank=1", public_base(&st));
            let cancel = format!("{}/membership", public_base(&st));
            let params = [
                ("mode", "subscription".to_string()),
                ("client_reference_id", auth.0.sub.clone()),
                ("success_url", success),
                ("cancel_url", cancel),
                ("line_items[0][price]", price_id),
                ("line_items[0][quantity]", "1".to_string()),
            ];
            let resp = client
                .post("https://api.stripe.com/v1/checkout/sessions")
                .basic_auth(&key, None::<&str>)
                .form(&params)
                .send()
                .await
                .map_err(|e| ApiError::bad(format!("Stripe 请求失败：{e}")))?;
            if !resp.status().is_success() {
                let txt = resp.text().await.unwrap_or_default();
                return Err(ApiError::bad(format!("Stripe 错误：{txt}")));
            }
            let j: Value = resp.json().await.map_err(|e| ApiError::bad(format!("解析失败：{e}")))?;
            let url = j.get("url").and_then(|u| u.as_str()).unwrap_or("").to_string();
            crate::webhooks_out::emit(&st, "subscription.checkout", json!({ "tierId": tier_id, "memberId": auth.0.sub }));
            ok(json!({ "url": url, "testMode": false }))
        }
        _ => {
            // 测试模式：不真实扣费，直接标记为已订阅（仅演示）
            st.db
                .execute("UPDATE members SET plan = ?, status = 1 WHERE id = ? AND tenant_id = ?",
                    vec![sval(tier_slug.clone()), sval(auth.0.sub.clone()), sval(st.tenant.clone())])
                .await
                .map_err(|e| ApiError::bad(format!("更新失败：{e}")))?;
            crate::webhooks_out::emit(&st, "subscription.activated", json!({ "tier": tier_slug, "memberId": auth.0.sub }));
            ok(json!({ "url": format!("{}/membership?thank=1&demo=1", public_base(&st)), "testMode": true }))
        }
    }
}

/// POST /api/stripe/webhook —— Stripe 事件回调（校验签名后落库）
pub async fn stripe_webhook(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    body: String,
) -> ApiResult {
    let secret = std::env::var("STRIPE_WEBHOOK_SECRET").unwrap_or_default();
    if !secret.is_empty() {
        let sig = headers.get("stripe-signature").and_then(|v| v.to_str().ok()).unwrap_or("");
        if !verify_stripe_sig(&secret, sig, &body) {
            return Err(ApiError::unauthorized("签名校验失败"));
        }
    }
    let evt: Value = serde_json::from_str(&body).map_err(|e| ApiError::bad(format!("JSON 解析失败：{e}")))?;
    if evt.get("type").and_then(|t| t.as_str()) == Some("checkout.session.completed") {
        let member_id = evt
            .pointer("/data/object/client_reference_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if !member_id.is_empty() {
            st.db
                .execute("UPDATE members SET status = 1 WHERE id = ? AND tenant_id = ?",
                    vec![sval(member_id.clone()), sval(st.tenant.clone())])
                .await
                .ok();
            crate::webhooks_out::emit(&st, "subscription.activated", json!({ "memberId": member_id }));
        }
    }
    ok(json!({ "received": true }))
}

/// Stripe 签名校验：HMAC-SHA256("{t}.{body}", secret) == v1
fn verify_stripe_sig(secret: &str, sig_header: &str, body: &str) -> bool {
    let t_val = match sig_header.strip_prefix("t=") {
        Some(rest) => rest.split(',').next().unwrap_or("").to_string(),
        None => return false,
    };
    let v1_val = match sig_header.split(',').find_map(|p| p.strip_prefix("v1=")) {
        Some(v) => v.to_string(),
        None => return false,
    };
    let mut mac = match HmacSha256::new_from_slice(secret.as_bytes()) {
        Ok(m) => m,
        Err(_) => return false,
    };
    mac.update(format!("{t_val}.{body}").as_bytes());
    let expected = hex::encode(mac.finalize().into_bytes());
    constant_time_eq(&expected, &v1_val)
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() { return false; }
    let mut diff = 0u8;
    for (x, y) in a.bytes().zip(b.bytes()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn public_base(st: &AppState) -> String {
    std::env::var("PUBLIC_BASE_URL").unwrap_or_else(|_| "http://localhost:5188".into())
}

fn internal(e: impl std::fmt::Display) -> ApiError {
    ApiError::bad(format!("行解析失败：{e}"))
}
