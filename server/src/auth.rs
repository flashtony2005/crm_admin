//! 认证：JWT 签发/校验 + 登录 + 当前用户。
//! Token 载荷只放身份事实（sub/role/tenant），权限由角色矩阵实时推导 ——
//! 角色授权变更后无需重签 token 即生效。

use argon2::{password_hash::{PasswordHash, PasswordHasher, SaltString}, Argon2, PasswordVerifier};
use rand_core::OsRng;
use axum::{extract::{FromRequestParts, State}, Json};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::{
    error::{ok, ApiError, ApiResult},
    perm,
    state::AppState,
};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Claims {
    pub sub: String,      // user id
    pub username: String,
    pub role: String,     // owner | editor | viewer
    pub tenant: String,   // tenant_id
    pub exp: u64,
}

const JWT_DAYS: u64 = 7;
const DEFAULT_SECRET: &str = "dev-secret-change-me";

fn secret() -> String {
    std::env::var("JWT_SECRET").unwrap_or_else(|_| DEFAULT_SECRET.into())
}

/// A2 安全治理：生产环境必须显式配置 JWT_SECRET（缺省值只允许在开发模式）。
/// 在 main 启动时调用；违规直接 panic，拒绝带弱密钥上线。
pub fn assert_jwt_secret() {
    let env_prod = std::env::var("CMS_ENV").as_deref() == Ok("production");
    if env_prod && secret() == DEFAULT_SECRET {
        panic!("拒绝启动：CMS_ENV=production 必须设置 JWT_SECRET 环境变量（不得使用默认值）");
    }
}

// ── A4 登录限速：同用户名连续失败 5 次 → 锁定 15 分钟（内存态，重启即清）──
const MAX_FAILS: u32 = 5;
const LOCK_DURATION: Duration = Duration::from_secs(15 * 60);

fn fail_board() -> &'static Mutex<HashMap<String, (u32, Option<Instant>)>> {
    static BOARD: OnceLock<Mutex<HashMap<String, (u32, Option<Instant>)>>> = OnceLock::new();
    BOARD.get_or_init(|| Mutex::new(HashMap::new()))
}

fn login_locked(username: &str) -> bool {
    let board = fail_board().lock().unwrap();
    match board.get(username) {
        Some((_, Some(until))) if *until > Instant::now() => true,
        _ => false,
    }
}

fn record_fail(username: &str) {
    let mut board = fail_board().lock().unwrap();
    let e = board.entry(username.to_string()).or_insert((0, None));
    e.0 += 1;
    if e.0 >= MAX_FAILS {
        *e = (0, Some(Instant::now() + LOCK_DURATION)); // 计数清零并锁定
    }
}

fn clear_fails(username: &str) {
    fail_board().lock().unwrap().remove(username);
}

pub fn sign(claims: &Claims) -> Result<String, ApiError> {
    encode(
        &Header::default(),
        claims,
        &EncodingKey::from_secret(secret().as_bytes()),
    )
    .map_err(|e| ApiError::bad(format!("签发失败：{e}")))
}

pub fn verify(token: &str) -> Option<Claims> {
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret().as_bytes()),
        &Validation::default(),
    )
    .ok()
    .map(|d| d.claims)
}

/// 提取器：请求携带有效 Bearer token 才放行（不含权限判断）。
pub struct Auth(pub Claims);

impl<S> FromRequestParts<S> for Auth
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
        Ok(Auth(claims))
    }
}

/// 权限断言（对应 RuoYi @ss.hasPermi 的编程式用法）
pub fn ensure(auth: &Auth, perm: &str) -> Result<(), ApiError> {
    let role = perm::Role::from_str(&auth.0.role)
        .ok_or_else(|| ApiError::unauthorized("未知角色"))?;
    if perm::perm_matches(&role.perms(), perm) {
        Ok(())
    } else {
        Err(ApiError::forbidden(perm))
    }
}

#[derive(Deserialize)]
pub struct LoginReq {
    pub username: String,
    pub password: String,
}

