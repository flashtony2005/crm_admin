//! 微信支付 API v3（Native 扫码支付）对接 —— P2 在线支付渠道。
//!
//! 凭据存 site_settings（后台「订单 → 微信支付配置」维护）：
//! - wechat_pay_appid            公众号/小程序 appid
//! - wechat_pay_mchid            商户号
//! - wechat_pay_api_v3_key       APIv3 密钥（32 字节，用于回调解密）
//! - wechat_pay_serial_no        商户 API 证书序列号（请求签名头用）
//! - wechat_pay_private_key      商户私钥 PEM（PKCS#8 或 PKCS#1）
//! - wechat_pay_platform_pub_key 微信支付平台公钥 PEM（可选；配置后回调强制验签）
//!
//! 安全说明：回调验签需要平台公钥/证书体系。未配置平台公钥时跳过验签，
//! 但回调体 resource.ciphertext 只能用 APIv3 密钥做 AES-256-GCM 解密成功，
//! 攻击者无密钥无法构造有效回调，实践中已具备防伪造能力。
//!
//! 金额单位：分（cents）。out_trade_no 复用 orders.order_no（POxxx）。


use aes_gcm::aead::{Aead, Payload};
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use axum::extract::State;
use base64::Engine;
use rsa::pkcs1::{DecodeRsaPrivateKey, DecodeRsaPublicKey};
use rsa::pkcs8::{DecodePrivateKey, DecodePublicKey};
use rsa::pkcs1v15::{Signature, SigningKey, VerifyingKey};
use rsa::sha2::Sha256;
use rsa::signature::{SignatureEncoding, Signer, Verifier};
use rsa::{RsaPrivateKey, RsaPublicKey};
use sea_orm::Value as SqlValue;
use serde_json::{json, Value};

use crate::error::{ok, ApiError, ApiResult};
use crate::state::AppState;

fn sval(s: String) -> SqlValue {
    SqlValue::String(Some(s))
}

/// 读 TEXT 型站点设置（site_settings KV），缺省空串
async fn setting(st: &AppState, key: &str) -> String {
    st.db
        .query_one(
            "SELECT value FROM site_settings WHERE tenant_id = ? AND key = ? LIMIT 1",
            vec![sval(st.tenant.clone()), sval(key.to_string())],
        )
        .await
        .ok()
        .flatten()
        .and_then(|r| r.try_get::<String>("", "value").ok())
        .unwrap_or_default()
}

#[derive(Clone, Debug, Default)]
pub struct PayConfig {
    pub appid: String,
    pub mchid: String,
    pub api_v3_key: String,
    pub serial_no: String,
    pub private_key: String,
    pub platform_pub: String,
}

/// 从 site_settings 装载微信支付配置
pub async fn load_config(st: &AppState) -> Result<PayConfig, ApiError> {
    Ok(PayConfig {
        appid: setting(st, "wechat_pay_appid").await,
        mchid: setting(st, "wechat_pay_mchid").await,
        api_v3_key: setting(st, "wechat_pay_api_v3_key").await,
        serial_no: setting(st, "wechat_pay_serial_no").await,
        private_key: setting(st, "wechat_pay_private_key").await,
        platform_pub: setting(st, "wechat_pay_platform_pub_key").await,
    })
}

/// 校验下单所需配置，返回缺失项列表（空 = 齐全）
pub fn missing_fields(cfg: &PayConfig) -> Vec<&'static str> {
    let mut miss = Vec::new();
    if cfg.appid.trim().is_empty() { miss.push("appid"); }
    if cfg.mchid.trim().is_empty() { miss.push("mchid"); }
    if cfg.api_v3_key.trim().is_empty() { miss.push("APIv3 密钥"); }
    if cfg.serial_no.trim().is_empty() { miss.push("证书序列号"); }
    if cfg.private_key.trim().is_empty() { miss.push("商户私钥"); }
    miss
}

