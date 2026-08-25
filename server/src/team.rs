//! 团队成员管理（Phase 3）。
//! 用户不是普通资源（含密码哈希），走专用 handler 而非通用网关；
//! 权限：查看 team.users.view、邀请/变更 team.users.invite。

use axum::{extract::{Path, State}, Json};
use sea_orm::{Statement, Value as SqlValue};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    auth::{ensure, Auth, hash_password},
    db::now_iso,
    error::{ok, ok_list, ApiError, ApiResult},
    perm,
    state::AppState,
};

fn sval(s: String) -> SqlValue {
    SqlValue::String(Some(s))
}

/// GET /api/team/users
pub async fn list(State(st): State<AppState>, auth: Auth) -> ApiResult {
    ensure(&auth, "team.users.view")?;
    let rows = st
        .db
        .query_all_statement(Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT id, username, nickname, role, status, created_at FROM users \
                 WHERE tenant_id = '{}' ORDER BY created_at ASC",
                st.tenant
            ),
        ))
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    let items: Vec<Value> = rows
        .iter()
        .filter_map(|r| {
            Some(json!({
                "id": r.try_get::<String>("", "id").ok()?,
                "username": r.try_get::<String>("", "username").ok()?,
                "nickname": r.try_get::<String>("", "nickname").ok()?,
                "role": r.try_get::<String>("", "role").ok()?,
                "status": r.try_get::<i64>("", "status").ok()?,
                "createdAt": r.try_get::<String>("", "created_at").ok()?,
            }))
        })
        .collect();
    let total = items.len();
    ok_list(items, total)
}

#[derive(serde::Deserialize)]
pub struct InviteReq {
    pub username: String,
    pub nickname: String,
    pub role: String,
}

/// POST /api/team/users —— 邀请成员（初始密码 demo1234，首登后应改密，Phase 3+）
pub async fn invite(State(st): State<AppState>, auth: Auth, Json(req): Json<InviteReq>) -> ApiResult {
    ensure(&auth, "team.users.invite")?;
    let username = req.username.trim().to_lowercase();
    if username.len() < 3 {
        return Err(ApiError::bad("用户名至少 3 个字符"));
    }
    if perm::Role::from_str(&req.role).is_none() {
        return Err(ApiError::bad("角色必须是 owner | editor | viewer"));
    }
    // 唯一性
    let dup = st
        .db
        .query_one_statement(Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT id FROM users WHERE username = ? LIMIT 1",
            vec![sval(username.clone())],
        ))
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    if dup.is_some() {
        return Err(ApiError::bad("用户名已存在"));
    }

    let id = Uuid::new_v4().to_string();
    let now = now_iso();
    st.db
        .execute_statement(Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Sqlite,
            "INSERT INTO users (id, username, nickname, password_hash, role, tenant_id, status, must_change_password, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, 1, 1, ?, ?)",
            vec![
                sval(id.clone()),
                sval(username.clone()),
                sval(req.nickname.trim().to_string()),
                sval(hash_password("demo1234")),
                sval(req.role.clone()),
                sval(st.tenant.clone()),
                sval(now.clone()),
                sval(now),
            ],
        ))
        .await
        .map_err(|e| ApiError::bad(format!("创建失败：{e}")))?;
    ok(json!({ "id": id, "username": username, "nickname": req.nickname, "role": req.role, "status": 1 }))
}

/// PUT /api/team/users/{id} —— 变更角色 / 启停（白名单字段）
pub async fn update_member(
    State(st): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> ApiResult {
    ensure(&auth, "team.users.invite")?;

    // 防锁死：不允许把自己降级/停用（否则可能失去最后一个 owner）
    if auth.0.sub == id {
        if let Some(status) = body.get("status") {
            if status.as_i64() == Some(0) {
                return Err(ApiError::bad("不能停用当前登录账号"));
            }
        }
    }

    let mut sets: Vec<String> = Vec::new();
    let mut vals: Vec<SqlValue> = Vec::new();
    if let Some(role) = body.get("role").and_then(|v| v.as_str()) {
        if perm::Role::from_str(role).is_none() {
            return Err(ApiError::bad("角色必须是 owner | editor | viewer"));
        }
        sets.push("role = ?".into());
        vals.push(sval(role.to_string()));
    }
    if let Some(nickname) = body.get("nickname").and_then(|v| v.as_str()) {
        sets.push("nickname = ?".into());
        vals.push(sval(nickname.to_string()));
    }
    if let Some(status) = body.get("status").and_then(|v| v.as_i64()) {
        if status != 0 && status != 1 {
            return Err(ApiError::bad("status 必须为 0 或 1"));
        }
        sets.push("status = ?".into());
        vals.push(SqlValue::BigInt(Some(status)));
    }
    if sets.is_empty() {
        return Err(ApiError::bad("没有可更新的字段"));
    }
    sets.push("updated_at = ?".into());
    vals.push(sval(now_iso()));
    vals.push(sval(id.clone()));
    vals.push(sval(st.tenant.clone()));

    st.db
        .execute_statement(Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Sqlite,
            format!("UPDATE users SET {} WHERE id = ? AND tenant_id = ?", sets.join(", ")),
            vals,
        ))
        .await
        .map_err(|e| ApiError::bad(format!("更新失败：{e}")))?;

    let row = st
        .db
        .query_one_statement(Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT id, username, nickname, role, status, created_at FROM users WHERE id = ? AND tenant_id = ?",
            vec![sval(id), sval(st.tenant.clone())],
        ))
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    match row {
        None => Err(ApiError::not_found("成员不存在")),
        Some(r) => ok(json!({
            "id": r.try_get::<String>("", "id").map_err(internal)?,
            "username": r.try_get::<String>("", "username").map_err(internal)?,
            "nickname": r.try_get::<String>("", "nickname").map_err(internal)?,
            "role": r.try_get::<String>("", "role").map_err(internal)?,
            "status": r.try_get::<i64>("", "status").map_err(internal)?,
            "createdAt": r.try_get::<String>("", "created_at").map_err(internal)?,
        })),
    }
}

fn internal(e: impl std::fmt::Display) -> ApiError {
    ApiError::bad(format!("行解析失败：{e}"))
}
