//! 认证：JWT 签发/校验 + 登录 + 当前用户。
//! Token 载荷只放身份事实（sub/role/tenant），权限由角色矩阵实时推导 ——
//! 角色授权变更后无需重签 token 即生效。

use argon2::{password_hash::{PasswordHash, PasswordHasher, SaltString}, Argon2, PasswordVerifier};
use rand_core::OsRng;
use axum::{
    extract::{FromRef, FromRequestParts, Path, State},
    Json,
};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::Deserialize;
use serde_json::json;
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
    /// 令牌版本号：与 users.token_version 比对，用于「改密/踢下线」后作废旧 token。
    /// 带 serde(default) 保证存量 token（无此字段）仍能解析，不会把已登录用户全踢掉。
    #[serde(default)]
    pub tv: i64,
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

// ── G2 登录限速落库：同用户名连续失败 5 次 → 锁定 15 分钟。
// 原为进程内 Mutex（重启即清、多实例不共享），现持久化到 login_locks 表；
// UPSERT 原子完成「计数+1 或触发锁定」，无读改写竞态。──
const MAX_FAILS: i64 = 5;
const LOCK_MINS: i64 = 15;

fn sval_str(s: String) -> sea_orm::Value {
    sea_orm::Value::String(Some(s))
}

async fn login_locked(st: &AppState, username: &str) -> bool {
    let now = crate::db::now_iso();
    // 顺带清理已过期的锁（登录低频，无性能顾虑）
    let _ = st
        .db
        .execute(
            "DELETE FROM login_locks WHERE locked_until <> '' AND locked_until <= ?",
            vec![sval_str(now.clone())],
        )
        .await;
    st.db
        .query_one(
            "SELECT 1 AS hit FROM login_locks WHERE username = ? AND locked_until > ?",
            vec![sval_str(username.to_string()), sval_str(now)],
        )
        .await
        .ok()
        .flatten()
        .is_some()
}

async fn record_fail(st: &AppState, username: &str) {
    let now = crate::db::now_iso();
    let lock_until = (chrono::Utc::now() + chrono::Duration::minutes(LOCK_MINS))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    // DO UPDATE 中引用原行值：fails+1 达阈值 → 计数清零并落锁定截止时间
    let _ = st
        .db
        .execute(
            "INSERT INTO login_locks (username, fails, locked_until, updated_at) \
             VALUES (?, 1, '', ?) \
             ON CONFLICT(username) DO UPDATE SET \
               fails = CASE WHEN login_locks.fails + 1 >= ? THEN 0 ELSE login_locks.fails + 1 END, \
               locked_until = CASE WHEN login_locks.fails + 1 >= ? THEN ? ELSE login_locks.locked_until END, \
               updated_at = ?",
            vec![
                sval_str(username.to_string()),
                sval_str(now.clone()),
                sea_orm::Value::BigInt(Some(MAX_FAILS)),
                sea_orm::Value::BigInt(Some(MAX_FAILS)),
                sval_str(lock_until),
                sval_str(now),
            ],
        )
        .await;
}

