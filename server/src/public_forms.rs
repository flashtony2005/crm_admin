//! 公开收集（B4）：无需登录的表单提交端点。
//!
//! `POST /api/public/forms/{id}/submit`  body: {name, phone?, interest?, note?}
//! - 校验表单存在且 status='published'；
//! - 落一条 form_submissions（原始数据留档）并给 forms.submissions +1；
//! - 若带 name+phone 则同时生成线索（leads，source=表单），去重：同表单同手机号
//!   已有未处理线索则跳过建线（防刷）。

use axum::{extract::{Path, State}, Json};
use sea_orm::Statement;
use serde_json::json;
use uuid::Uuid;

use crate::{db::now_iso, error::{ApiError, ApiResult, ok}, state::AppState};

fn sval(s: String) -> SqlValue {
    SqlValue::String(Some(s))
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct SubmitReq {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub phone: String,
    #[serde(default)]
    pub interest: String,
    #[serde(default)]
    pub note: String,
}

/// POST /api/public/forms/{id}/submit —— 公开入口（匿名）
pub async fn submit(
    State(st): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SubmitReq>,
) -> ApiResult {
    let form = st
        .db
        .query_one_statement(Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT title FROM forms WHERE id = ? AND tenant_id = ? AND status = 'published' LIMIT 1",
            vec![sval(id.clone()), sval(st.tenant.clone())],
        ))
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    let Some(row) = form else {
        return Err(ApiError::not_found("表单不存在或未开放"));
    };
    let form_title: String = row.try_get("", "title").unwrap_or_default();

    if req.name.trim().is_empty() {
        return Err(ApiError::bad("请填写姓名"));
    }
    let now = now_iso();

    // 原始提交留档
    st.db
        .execute_statement(Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Sqlite,
            "INSERT INTO form_submissions (id, tenant_id, form_id, data, created_at) VALUES (?, ?, ?, ?, ?)",
            vec![
                sval(Uuid::new_v4().to_string()),
                sval(st.tenant.clone()),
                sval(id.clone()),
                sval(json!(req).to_string()),
                sval(now.clone()),
            ],
        ))
        .await
        .map_err(|e| ApiError::bad(format!("写入失败：{e}")))?;

    // 计数 +1
    st.db
        .execute_statement(Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "UPDATE forms SET submissions = submissions + 1, updated_at = '{now}' WHERE id = '{}'",
                id.replace('\'', "''")
            ),
        ))
        .await
        .ok();

    // 有手机号 → 建线索（同表单同号去重）
    let mut lead_created = false;
    if !req.phone.trim().is_empty() && req.phone.trim().len() >= 6 {
        let dup = st
            .db
            .query_one_statement(Statement::from_sql_and_values(
                sea_orm::DatabaseBackend::Sqlite,
                "SELECT id FROM leads WHERE tenant_id = ? AND source LIKE ? AND phone = ? LIMIT 1",
                vec![
                    sval(st.tenant.clone()),
                    sval(format!("表单:{}%", form_title)),
                    sval(req.phone.trim().to_string()),
                ],
            ))
            .await
            .ok()
            .flatten();
        if dup.is_none() {
            st.db
                .execute_statement(Statement::from_sql_and_values(
                    sea_orm::DatabaseBackend::Sqlite,
                    "INSERT INTO leads (id, tenant_id, name, phone, interest, source, status, created_at, updated_at) \
                     VALUES (?, ?, ?, ?, ?, ?, 'new', ?, ?)",
                    vec![
                        sval(Uuid::new_v4().to_string()),
                        sval(st.tenant.clone()),
                        sval(req.name.trim().to_string()),
                        sval(req.phone.trim().to_string()),
                        sval(req.interest.trim().to_string()),
                        sval(format!("表单:{form_title}")),
                        sval(now.clone()),
                        sval(now),
                    ],
                ))
                .await
                .map_err(|e| ApiError::bad(format!("写入失败：{e}")))?;
            lead_created = true;
        }
    }

    ok(json!({ "received": true, "leadCreated": lead_created }))
}

use sea_orm::Value as SqlValue;

/// GET /api/public/forms/{id} —— 匿名读取表单标题（用于公开页渲染）
pub async fn get_public(State(st): State<AppState>, Path(id): Path<String>) -> ApiResult {
    let row = st
        .db
        .query_one_statement(Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT title, descr FROM forms WHERE id = ? AND tenant_id = ? AND status = 'published' LIMIT 1",
            vec![
                SqlValue::String(Some(id)),
                SqlValue::String(Some(st.tenant.clone())),
            ],
        ))
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    let Some(r) = row else { return Err(ApiError::not_found("表单不存在或未开放")) };
    ok(json!({
        "title": r.try_get::<String>("", "title").unwrap_or_default(),
        "descr": r.try_get::<String>("", "descr").unwrap_or_default(),
    }))
}