/// POST /api/auth/login
pub async fn login(
    State(st): State<AppState>,
    Json(req): Json<LoginReq>,
) -> ApiResult {
    // A4：锁定中的账号即使密码正确也拒绝
    if login_locked(&req.username) {
        return Err(ApiError::rate_limited("尝试次数过多，请 15 分钟后再试"));
    }
    let row = sqlx_user_by_username(&st, &req.username).await?;
    let Some(user) = row else {
        record_fail(&req.username);
        return Err(ApiError::unauthorized("用户名或密码错误"));
    };
    if user.status != 1 {
        return Err(ApiError::unauthorized("账号已被停用"));
    }
    let hash = PasswordHash::new(&user.password_hash)
        .map_err(|_| ApiError::bad("密码哈希损坏"))?;
    let valid = Argon2::default()
        .verify_password(req.password.as_bytes(), &hash)
        .is_ok();
    if !valid {
        record_fail(&req.username);
        return Err(ApiError::unauthorized("用户名或密码错误"));
    }
    clear_fails(&req.username);
    let now = now_secs();
    let claims = Claims {
        sub: user.id.clone(),
        username: user.username.clone(),
        role: user.role.clone(),
        tenant: user.tenant_id.clone(),
        exp: now + JWT_DAYS * 86400,
    };
    let token = sign(&claims)?;
    ok(json!({
        "token": token,
        "user": {
            "id": user.id_num(),
            "username": user.username,
            "nickname": user.nickname,
            "email": user.email,
            "role": user.role,
            "tenant_id": 1,
            "status": user.status,
            "mustChangePassword": user.must_change_password != 0,
            "created_at": user.created_at,
            "updated_at": user.updated_at,
        },
    }))
}

#[derive(Deserialize)]
pub struct ChangePasswordReq {
    pub old_password: String,
    pub new_password: String,
}

/// POST /api/me/password —— 修改自己的密码（任意登录角色）
pub async fn change_password(
    State(st): State<AppState>,
    auth: Auth,
    Json(req): Json<ChangePasswordReq>,
) -> ApiResult {
    if req.new_password.len() < 8 {
        return Err(ApiError::bad("新密码至少 8 位"));
    }
    if req.new_password == req.old_password {
        return Err(ApiError::bad("新密码不能与旧密码相同"));
    }
    let row = sqlx_user_by_username(&st, &auth.0.username).await?;
    let Some(user) = row else { return Err(ApiError::not_found("用户不存在")) };
    let hash = PasswordHash::new(&user.password_hash)
        .map_err(|_| ApiError::bad("密码哈希损坏"))?;
    let valid = Argon2::default()
        .verify_password(req.old_password.as_bytes(), &hash)
        .is_ok();
    if !valid {
        return Err(ApiError::bad("旧密码不正确"));
    }
    st.db
        .execute_statement(sea_orm::Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Sqlite,
            "UPDATE users SET password_hash = ?, must_change_password = 0, updated_at = ? WHERE id = ?",
            vec![
                sea_orm::Value::String(Some(hash_password(&req.new_password))),
                sea_orm::Value::String(Some(crate::db::now_iso())),
                sea_orm::Value::String(Some(user.id)),
            ],
        ))
        .await
        .map_err(|e| ApiError::bad(format!("更新失败：{e}")))?;
    ok(json!({ "changed": true }))
}

/// GET /api/user/me —— 返回用户 + 实时权限集（前端 setGranted 的数据源）
pub async fn me(State(st): State<AppState>, auth: Auth) -> ApiResult {
    let role = perm::Role::from_str(&auth.0.role).ok_or_else(|| ApiError::unauthorized("未知角色"))?;
    let perms = role.perms();
    let row = sqlx_user_by_username(&st, &auth.0.username).await?;
    let (nickname, email, mcp) = row
        .map(|u| (u.nickname, u.email, u.must_change_password != 0))
        .unwrap_or_else(|| (auth.0.username.clone(), String::new(), false));
    ok(json!({
        "id": 0,
        "username": auth.0.username,
        "nickname": nickname,
        "email": email,
        "role": auth.0.role,
        "tenant_id": 1,
        "status": 1,
        "mustChangePassword": mcp,
        "created_at": "",
        "updated_at": "",
        "permissions": perms,
    }))
}