/// 解析商户私钥：优先 PKCS#8（-----BEGIN PRIVATE KEY-----），回落 PKCS#1
fn load_private_key(pem: &str) -> Result<RsaPrivateKey, ApiError> {
    if let Ok(k) = RsaPrivateKey::from_pkcs8_pem(pem.trim()) {
        return Ok(k);
    }
    RsaPrivateKey::from_pkcs1_pem(pem.trim())
        .map_err(|e| ApiError::bad(format!("商户私钥解析失败（需 PEM 格式）：{e}")))
}

/// 解析平台公钥：优先 PKCS#8（SPKI，BEGIN PUBLIC KEY），回落 PKCS#1
fn load_public_key(pem: &str) -> Result<RsaPublicKey, ApiError> {
    // 公钥走 SPKI（BEGIN PUBLIC KEY）；注意 from_pkcs8_* 是私钥（PKCS#8）专属
    if let Ok(k) = RsaPublicKey::from_public_key_pem(pem.trim()) {
        return Ok(k);
    }
    RsaPublicKey::from_pkcs1_pem(pem.trim())
        .map_err(|e| ApiError::bad(format!("平台公钥解析失败（需 PEM 格式）：{e}")))
}

/// SHA256withRSA 签名（微信 v3 请求签名），返回 base64
fn sign_message(private_pem: &str, msg: &str) -> Result<String, ApiError> {
    let key = load_private_key(private_pem)?;
    let sk = SigningKey::<Sha256>::new(key);
    let sig = sk.sign(msg.as_bytes());
    Ok(base64::engine::general_purpose::STANDARD.encode(sig.to_bytes()))
}

/// 平台公钥验签（回调头 Wechatpay-Signature），message = "{ts}\n{nonce}\n{body}\n"
pub fn verify_platform_signature(platform_pem: &str, ts: &str, nonce: &str, body: &str, sig_b64: &str) -> bool {
    let Ok(pk) = load_public_key(platform_pem) else { return false };
    let Ok(raw) = base64::engine::general_purpose::STANDARD.decode(sig_b64) else { return false };
    let Ok(sig) = Signature::try_from(raw.as_slice()) else { return false };
    let vk = VerifyingKey::<Sha256>::new(pk);
    vk.verify(format!("{ts}\n{nonce}\n{body}\n").as_bytes(), &sig).is_ok()
}

/// 对外服务基址（回调 notify_url 用）
fn public_base() -> String {
    std::env::var("PUBLIC_BASE_URL").unwrap_or_else(|_| "http://localhost:8088".into())
}

/// 微信 v3 Native 下单，返回 code_url（weixin://...）
pub async fn create_native_order(
    st: &AppState,
    cfg: &PayConfig,
    order_no: &str,
    amount_cents: i64,
    description: &str,
) -> Result<String, ApiError> {
    let _ = st; // 预留：后续可读环境级覆写
    let path = "/v3/pay/transactions/native";
    let body = json!({
        "appid": cfg.appid.trim(),
        "mchid": cfg.mchid.trim(),
        "description": description,
        "out_trade_no": order_no,
        "notify_url": format!("{}/api/public/pay/wechat/notify", public_base().trim_end_matches('/')),
        "amount": { "total": amount_cents.max(1), "currency": "CNY" },
    })
    .to_string();
    let ts = chrono::Utc::now().timestamp().to_string();
    let nonce = uuid::Uuid::new_v4().simple().to_string();
    let msg = format!("POST\n{path}\n{ts}\n{nonce}\n{body}\n");
    let sig = sign_message(&cfg.private_key, &msg)?;
    let auth = format!(
        "WECHATPAY2-SHA256-RSA2048 mchid=\"{}\",nonce_str=\"{}\",signature=\"{}\",timestamp=\"{}\",serial_no=\"{}\"",
        cfg.mchid.trim(), nonce, sig, ts, cfg.serial_no.trim()
    );

    let base = std::env::var("WECHAT_PAY_API_BASE").unwrap_or_else(|_| "https://api.mch.weixin.qq.com".into());
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}{}", base.trim_end_matches('/'), path))
        .header("Authorization", auth)
        .header("Accept", "application/json")
        .header("User-Agent", "cms-server/0.1")
        .body(body)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| ApiError::bad(format!("微信下单请求失败：{e}")))?;
    let status = resp.status();
    let txt = resp.text().await.unwrap_or_default();
    let j: Value = serde_json::from_str(&txt)
        .map_err(|e| ApiError::bad(format!("微信下单响应解析失败：{e} · {txt}")))?;
    if !status.is_success() {
        let code = j.get("code").and_then(|x| x.as_str()).unwrap_or("");
        let msg = j.pointer("/message").and_then(|x| x.as_str()).unwrap_or("");
        return Err(ApiError::bad(format!("微信下单失败（HTTP {status}）：{code} {msg}")));
    }
    let code_url = j
        .get("code_url")
        .and_then(|x| x.as_str())
        .unwrap_or_default()
        .to_string();
    if code_url.is_empty() {
        return Err(ApiError::bad("微信下单失败：响应缺少 code_url"));
    }
    Ok(code_url)
}

