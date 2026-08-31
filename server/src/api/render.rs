//! Render Contract —— 整个 Domain Model V1 的验收出口。
//!
//! `GET /api/v1/sites/:site_id/render?route=/` 返回一份**与具体前端无关**的
//! 呈现契约：外部网站（Next / Astro / Vue / Svelte / React 皆可）只消费这份
//! JSON 就能渲染出完整页面，**完全不需要依赖 Admin UI**。
//!
//! 这就是「Site 只是呈现载体、外部网站只是人类可见的呈现层」落地的地方：
//! 内容从 Content 来，结构从 TemplateVersion 来，样式从 ThemeVersion 来，
//! 本接口只负责把三者按 binding 组装好。
//!
//! 契约形态：
//! ```json
//! {
//!   "schema": "admin.render.v1",
//!   "site": {...}, "template": {...}, "theme": {...},
//!   "navigation": {}, "sections": [{ "id","component","props","data" }]
//! }
//! ```

use axum::{
    extract::{Path, Query, State},
};
use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::{
    api::{int, js, opt, s, txt},
    auth::{ensure, Auth},
    error::{ApiError, ApiResult},
    state::AppState,
};

#[derive(Deserialize, Default)]
pub struct RenderQuery {
    /// 路由模式，如 `/`、`/article/:slug`
    pub route: Option<String>,
    /// 钉住模板版本（缺省取该模板最新已发布版本）
    pub version: Option<i64>,
    /// 是否纳入草稿版本（预览用）
    pub draft: Option<bool>,
}

