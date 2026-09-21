//! 会员（Members）：公开注册 / 登录 / 个人资料。
//! 复用 auth 的 JWT 与 argon2，但签发 role="member" 的令牌，与管理员令牌隔离
//! （管理员 Auth 提取器只接受 owner/editor/viewer；会员令牌无法访问管理端点）。

use argon2::{password_hash::{PasswordHash, PasswordVerifier}, Argon2};
use axum::{extract::{Path, State}, Json};
use sea_orm::{Statement, Value as SqlValue};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    auth::{hash_password, sign, verify, Auth},
    db::now_iso,
    error::{ok, ApiError, ApiResult},
    state::AppState,
};

fn sval(s: String) -> SqlValue { SqlValue::String(Some(s)) }

/// 会员令牌提取器：仅接受 role=="member" 的合法 JWT。
pub struct MemberAuth(pub crate::auth::Claims);

impl<S> axum::extract::FromRequestParts<S> for MemberAuth
where
    S: Send + Sync,
{
    type Rejection = ApiError;
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or_else(|| ApiError::unauthorized("未登录"))?;
        let claims = verify(token).ok_or_else(|| ApiError::unauthorized("登录已过期"))?;
        if claims.role != "member" {
            return Err(ApiError::unauthorized("需要会员身份"));
        }
        Ok(MemberAuth(claims))
    }
}

fn member_claims(id: &str, email: &str, tenant: &str) -> crate::auth::Claims {
    crate::auth::Claims {
        sub: id.to_string(),
        username: email.to_string(),
        role: "member".to_string(),
        tenant: tenant.to_string(),
        // 会员 token 不参与 users.token_version 校验（那是后台账号体系），置 0
        tv: 0,
        exp: crate::auth::now_secs() + 30 * 86400,
    }
}

#[derive(Deserialize)]
pub struct MemberRegister {
    pub email: String,
    pub name: Option<String>,
    pub password: String,
    /// 邀请码：member_invite_required=on 时必填；off 时可空
    #[serde(default)]
    pub invite_code: Option<String>,
}

/// 读注册门槛开关：site_settings.member_invite_required ∈ on/1/true 视为开启
async fn invite_required(st: &AppState) -> bool {
    let v = st
        .db
        .query_one(
            "SELECT value FROM site_settings WHERE tenant_id = ? AND key = 'member_invite_required' LIMIT 1",
            vec![sval(st.tenant.clone())],
        )
        .await
        .ok()
        .flatten()
        .and_then(|r| r.try_get::<String>("", "value").ok())
        .unwrap_or_else(|| "off".into());
    matches!(v.as_str(), "on" | "1" | "true")
}

/// 校验并原子核销一个邀请码：返回邀请人 member id。
/// 核销走 `UPDATE ... WHERE used < quota` 条件写 + rows_affected 判定，
/// 并发重复注册不会超额消费额度。
async fn consume_invite(st: &AppState, code: &str) -> Result<String, ApiError> {
    let now = now_iso();
    let row = st
        .db
        .query_one(
            "SELECT id, owner_member_id, quota, used, expires_at, enabled \
             FROM invite_codes WHERE tenant_id = ? AND code = ? LIMIT 1",
            vec![sval(st.tenant.clone()), sval(code.to_string())],
        )
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    let Some(r) = row else { return Err(ApiError::bad("邀请码无效")) };
    let id: String = r.try_get("", "id").map_err(internal)?;
    let owner: String = r.try_get("", "owner_member_id").unwrap_or_default();
    let quota: i64 = r.try_get::<i64>("", "quota").unwrap_or(1);
    let used: i64 = r.try_get::<i64>("", "used").unwrap_or(0);
    let expires_at: String = r.try_get("", "expires_at").unwrap_or_default();
    let enabled: i64 = r.try_get::<i64>("", "enabled").unwrap_or(1);
    if enabled != 1 {
        return Err(ApiError::bad("邀请码已停用"));
    }
    if used >= quota {
        return Err(ApiError::bad("邀请码次数已用完"));
    }
    if !expires_at.is_empty() && expires_at.as_str() <= now.as_str() {
        return Err(ApiError::bad("邀请码已过期"));
    }
    let n = st
        .db
        .execute(
            "UPDATE invite_codes SET used = used + 1, updated_at = ? \
             WHERE id = ? AND used < quota",
            vec![sval(now), sval(id)],
        )
        .await
        .map_err(|e| ApiError::bad(format!("核销失败：{e}")))?;
    if n == 0 {
        return Err(ApiError::bad("邀请码次数已用完"));
    }
    Ok(owner)
}

