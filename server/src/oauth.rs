//! OAuth2 授权码流（B6）：通用框架 + 真实 provider。
//!
//! 流程：前端填 client_id/secret（存库）→ GET /api/integrations/{id}/oauth/start
//! 生成 state 并返回授权 URL → 浏览器跳三方授权页 → 三方回调本服务
//! /api/integrations/oauth/callback（匿名）→ 校验 state → code 换 token →
//! 存库（Secret 掩码）→ 302 回前端标记页。
//!
//! provider 注册表：
//! - google：GA4 / Search Console（scope: analytics.readonly）
//! - github：开发者平台（scope: user:email）
//! - mock：本地模拟授权服务（开发/测试用，冒烟验证整条链路）

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::{error::{ApiError, ApiResult, ok}, state::AppState};

const STATE_TTL: Duration = Duration::from_secs(10 * 60);

struct StateEntry {
    provider: String,
    integration_id: String,
    expires: Instant,
}

fn states() -> &'static Mutex<HashMap<String, StateEntry>> {
    static S: OnceLock<Mutex<HashMap<String, StateEntry>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashMap::new()))
}

#[derive(Clone)]
pub struct Provider {
    pub name: &'static str,
    pub authorize_url: &'static str,
    pub token_url: &'static str,
    pub scope: &'static str,
}

pub fn providers() -> Vec<Provider> {
    vec![
        Provider {
            name: "google",
            authorize_url: "https://accounts.google.com/o/oauth2/v2/auth",
            token_url: "https://oauth2.googleapis.com/token",
            scope: "https://www.googleapis.com/auth/analytics.readonly",
        },
        Provider {
            name: "github",
            authorize_url: "https://github.com/login/oauth/authorize",
            token_url: "https://github.com/login/oauth/access_token",
            scope: "user:email",
        },
        Provider {
            name: "mock",
            authorize_url: "http://127.0.0.1:9191/oauth/mock/authorize",
            token_url: "http://127.0.0.1:9191/oauth/mock/token",
            scope: "mock",
        },
    ]
}

fn find_provider(name: &str) -> Option<Provider> {
    providers().into_iter().find(|p| p.name == name)
}

fn sval(s: String) -> sea_orm::Value {
    sea_orm::Value::String(Some(s))
}

fn base_url() -> String {
    std::env::var("PUBLIC_BASE_URL").unwrap_or_else(|_| "http://localhost:5188".into())
}

fn redirect_uri(provider: &str) -> String {
    format!("{}/api/integrations/oauth/callback", base_url())
}

/// GET /api/integrations/{id}/oauth/start —— 返回三方授权 URL
/// （前置：集成已存 client_id/secret）
pub async fn start(State(st): State<AppState>, auth: crate::auth::Auth, Path(id): Path<String>) -> ApiResult {
    crate::auth::ensure(&auth, "automation.integrations.toggle")?;
    let row = st
        .db
        .query_one(
            "SELECT key, oauth_provider, oauth_client_id, oauth_client_secret FROM integrations \
             WHERE id = ? AND tenant_id = ? LIMIT 1",
            vec![sval(id.clone()), sval(st.tenant.clone())],
        )
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    let Some(r) = row else { return Err(ApiError::not_found("集成不存在")) };
    let provider_name: String = r.try_get("", "oauth_provider").unwrap_or_default();
    let client_id: String = r.try_get("", "oauth_client_id").unwrap_or_default();
    let client_secret: String = r.try_get("", "oauth_client_secret").unwrap_or_default();
    if provider_name.is_empty() {
        return Err(ApiError::bad("该集成不支持 OAuth 连接（请用 API Key 方式）"));
    }
    if client_id.is_empty() || client_secret.is_empty() {
        return Err(ApiError::bad("请先填写 client_id 与 client_secret"));
    }
    let Some(provider) = find_provider(&provider_name) else {
        return Err(ApiError::bad(format!("未知 provider：{provider_name}")));
    };
    let state = uuid::Uuid::new_v4().to_string();
    states().lock().unwrap().insert(
        state.clone(),
        StateEntry { provider: provider.name.to_string(), integration_id: id, expires: Instant::now() + STATE_TTL },
    );
    let url = format!(
        "{}?client_id={}&redirect_uri={}&response_type=code&scope={}&state={}",
        provider.authorize_url,
        urlencode(&client_id),
        urlencode(&redirect_uri(provider.name)),
        urlencode(provider.scope),
        state,
    );
    ok(json!({ "authorizeUrl": url, "state": state }))
}