/// GET /api/v1/sites/:site_id/render?route=/
pub async fn render(
    State(st): State<AppState>,
    auth: Auth,
    Path(site_id): Path<String>,
    Query(q): Query<RenderQuery>,
) -> ApiResult {
    ensure(&auth, "domain.render.view")?;

    let route = q.route.clone().unwrap_or_else(|| "/".into());
    let site = crate::api::sites::resolve(&st, &site_id).await?;
    let sid = txt(&site, "id");
    let draft = q.draft.unwrap_or(false);

    // ── 1. 路由 → 模板绑定 ──
    let binding = st
        .db
        .query_one(
            "SELECT * FROM site_templates
              WHERE site_id = ? AND route = ?
              ORDER BY is_default DESC, updated_at DESC LIMIT 1",
            vec![s(sid.clone()), s(route.clone())],
        )
        .await
        .map_err(crate::api::db_err("查询路由绑定失败"))?
        .ok_or_else(|| ApiError::not_found(format!("站点 {site_id} 未绑定路由 {route}")))?;

    let template_id = txt(&binding, "template_id");
    let template = crate::api::must_get(&st, "templates", &template_id).await?;

    // ── 2. 模板版本：钉版本 → 最新已发布 → 最新任意版本 ──
    let version_row = match q.version {
        Some(v) => st
            .db
            .query_one(
                "SELECT * FROM template_versions WHERE template_id = ? AND version = ? LIMIT 1",
                vec![s(template_id.clone()), sea_orm::Value::BigInt(Some(v))],
            )
            .await
            .map_err(crate::api::db_err("查询模板版本失败"))?
            .ok_or_else(|| ApiError::not_found(format!("模板版本 {v} 不存在")))?,
        None => {
            let status_clause = if draft { "1=1" } else { "status = 'published'" };
            st.db
                .query_one(
                    &format!(
                        "SELECT * FROM template_versions
                          WHERE template_id = ? AND {status_clause}
                          ORDER BY version DESC LIMIT 1"
                    ),
                    vec![s(template_id.clone())],
                )
                .await
                .map_err(crate::api::db_err("查询模板版本失败"))?
                // 没有任何已发布版本时退回草稿，避免预览场景直接 404
                .ok_or_else(|| ApiError::not_found("该模板尚无可用版本"))?
        }
    };
    let definition = js(&version_row, "definition_json");

    // ── 3. 主题：默认绑定 + 最新版本 ──
    let theme_binding = st
        .db
        .query_one(
            "SELECT * FROM site_themes WHERE site_id = ? ORDER BY is_default DESC, updated_at DESC LIMIT 1",
            vec![s(sid.clone())],
        )
        .await
        .map_err(crate::api::db_err("查询主题绑定失败"))?;

    let mut theme_payload = Value::Null;
    if let Some(tb) = theme_binding {
        let theme_id = txt(&tb, "theme_id");
        let theme = crate::api::must_get(&st, "themes", &theme_id).await?;
        let status_clause = if draft { "1=1" } else { "status = 'published'" };
        if let Some(tv) = st
            .db
            .query_one(
                &format!(
                    "SELECT * FROM theme_versions
                      WHERE theme_id = ? AND {status_clause}
                      ORDER BY version DESC LIMIT 1"
                ),
                vec![s(theme_id.clone())],
            )
            .await
            .map_err(crate::api::db_err("查询主题版本失败"))?
        {
            theme_payload = json!({
                "id": txt(&theme, "id"),
                "name": txt(&theme, "name"),
                "slug": txt(&theme, "slug"),
                "version": int(&tv, "version"),
                "tokens": js(&tv, "tokens_json"),
                "components": js(&tv, "components_json"),
                "overrides": js(&tb, "config_json"),
            });
        }
    }

    // ── 4. 按 binding 解析每个 section 的数据 ──
    let sections = match definition.get("sections").and_then(|v| v.as_array()) {
        Some(list) => {
            let mut out = Vec::with_capacity(list.len());
            for sec in list {
                out.push(resolve_section(&st, sec).await?);
            }
            out
        }
        None => vec![],
    };

    // ── 5. 导航：沿用既有 nav_links（旧表不在本轮重构范围），空则给空数组 ──
    let nav = st
        .db
        .query_all(
            "SELECT label, href, target FROM nav_links
              WHERE tenant_id = ? AND grp = 'nav' AND enabled = 1
              ORDER BY sort ASC",
            vec![s(st.tenant.clone())],
        )
        .await
        .map(|rows| {
            rows.iter()
                .map(|r| {
                    json!({
                        "label": txt(r, "label"),
                        "href": txt(r, "href"),
                        "target": opt(r, "target").unwrap_or_default(),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    crate::api::ok(json!({
        "schema": "admin.render.v1",
        "site": {
            "id": txt(&site, "id"),
            "name": txt(&site, "name"),
            "slug": txt(&site, "slug"),
            "domain": opt(&site, "domain"),
            "locale": txt(&site, "default_locale"),
            "timezone": txt(&site, "timezone"),
            "settings": js(&site, "settings_json"),
        },
        "template": {
            "id": txt(&template, "id"),
            "name": txt(&template, "name"),
            "slug": txt(&template, "slug"),
            "type": txt(&template, "type"),
            "version": int(&version_row, "version"),
            "definition": definition,
            "bindingConfig": js(&binding, "config_json"),
        },
        "theme": theme_payload,
        "navigation": { "nav": nav },
        "sections": sections,
    }))
}

// ── Section 数据解析 ──────────────────────────────────────────────────────

/// 把一个 section 声明（含 binding）解析成 `{ id, component, props, data }`。
///
/// binding.source 支持：
/// - `site.profile`    → 站点身份
/// - `profile.metrics` → profile 类型内容里的 metrics 字段
/// - `content`         → 按 type（可带 featured / limit）查内容列表
///
/// 解析失败**不打断整页**：该 section 退化为 `data: null` 并带上 error，
/// 这样一个坏掉的 binding 不会让整站白屏。
async fn resolve_section(st: &AppState, sec: &Value) -> Result<Value, ApiError> {
    let binding = sec.get("binding").cloned().unwrap_or(Value::Null);
    let source = binding.get("source").and_then(|v| v.as_str()).unwrap_or("");

    let data = match source {
        "site.profile" => fetch_site_profile(st).await,
        "profile.metrics" => fetch_profile_metrics(st).await,
        "content" => {
            let ctype = binding.get("type").and_then(|v| v.as_str()).unwrap_or("");
            let limit = binding.get("limit").and_then(|v| v.as_i64()).unwrap_or(20);
            let featured = binding.get("featured").and_then(|v| v.as_bool()).unwrap_or(false);
            fetch_contents(st, ctype, limit, featured).await
        }
        // 未知 source：显式置 null，让 Renderer 自行决定是否跳过
        _ => Ok(Value::Null),
    };

    let (payload, error) = match data {
        Ok(v) => (v, None),
        Err(e) => (Value::Null, Some(e.message)),
    };

    let mut m: Map<String, Value> = Map::new();
    m.insert("id".into(), sec.get("id").cloned().unwrap_or(Value::Null));
    m.insert(
        "component".into(),
        sec.get("component").cloned().unwrap_or(Value::Null),
    );
    m.insert(
        "props".into(),
        sec.get("props").cloned().unwrap_or_else(|| json!({})),
    );
    m.insert("binding".into(), binding);
    m.insert("data".into(), payload);
    if let Some(err) = error {
        m.insert("error".into(), err.into());
    }
    Ok(Value::Object(m))
}

async fn fetch_site_profile(st: &AppState) -> Result<Value, ApiError> {
    let r = st
        .db
        .query_one(
            "SELECT id, name, slug, description, default_locale, settings_json
               FROM sites ORDER BY updated_at DESC LIMIT 1",
            vec![],
        )
        .await
        .map_err(crate::api::db_err("查询站点身份失败"))?
        .ok_or_else(|| ApiError::not_found("尚无站点"))?;
    Ok(json!({
        "id": txt(&r, "id"),
        "name": txt(&r, "name"),
        "slug": txt(&r, "slug"),
        "description": opt(&r, "description"),
        "locale": txt(&r, "default_locale"),
        "settings": js(&r, "settings_json"),
    }))
}

/// profile 内容里的 metrics 字段（`data_json.metrics`）
async fn fetch_profile_metrics(st: &AppState) -> Result<Value, ApiError> {
    let r = st
        .db
        .query_one(
            "SELECT data_json FROM contents WHERE type = 'profile' LIMIT 1",
            vec![],
        )
        .await
        .map_err(crate::api::db_err("查询 profile 指标失败"))?;
    match r {
        Some(row) => Ok(js(&row, "data_json").get("metrics").cloned().unwrap_or(Value::Null)),
        None => Ok(Value::Null),
    }
}

/// 按内容类型取列表；featured 时只取 primary 挂载权重 ≥ 0.8 的高相关内容
async fn fetch_contents(
    st: &AppState,
    ctype: &str,
    limit: i64,
    featured: bool,
) -> Result<Value, ApiError> {
    if ctype.is_empty() {
        return Err(ApiError::bad("content binding 缺少 type"));
    }
    let featured_clause = if featured {
        "AND id IN (SELECT content_id FROM content_contexts WHERE role = 'primary' AND weight >= 0.8)"
    } else {
        ""
    };
    let sql = format!(
        "SELECT id, type, slug, title, summary, data_json, published_at, updated_at
           FROM contents
          WHERE type = ? AND status = 'published' {featured_clause}
          ORDER BY COALESCE(published_at, updated_at) DESC
          LIMIT ?"
    );
    let rows = st
        .db
        .query_all(
            &sql,
            vec![s(ctype.to_string()), sea_orm::Value::BigInt(Some(limit))],
        )
        .await
        .map_err(crate::api::db_err("查询内容失败"))?;

    let items = rows
        .iter()
        .map(|r| {
            json!({
                "id": txt(r, "id"),
                "type": txt(r, "type"),
                "slug": opt(r, "slug"),
                "title": opt(r, "title"),
                "summary": opt(r, "summary"),
                "data": js(r, "data_json"),
                "publishedAt": opt(r, "published_at"),
                "updatedAt": txt(r, "updated_at"),
            })
        })
        .collect::<Vec<_>>();
    Ok(Value::Array(items))
}