/// POST /api/public/members/register
pub async fn register(State(st): State<AppState>, Json(req): Json<MemberRegister>) -> ApiResult {
    let email = req.email.trim().to_lowercase();
    if !email.contains('@') {
        return Err(ApiError::bad("邮箱格式不正确"));
    }
    if req.password.len() < 8 {
        return Err(ApiError::bad("密码至少 8 位"));
    }
    // 邀请门槛：开启时必须凭有效邀请码注册；关闭时提供了码也要校验（显式意图）
    let required = invite_required(&st).await;
    let code = req
        .invite_code
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let invited_by = match code.as_deref() {
        Some(c) => consume_invite(&st, c).await?,
        None if required => return Err(ApiError::bad("本站已开启邀请制，注册需要邀请码")),
        None => String::new(),
    };
    // 重名检查
    let exist = st
        .db
        .query_one(
            "SELECT id FROM members WHERE tenant_id = ? AND email = ? LIMIT 1",
            vec![sval(st.tenant.clone()), sval(email.clone())],
        )
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    if exist.is_some() {
        return Err(ApiError::bad("该邮箱已注册"));
    }
    let id = Uuid::new_v4().to_string();
    let now = now_iso();
    let name = req.name.clone().unwrap_or_else(|| email.clone());
    st.db
        .execute(
            "INSERT INTO members (id, tenant_id, email, name, password_hash, status, plan, invited_by, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, 1, 'free', ?, ?, ?)",
            vec![
                sval(id.clone()),
                sval(st.tenant.clone()),
                sval(email.clone()),
                sval(name.clone()),
                sval(hash_password(&req.password)),
                sval(invited_by.clone()),
                sval(now.clone()),
                sval(now.clone()),
            ],
        )
        .await
        .map_err(|e| ApiError::bad(format!("注册失败：{e}")))?;
    let token = sign(&member_claims(&id, &email, &st.tenant))?;
    // 触发会员注册出站 Webhook
    crate::webhooks_out::emit(&st, "member.registered", json!({ "id": id, "email": email, "name": name, "invited": !invited_by.is_empty() }));
    ok(json!({ "token": token, "member": public_member(&id, &email, &name, "free") }))
}

#[derive(Deserialize)]
pub struct MemberLogin {
    pub email: String,
    pub password: String,
}

/// POST /api/public/members/login
pub async fn login(State(st): State<AppState>, Json(req): Json<MemberLogin>) -> ApiResult {
    let email = req.email.trim().to_lowercase();
    let row = st
        .db
        .query_one(
            "SELECT id, name, password_hash, plan, status FROM members WHERE tenant_id = ? AND email = ? LIMIT 1",
            vec![sval(st.tenant.clone()), sval(email.clone())],
        )
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    let Some(r) = row else { return Err(ApiError::unauthorized("邮箱或密码错误")) };
    let id: String = r.try_get("", "id").map_err(internal)?;
    let name: String = r.try_get("", "name").unwrap_or_default();
    let hash: String = r.try_get("", "password_hash").map_err(internal)?;
    let plan: String = r.try_get("", "plan").unwrap_or_else(|_| "free".into());
    let status: i64 = r.try_get::<i64>("", "status").unwrap_or(1);
    if status != 1 {
        return Err(ApiError::unauthorized("账号已被停用"));
    }
    let ph = PasswordHash::new(&hash).map_err(|_| ApiError::bad("密码哈希损坏"))?;
    if !Argon2::default().verify_password(req.password.as_bytes(), &ph).is_ok() {
        return Err(ApiError::unauthorized("邮箱或密码错误"));
    }
    let token = sign(&member_claims(&id, &email, &st.tenant))?;
    ok(json!({ "token": token, "member": public_member(&id, &email, &name, &plan) }))
}

/// GET /api/public/members/me
pub async fn me(State(st): State<AppState>, auth: MemberAuth) -> ApiResult {
    let row = st
        .db
        .query_one(
            "SELECT id, email, name, plan FROM members WHERE id = ? AND tenant_id = ? LIMIT 1",
            vec![sval(auth.0.sub.clone()), sval(st.tenant.clone())],
        )
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    let Some(r) = row else { return Err(ApiError::not_found("会员不存在")) };
    let id: String = r.try_get("", "id").map_err(internal)?;
    let email: String = r.try_get("", "email").unwrap_or_default();
    let name: String = r.try_get("", "name").unwrap_or_default();
    let plan: String = r.try_get("", "plan").unwrap_or_else(|_| "free".into());
    ok(json!({ "member": public_member(&id, &email, &name, &plan) }))
}

/// GET /api/public/members/plans —— 当前会员套餐（供前端展示）
pub async fn plans(_st: State<AppState>) -> ApiResult {
    ok(json!({ "plan": "free" }))
}

fn public_member(id: &str, email: &str, name: &str, plan: &str) -> Value {
    json!({ "id": id, "email": email, "name": name, "plan": plan })
}

#[derive(Deserialize)]
pub struct MemberUpdate {
    pub name: Option<String>,
    pub plan: Option<String>,
}

/// POST /api/public/members/me —— 更新昵称（会员自助）
pub async fn update_me(State(st): State<AppState>, auth: MemberAuth, Json(req): Json<MemberUpdate>) -> ApiResult {
    let mut sets = Vec::new();
    let mut args: Vec<SqlValue> = vec![];
    if let Some(n) = &req.name {
        sets.push("name = ?");
        args.push(sval(n.trim().to_string()));
    }
    if let Some(p) = &req.plan {
        sets.push("plan = ?");
        args.push(sval(p.clone()));
    }
    if sets.is_empty() {
        return Err(ApiError::bad("无更新字段"));
    }
    sets.push("updated_at = ?");
    args.push(sval(now_iso()));
    args.push(sval(auth.0.sub.clone()));
    args.push(sval(st.tenant.clone()));
    st.db
        .execute(&format!("UPDATE members SET {} WHERE id = ? AND tenant_id = ?", sets.join(", ")), args)
        .await
        .map_err(|e| ApiError::bad(format!("更新失败：{e}")))?;
    ok(json!({ "updated": true }))
}

fn internal(e: impl std::fmt::Display) -> ApiError {
    ApiError::bad(format!("行解析失败：{e}"))
}

// 防止未使用告警
#[allow(dead_code)]
fn _use(_: Auth) {}