/// 回调解密：resource.ciphertext（base64）→ AES-256-GCM（key=APIv3 密钥, nonce=12B, aad=associated_data）
pub fn decrypt_resource(api_v3_key: &str, ciphertext_b64: &str, nonce: &str, aad: &str) -> Result<Value, ApiError> {
    let key = api_v3_key.trim().as_bytes();
    if key.len() != 32 {
        return Err(ApiError::bad("APIv3 密钥长度须为 32 字节"));
    }
    let ct = base64::engine::general_purpose::STANDARD
        .decode(ciphertext_b64)
        .map_err(|e| ApiError::bad(format!("回文 base64 解码失败：{e}")))?;
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| ApiError::bad(format!("密钥初始化失败：{e}")))?;
    let nonce = Nonce::from_slice(nonce.as_bytes()); // 12 字节
    let pt = cipher
        .decrypt(
            nonce,
            Payload { msg: ct.as_slice(), aad: aad.as_bytes() },
        )
        .map_err(|_| ApiError::unauthorized("回调解密失败（APIv3 密钥不匹配或数据被篡改）"))?;
    serde_json::from_slice(&pt).map_err(|e| ApiError::bad(format!("解密明文 JSON 解析失败：{e}")))
}

/// 文本 → 二维码 SVG（含静区），供前端直接内嵌展示
pub fn qr_svg(text: &str) -> Result<String, ApiError> {
    let code = qrcode::QrCode::new(text.as_bytes())
        .map_err(|e| ApiError::bad(format!("二维码生成失败：{e}")))?;
    Ok(code
        .render::<qrcode::render::svg::Color>()
        .dark_color(qrcode::render::svg::Color("#111827"))
        .light_color(qrcode::render::svg::Color("#ffffff"))
        .quiet_zone(true)
        .build())
}

/// GET /api/admin/payconfig —— 微信支付配置查看（脱敏；需 site.settings.update）
pub async fn pay_config_get(State(st): State<AppState>, auth: crate::auth::Auth) -> ApiResult {
    crate::auth::ensure(&auth, "site.settings.update")?;
    let cfg = load_config(&st).await?;
    let mask = |v: &str| {
        if v.trim().is_empty() {
            String::new()
        } else if v.len() > 4 {
            format!("••••{}", &v[v.len() - 4..])
        } else {
            "••••".into()
        }
    };
    ok(json!({
        "appid": cfg.appid,
        "mchid": cfg.mchid,
        "serialNo": cfg.serial_no,
        "apiV3KeySet": !cfg.api_v3_key.trim().is_empty(),
        "apiV3KeyMasked": mask(&cfg.api_v3_key),
        "privateKeySet": !cfg.private_key.trim().is_empty(),
        "platformPubKeySet": !cfg.platform_pub.trim().is_empty(),
        "ready": missing_fields(&cfg).is_empty(),
    }))
}