async fn clear_fails(st: &AppState, username: &str) {
    let _ = st
        .db
        .execute(
            "DELETE FROM login_locks WHERE username = ?",
            vec![sval_str(username.to_string())],
        )
        .await;
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
    S: Send + Sync + Clone + 'static,
    AppState: axum::extract::FromRef<S>,
{
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or_else(|| ApiError::unauthorized("未登录"))?;
        let claims = verify(token).ok_or_else(|| ApiError::unauthorized("登录已过期"))?;

        // F4 修复：此前 Auth 只验签不查库，导致：
        //   (a) 管理员被停用后，手上 token 在剩余有效期内（最长 7 天）仍是全权限；
        //   (b) 用户改密后，旧设备 token 依然可用。
        // 现按请求回查 users：停用立即失效，token_version 被 bump 后立即失效。
        // 权限仍由 role 实时推导（不入库），保留原有「改角色即时生效」的设计。
        let st = AppState::from_ref(state);
        match sqlx_user_by_id(&st, &claims.sub).await? {
            Some(u) => {
                if u.status != 1 {
                    return Err(ApiError::unauthorized("账号已被停用"));
                }
                if u.token_version > claims.tv {
                    return Err(ApiError::unauthorized("登录已失效，请重新登录"));
                }
            }
            None => return Err(ApiError::unauthorized("账号不存在或已删除")),
        }
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
    if login_locked(&st, &req.username).await {
        return Err(ApiError::rate_limited("尝试次数过多，请 15 分钟后再试"));
    }
    let row = sqlx_user_by_username(&st, &req.username).await?;
    let Some(user) = row else {
        record_fail(&st, &req.username).await;
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
        record_fail(&st, &req.username).await;
        return Err(ApiError::unauthorized("用户名或密码错误"));
    }
    clear_fails(&st, &req.username).await;
    let now = now_secs();
    let claims = Claims {
        sub: user.id.clone(),
        username: user.username.clone(),
        role: user.role.clone(),
        tenant: user.tenant_id.clone(),
        tv: user.token_version,
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
            // token_version +1：改密后其它设备/旧 token 立即失效
            "UPDATE users SET password_hash = ?, must_change_password = 0, \
             token_version = COALESCE(token_version, 1) + 1, updated_at = ? WHERE id = ?",
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
    pub token_version: i64,
    pub must_change_password: i64,
    pub created_at: String,
    pub updated_at: String,
}

impl UserRow {
    fn id_num(&self) -> i64 {
        self.id.parse().unwrap_or(0)
    }
}

/// 用户查询：按列名取（username 或 id）
///
/// F4 修复：原实现手工拼接 SQL 并用 `replace('\'', "''")` 转义单引号，是全代码库
/// 唯一的非参数化查询——依赖手写转义正确性，一旦漏掉某个入参即 SQL 注入。
/// 改为绑定参数，由驱动处理转义。
async fn sqlx_user_by(
    st: &AppState,
    column: &str,
    value: &str,
) -> Result<Option<UserRow>, ApiError> {
    let sql = format!(
        "SELECT id, username, nickname, email, password_hash, role, tenant_id, status, \
         COALESCE(token_version, 1) AS token_version, must_change_password, created_at, updated_at \
         FROM users WHERE {column} = ? LIMIT 1"
    );
    let res = st
        .db
        .query_one(
            &sql,
            vec![sea_orm::Value::String(Some(value.to_string()))],
        )
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
        token_version: r.try_get::<i64>("", "token_version").unwrap_or(1),
        must_change_password: r.try_get::<i64>("", "must_change_password").unwrap_or(0),
        created_at: r.try_get("", "created_at").map_err(internal)?,
        updated_at: r.try_get("", "updated_at").map_err(internal)?,
    }))
}

async fn sqlx_user_by_username(st: &AppState, username: &str) -> Result<Option<UserRow>, ApiError> {
    sqlx_user_by(st, "username", username).await
}

/// 按 id 查用户（Auth 提取器逐请求校验状态用）
async fn sqlx_user_by_id(st: &AppState, id: &str) -> Result<Option<UserRow>, ApiError> {
    sqlx_user_by(st, "id", id).await
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

/// POST /api/admin/users/{id}/revoke-sessions —— 让该用户的全部已签发 token 立即失效
///
/// F4 配套：管理员被停用/离职、或用户怀疑会话泄露时，无需等 JWT 自然过期（最长 7 天），
/// 直接 bump token_version 即可踢下线。Auth 提取器逐请求比对版本号。
pub async fn revoke_sessions(
    State(st): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
) -> ApiResult {
    ensure(&auth, "team.users.update")?;
    let n = st
        .db
        .execute(
            "UPDATE users SET token_version = COALESCE(token_version, 1) + 1, updated_at = ? \
             WHERE id = ? AND tenant_id = ?",
            vec![
                sea_orm::Value::String(Some(crate::db::now_iso())),
                sea_orm::Value::String(Some(id)),
                sea_orm::Value::String(Some(st.tenant.clone())),
            ],
        )
        .await
        .map_err(|e| ApiError::bad(format!("更新失败：{e}")))?;
    if n == 0 {
        return Err(ApiError::not_found("用户不存在"));
    }
    ok(json!({ "revoked": true }))
}