/// GET /api/integrations/oauth/callback —— 三方回调（匿名）
pub async fn callback(
    State(st): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let state = params.get("state").cloned().unwrap_or_default();
    let code = params.get("code").cloned().unwrap_or_default();
    let entry = states().lock().unwrap().remove(&state);
    let Some(entry) = entry else {
        return redirect_home("oauth=bad_state");
    };
    if entry.expires < Instant::now() {
        return redirect_home("oauth=expired");
    }
    let Some(provider) = find_provider(&entry.provider) else {
        return redirect_home("oauth=bad_provider");
    };
    // 读回 client 凭据
    let row = st
        .db
        .query_one(
            "SELECT oauth_client_id, oauth_client_secret FROM integrations WHERE id = ? AND tenant_id = ?",
            vec![sval(entry.integration_id.clone()), sval(st.tenant.clone())],
        )
        .await
        .ok()
        .flatten();
    let (Some(r), ) = (row,) else { return redirect_home("oauth=missing") };
    let client_id: String = r.try_get("", "oauth_client_id").unwrap_or_default();
    let client_secret: String = r.try_get("", "oauth_client_secret").unwrap_or_default();

    // code → token
    let token_body = json!({
        "client_id": client_id,
        "client_secret": client_secret,
        "code": code,
        "grant_type": "authorization_code",
        "redirect_uri": redirect_uri(&entry.provider),
    });
    let mut req = reqwest::Client::new()
        .post(provider.token_url)
        .form(&token_body);
    if provider.name == "github" {
        req = req.header("Accept", "application/json");
    }
    let token_resp = match req.send().await {
        Ok(r) => r,
        Err(_) => return redirect_home("oauth=token_fail"),
    };
    let token_json: Value = match token_resp.json().await {
        Ok(v) => v,
        Err(_) => return redirect_home("oauth=token_fail"),
    };
    let access_token = token_json.get("access_token").and_then(|t| t.as_str()).unwrap_or("");
    if access_token.is_empty() {
        let err = token_json.get("error_description").or_else(|| token_json.get("error")).map(|e| e.to_string()).unwrap_or_default();
        return redirect_home(&format!("oauth=token_error&msg={}", urlencode(&err)));
    }

    // 存库并标记已连接
    let _ = st
        .db
        .execute(
            "UPDATE integrations SET oauth_token = ?, connected = 1, updated_at = ? WHERE id = ?",
            vec![
                sval(access_token.to_string()),
                sval(crate::db::now_iso()),
                sval(entry.integration_id.clone()),
            ],
        )
        .await;
    redirect_home("oauth=connected")
}

/// GET /api/integrations/{id}/oauth/status —— 是否已完成授权
pub async fn status(State(st): State<AppState>, auth: crate::auth::Auth, Path(id): Path<String>) -> ApiResult {
    crate::auth::ensure(&auth, "automation.integrations.view")?;
    let row = st
        .db
        .query_one(
            "SELECT connected, oauth_token FROM integrations WHERE id = ? AND tenant_id = ? LIMIT 1",
            vec![sval(id.clone()), sval(st.tenant.clone())],
        )
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    let Some(r) = row else { return Err(ApiError::not_found("集成不存在")) };
    let connected: i64 = r.try_get("", "connected").unwrap_or(0);
    let token: Option<String> = r.try_get("", "oauth_token").unwrap_or(None);
    ok(json!({
        "connected": connected == 1 && token.as_deref().unwrap_or("").len() > 0,
        "hasToken": token.is_some() && !token.unwrap_or_default().is_empty(),
    }))
}

fn redirect_home(query: &str) -> Response {
    Redirect::temporary(&format!("{}/settings?{}", base_url(), query))
        .into_response()
}

fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

// 使 StatusCode 导入被使用（Redirect 等）
#[allow(dead_code)]
fn _unused(_: StatusCode) {}