/// PUT /api/admin/payconfig —— 微信支付配置保存（只更新出现的非空字段；需 site.settings.update）
pub async fn pay_config_put(
    State(st): State<AppState>,
    auth: crate::auth::Auth,
    axum::Json(body): axum::Json<Value>,
) -> ApiResult {
    crate::auth::ensure(&auth, "site.settings.update")?;
    // 入参 camelCase → KV key
    let map: &[(&str, &str)] = &[
        ("appid", "wechat_pay_appid"),
        ("mchid", "wechat_pay_mchid"),
        ("serialNo", "wechat_pay_serial_no"),
        ("apiV3Key", "wechat_pay_api_v3_key"),
        ("privateKey", "wechat_pay_private_key"),
        ("platformPubKey", "wechat_pay_platform_pub_key"),
    ];
    let now = crate::db::now_iso();
    let mut saved = 0;
    for (field, key) in map {
        if let Some(v) = body.get(*field).and_then(|x| x.as_str()) {
            if v.trim().is_empty() {
                continue; // 空值跳过：不覆盖已有凭据
            }
            st.db
                .execute_statement(sea_orm::Statement::from_sql_and_values(
                    sea_orm::DatabaseBackend::Sqlite,
                    "INSERT INTO site_settings (key, value, tenant_id, created_at, updated_at) \
                     VALUES (?, ?, ?, ?, ?) \
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
                    vec![
                        SqlValue::String(Some(key.to_string())),
                        SqlValue::String(Some(v.to_string())),
                        SqlValue::String(Some(st.tenant.clone())),
                        SqlValue::String(Some(now.clone())),
                        SqlValue::String(Some(now.clone())),
                    ],
                ))
                .await
                .map_err(|e| ApiError::bad(format!("保存配置失败：{e}")))?;
            saved += 1;
        }
    }
    ok(json!({ "ok": true, "saved": saved }))
}

/// POST /api/public/pay/wechat/notify —— 微信支付回调（验签 → 解密 → 幂等发货）
/// 响应体必须是 {"code":"SUCCESS"} / {"code":"FAIL"}，非 2xx 微信会重试。
pub async fn notify(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    body: String,
) -> axum::response::Response {
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    match notify_inner(&st, &headers, &body).await {
        Ok(()) => (
            StatusCode::OK,
            axum::Json(json!({ "code": "SUCCESS", "message": "成功" })),
        )
            .into_response(),
        Err(e) => {
            eprintln!("[wechat_pay] 回调处理失败：{e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(json!({ "code": "FAIL", "message": e })),
            )
                .into_response()
        }
    }
}

