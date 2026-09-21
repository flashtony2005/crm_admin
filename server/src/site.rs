//! 站点级设置（主题 / 模板 / 品牌）。
//!
//! - `GET /api/public/site`  免认证，读 site_settings KV，返回
//!   `{ theme, template, siteTitle, siteTagline, homeTheme, homeTemplate,
//!      mainPort, home }`（供公开站点套用发布者设定）。
//! - `PUT /api/admin/site`    需 `site.settings.update` 权限（Owner 通配），
//!   增量 upsert 上述 KV。
//!
//! `homeTemplate` / `mainPort` 用于首页模板切换与多端口管理：
//! 同一套内容可在多个端口以不同模板并行测试（如 5199=coucouya、5197=fastshot），
//! 后台统一配置「激活模板 + 主端口」，页面读取后以角标显示自身状态。
//!
//! `home` 是首页区块的结构化内容（数据带 / 归属 / 编号条目 / 系列 / Web3 卡…），
//! 以 JSON 字符串存于 KV；读不到时返回 null，前端回落到内置默认内容。
//! 这样新增首页区块不必改表结构，也不必为每个区块单独建表。
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
    // 公开主页模板：coucouya=默认风格，fastshot=Fastshot 风格（独立目录/端口）
    ("home_template", "coucouya"),
    // 主端口：上线后对外提供服务的端口；测试端口可并存多个
    ("main_port", "5199"),
    // 注册门槛：off=开放注册；on=仅凭有效邀请码注册（创作者社区私域开关）
    ("member_invite_required", "off"),
    // 积分规则：每日签到奖励 / 邀请奖励（好友首次付费开通后发给邀请人）
    ("points_signin", "10"),
    ("points_invite_reward", "100"),
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

    // home：首页区块 JSON。存的是字符串，按 JSON 解析；解析失败或不存在 → null，
    // 由前端回落到内置默认内容，避免后台没配过首页时整站白屏。
    let home: Value = map
        .get("home")
        .and_then(|v| serde_json::from_str::<Value>(v).ok())
        .unwrap_or(Value::Null);

    // home_theme：公开主页（coucouya）的风格键，独立于本后台自身的 theme，
    // 避免与本套站点的 sepia/paper 等主题混淆。
    let home_theme = map.get("home_theme").cloned().unwrap_or_default();

    // home_template / main_port：首页模板与主端口（各模板测试端口并存，
    // 由后台统一配置哪个模板激活、哪个端口对外服务）。
    let home_template = map
        .get("home_template")
        .cloned()
        .unwrap_or_else(|| "coucouya".into());
    let main_port = map.get("main_port").cloned().unwrap_or_else(|| "5199".into());

    Ok(Json(json!({
        "ok": true,
        "data": {
            "theme": map.get("theme").cloned().unwrap_or_else(|| "paper".into()),
            "template": map.get("template").cloned().unwrap_or_else(|| "default".into()),
            "siteTitle": map.get("site_title").cloned().unwrap_or_else(|| "LightPress".into()),
            "siteTagline": map.get("site_tagline").cloned().unwrap_or_default(),
            "homeTheme": home_theme,
            "homeTemplate": home_template,
            "mainPort": main_port,
            "memberInviteRequired": map
                .get("member_invite_required")
                .cloned()
                .unwrap_or_else(|| "off".into()),
            "pointsSignin": map.get("points_signin").cloned().unwrap_or_else(|| "10".into()),
            "pointsInviteReward": map.get("points_invite_reward").cloned().unwrap_or_else(|| "100".into()),
            "home": home,
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

    // 收集待写入的 KV：标量 + home（首页区块 JSON）。home_theme 是公开主页
    // （coucouya）的独立风格键，与本后台自身 theme 解耦。
    let mut pairs: Vec<(String, String)> = Vec::new();
    // home_template / main_port：首页模板（coucouya | fastshot）与对外主端口。
    // member_invite_required：注册邀请制开关（off | on）。
    for k in [
        "theme",
        "template",
        "site_title",
        "site_tagline",
        "home_theme",
        "home_template",
        "main_port",
        "member_invite_required",
        "points_signin",
        "points_invite_reward",
    ] {
        if let Some(v) = body.get(k).and_then(|x| x.as_str()) {
            pairs.push((k.to_string(), v.to_string()));
        }
    }
    // home 允许传对象/数组（序列化存储）或已是 JSON 字符串（原样存储）。
    if let Some(v) = body.get("home") {
        if !v.is_null() {
            let text = match v {
                Value::String(x) => x.clone(),
                other => other.to_string(),
            };
            pairs.push(("home".to_string(), text));
        }
    }

    for (k, v) in pairs {
        st.db
            .execute_statement(sea_orm::Statement::from_sql_and_values(
                sea_orm::DatabaseBackend::Sqlite,
                "INSERT INTO site_settings (key, value, tenant_id, created_at, updated_at) \
                 VALUES (?, ?, ?, ?, ?) \
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
                vec![
                    SqlValue::String(Some(k)),
                    SqlValue::String(Some(v)),
                    SqlValue::String(Some(st.tenant.clone())),
                    SqlValue::String(Some(now.clone())),
                    SqlValue::String(Some(now.clone())),
                ],
            ))
            .await
            .map_err(|e| ApiError::bad(format!("更新站点设置失败：{e}")))?;
    }
    Ok((StatusCode::OK, Json(json!({ "ok": true }))).into_response())
}
