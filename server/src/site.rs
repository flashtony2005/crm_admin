//! 站点级设置（主题 / 模板 / 品牌）。
//!
//! - `GET /api/public/site`  免认证，读 site_settings KV，返回
//!   `{ theme, template, siteTitle, siteTagline }`（供公开站点套用发布者设定）。
//! - `PUT /api/admin/site`    需 `site.settings.update` 权限（Owner 通配），
//!   增量 upsert 上述 4 个 KV。
//!
//! site_settings 采用 key TEXT PRIMARY KEY 的 KV 模型；单租户固定 t_demo。

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use sea_orm::Value as SqlValue;
use serde_json::{json, Value};

use crate::auth::{ensure, Auth};
use crate::cmsdb::Row;
use crate::error::{ok, ApiError, ApiResult};
use crate::state::AppState;

/// 读 TEXT 列（NULL→空串）
fn s(r: &Row, col: &str) -> String {
    r.try_get::<String>("", col).unwrap_or_default()
}

/// KV 默认值（读不到时使用）
const DEFAULTS: &[(&str, &str)] = &[
    ("theme", "paper"),
    ("template", "default"),
    ("site_title", "LightPress"),
    ("site_tagline", "专注内容的现代发布平台"),
];

/// GET /api/public/site —— 公开站点配置（免认证）
pub async fn site(State(st): State<AppState>) -> ApiResult {
    let rows = st
        .db
        .query_all_statement(sea_orm::Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT key, value FROM site_settings WHERE tenant_id = ?",
            vec![SqlValue::String(Some(st.tenant.clone()))],
        ))
        .await
        .map_err(|e| ApiError::bad(format!("读取站点设置失败：{e}")))?;

    let mut map: std::collections::HashMap<String, String> =
        DEFAULTS.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect();
    for r in &rows {
        let k = s(r, "key");
        let v = s(r, "value");
        if !k.is_empty() {
            map.insert(k, v);
        }
    }

    Ok(Json(json!({
        "ok": true,
        "data": {
            "theme": map.get("theme").cloned().unwrap_or_else(|| "paper".into()),
            "template": map.get("template").cloned().unwrap_or_else(|| "default".into()),
            "siteTitle": map.get("site_title").cloned().unwrap_or_else(|| "LightPress".into()),
            "siteTagline": map.get("site_tagline").cloned().unwrap_or_default(),
        }
    })).into_response())
}

/// PUT /api/admin/site —— 增量更新站点设置（Owner 权限）
pub async fn update_site(
    State(st): State<AppState>,
    auth: Auth,
    Json(body): Json<Value>,
) -> ApiResult {
    ensure(&auth, "site.settings.update")?;
    let now = crate::db::now_iso();
    let keys = ["theme", "template", "site_title", "site_tagline"];
    for k in keys {
        if let Some(v) = body.get(k).and_then(|x| x.as_str()) {
            st.db
                .execute_statement(sea_orm::Statement::from_sql_and_values(
                    sea_orm::DatabaseBackend::Sqlite,
                    "INSERT INTO site_settings (key, value, tenant_id, created_at, updated_at) \
                     VALUES (?, ?, ?, ?, ?) \
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
                    vec![
                        SqlValue::String(Some(k.to_string())),
                        SqlValue::String(Some(v.to_string())),
                        SqlValue::String(Some(st.tenant.clone())),
                        SqlValue::String(Some(now.clone())),
                        SqlValue::String(Some(now.clone())),
                    ],
                ))
                .await
                .map_err(|e| ApiError::bad(format!("更新站点设置失败：{e}")))?;
        }
    }
    Ok((StatusCode::OK, Json(json!({ "ok": true }))).into_response())
}