async fn notify_inner(st: &AppState, headers: &axum::http::HeaderMap, body: &str) -> Result<(), String> {
    let cfg = load_config(st).await.map_err(|e| e.message)?;
    if cfg.api_v3_key.trim().is_empty() {
        return Err("微信支付未配置（缺少 APIv3 密钥）".into());
    }
    let hv = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string()
    };
    let ts = hv("wechatpay-timestamp");
    let nonce = hv("wechatpay-nonce");
    let sig = hv("wechatpay-signature");
    // 验签：配置了平台公钥则强制；未配置时依赖回文解密（无密钥无法伪造有效密文）
    if !cfg.platform_pub.trim().is_empty() {
        if !verify_platform_signature(&cfg.platform_pub, &ts, &nonce, body, &sig) {
            return Err("回调签名校验失败".into());
        }
    }
    let evt: Value = serde_json::from_str(body).map_err(|e| format!("回调 JSON 解析失败：{e}"))?;
    let ciphertext = evt.pointer("/resource/ciphertext").and_then(|x| x.as_str()).unwrap_or("");
    let res_nonce = evt.pointer("/resource/nonce").and_then(|x| x.as_str()).unwrap_or("");
    let aad = evt.pointer("/resource/associated_data").and_then(|x| x.as_str()).unwrap_or("");
    if ciphertext.is_empty() {
        return Err("回调缺少 resource.ciphertext".into());
    }
    let plain = decrypt_resource(&cfg.api_v3_key, ciphertext, res_nonce, aad)
        .map_err(|e| e.message)?;
    let out_trade_no = plain.get("out_trade_no").and_then(|x| x.as_str()).unwrap_or("");
    let trade_state = plain.get("trade_state").and_then(|x| x.as_str()).unwrap_or("");
    let transaction_id = plain.get("transaction_id").and_then(|x| x.as_str()).unwrap_or("");
    if out_trade_no.is_empty() {
        return Err("回调缺少 out_trade_no".into());
    }
    if trade_state != "SUCCESS" {
        // 非成功态（REFUND/CLOSED/...）：确认收到即可，不改变订单状态
        return Ok(());
    }

    // F5 修复：校验回调金额与本地订单金额一致。
    // 此前只按 out_trade_no 匹配就置为已支付，若订单号被猜到/泄露，
    // 攻击者可用一笔极小额支付（如 1 分）把自己的大额订单「买」成已支付。
    let paid_total = plain
        .pointer("/amount/total")
        .and_then(|x| x.as_i64())
        .unwrap_or(0);
    if paid_total <= 0 {
        return Err("回调缺少 amount.total".into());
    }
    let expect = st
        .db
        .query_one(
            "SELECT id, amount_cents FROM orders WHERE tenant_id = ? AND order_no = ? LIMIT 1",
            vec![
                sea_orm::Value::String(Some(st.tenant.clone())),
                sea_orm::Value::String(Some(out_trade_no.to_string())),
            ],
        )
        .await
        .map_err(|e| format!("查询订单失败：{e}"))?;
    let Some(er) = expect else {
        return Err(format!("订单不存在：{out_trade_no}"));
    };
    let expect_cents: i64 = er.try_get("", "amount_cents").unwrap_or(0);
    if expect_cents <= 0 {
        return Err(format!("订单金额异常：{out_trade_no}"));
    }
    if paid_total != expect_cents {
        eprintln!(
            "[wechat_pay] 金额不符，拒绝发货：order={out_trade_no} 回调={paid_total} 本地={expect_cents}"
        );
        return Err(format!(
            "回调金额不符（回调 {paid_total}，订单 {expect_cents}）"
        ));
    }

    // 原子确认：只有 pending → paid 成功一次；已 paid 幂等回 SUCCESS
    let n = st
        .db
        .execute(
            "UPDATE orders SET status = 'paid', paid_at = ?, updated_at = ?, ref_no = ? \
             WHERE tenant_id = ? AND order_no = ? AND status = 'pending'",
            vec![
                sval(crate::db::now_iso()),
                sval(crate::db::now_iso()),
                sval(transaction_id.to_string()),
                sval(st.tenant.clone()),
                sval(out_trade_no.to_string()),
            ],
        )
        .await
        .map_err(|e| format!("订单更新失败：{e}"))?;
    if n == 0 {
        let row = st
            .db
            .query_one(
                "SELECT status FROM orders WHERE tenant_id = ? AND order_no = ? LIMIT 1",
                vec![sval(st.tenant.clone()), sval(out_trade_no.to_string())],
            )
            .await
            .map_err(|e| format!("订单查询失败：{e}"))?;
        let status = row
            .and_then(|r| r.try_get::<String>("", "status").ok())
            .unwrap_or_default();
        if status == "paid" {
            return Ok(()); // 重复回调，幂等确认
        }
        return Err(format!("订单 {out_trade_no} 不存在或状态不允许确认（当前：{status}）"));
    }
    // 发货（与人工确认同一条链路：充积分 / 开订阅 / 邀请奖励）
    let id: String = st
        .db
        .query_one(
            "SELECT id FROM orders WHERE tenant_id = ? AND order_no = ? LIMIT 1",
            vec![sval(st.tenant.clone()), sval(out_trade_no.to_string())],
        )
        .await
        .map_err(|e| format!("订单查询失败：{e}"))?
        .and_then(|r| r.try_get::<String>("", "id").ok())
        .ok_or_else(|| format!("订单 {out_trade_no} 不存在"))?;
    crate::points::fulfill_order(st, &id)
        .await
        .map_err(|e| format!("发货失败：{}", e.message))?;
    crate::webhooks_out::emit(
        st,
        "order.paid",
        json!({ "orderNo": out_trade_no, "channel": "wechat", "transactionId": transaction_id }),
    );
    Ok(())
}