#[derive(Deserialize)]
pub struct UpdateProfileReq {
    pub nickname: Option<String>,
    pub email: Option<String>,
}

/// POST /api/me/profile —— 更新当前用户的昵称 / 邮箱（任意登录角色）
pub async fn update_profile(
    State(st): State<AppState>,
    auth: Auth,
    Json(req): Json<UpdateProfileReq>,
) -> ApiResult {
    let row = sqlx_user_by_username(&st, &auth.0.username).await?;
    let Some(user) = row else { return Err(ApiError::not_found("用户不存在")) };

    let mut nickname = user.nickname.clone();
    if let Some(ref n) = req.nickname {
        let n = n.trim().to_string();
        if n.is_empty() {
            return Err(ApiError::bad("昵称不能为空"));
        }
        nickname = n;
    }
    let mut email = user.email.clone();
    if let Some(ref e) = req.email {
        let e = e.trim().to_string();
        if !e.is_empty() && !e.contains('@') {
            return Err(ApiError::bad("邮箱格式不正确"));
        }
        email = e;
    }

    st.db
        .execute_statement(sea_orm::Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Sqlite,
            "UPDATE users SET nickname = ?, email = ?, updated_at = ? WHERE id = ?",
            vec![
                sea_orm::Value::String(Some(nickname.clone())),
                sea_orm::Value::String(Some(email.clone())),
                sea_orm::Value::String(Some(crate::db::now_iso())),
                sea_orm::Value::String(Some(user.id)),
            ],
        ))
        .await
        .map_err(|e| ApiError::bad(format!("更新失败：{e}")))?;

    ok(json!({
        "username": auth.0.username,
        "nickname": nickname,
        "email": email,
    }))
}

// ── 用户表访问（Phase 3 换 SeaORM entity）──

pub struct UserRow {
    pub id: String,
    pub username: String,
    pub nickname: String,
    pub email: String,
    pub password_hash: String,
    pub role: String,
    pub tenant_id: String,
    pub status: i64,
    pub must_change_password: i64,
    pub created_at: String,
    pub updated_at: String,
}

impl UserRow {
    fn id_num(&self) -> i64 {
        self.id.parse().unwrap_or(0)
    }
}

async fn sqlx_user_by_username(st: &AppState, username: &str) -> Result<Option<UserRow>, ApiError> {
    use sea_orm::Statement;
    let res = st
        .db
        .query_one_statement(Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT id, username, nickname, email, password_hash, role, tenant_id, status, must_change_password, created_at, updated_at \
                 FROM users WHERE username = '{}' LIMIT 1",
                username.replace('\'', "''")
            ),
        ))
        .await
        .map_err(|e| ApiError::bad(format!("查询用户失败：{e}")))?;
    let Some(r) = res else { return Ok(None) };
    Ok(Some(UserRow {
        id: r.try_get("", "id").map_err(internal)?,
        username: r.try_get("", "username").map_err(internal)?,
        nickname: r.try_get("", "nickname").map_err(internal)?,
        email: r.try_get("", "email").unwrap_or_default(),
        password_hash: r.try_get("", "password_hash").map_err(internal)?,
        role: r.try_get("", "role").map_err(internal)?,
        tenant_id: r.try_get("", "tenant_id").map_err(internal)?,
        status: r.try_get::<i64>("", "status").map_err(internal)?,
        must_change_password: r.try_get::<i64>("", "must_change_password").unwrap_or(0),
        created_at: r.try_get("", "created_at").map_err(internal)?,
        updated_at: r.try_get("", "updated_at").map_err(internal)?,
    }))
}

fn internal(e: impl std::fmt::Display) -> ApiError {
    ApiError::bad(format!("行解析失败：{e}"))
}

pub(crate) fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// 哈希工具（bootstrap 种子用户用）
pub fn hash_password(pw: &str) -> String {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(pw.as_bytes(), &salt)
        .expect("argon2 hash")
        .to_string()
}
