//! Site API —— 呈现载体 + 它的呈现绑定（模板路由 / 主题）。
//!
//! 这里最能体现设计原则：**Site 自身不存任何呈现 JSON**。
//! 模板与主题都通过绑定表关联，所以「换模板 / 换主题 / 同一路由挂多个模板」
//! 全是绑定层的操作，不动站点记录本身。
//!
//! 写权限刻意只给 owner（`perm.rs` 注释有说明）：站点配置改动面大，
//! editor 的修改应走审批。

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::{
    api::{b_js, b_str, js, must_get, now, ok, ok_empty, ok_id, ok_list, opt, s, so, txt},
    auth::{ensure, Auth},
    error::{ApiError, ApiResult},
    state::AppState,
};

fn site_json(r: &crate::cmsdb::Row) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("id".into(), txt(r, "id").into());
    m.insert("name".into(), txt(r, "name").into());
    m.insert("slug".into(), txt(r, "slug").into());
    m.insert("domain".into(), opt(r, "domain").into());
    m.insert("description".into(), opt(r, "description").into());
    m.insert("defaultLocale".into(), txt(r, "default_locale").into());
    m.insert("timezone".into(), txt(r, "timezone").into());
    m.insert("status".into(), txt(r, "status").into());
    m.insert("settings".into(), js(r, "settings_json"));
    m.insert("metadata".into(), js(r, "metadata_json"));
    m.insert("createdAt".into(), txt(r, "created_at").into());
    m.insert("updatedAt".into(), txt(r, "updated_at").into());
    m
}

/// 按 id 或 slug 定位站点（对外路径两种都支持，前端拿哪个都能用）
pub(crate) async fn resolve(
    st: &AppState,
    id_or_slug: &str,
) -> Result<crate::cmsdb::Row, ApiError> {
    st.db
        .query_one(
            "SELECT * FROM sites WHERE id = ? OR slug = ? LIMIT 1",
            vec![s(id_or_slug.to_string()), s(id_or_slug.to_string())],
        )
        .await
        .map_err(crate::api::db_err("查询站点失败"))?
        .ok_or_else(|| ApiError::not_found(format!("站点不存在：{id_or_slug}")))
}

// ── 基础 CRUD ─────────────────────────────────────────────────────────────

/// GET /api/v1/sites
pub async fn list(State(st): State<AppState>, auth: Auth) -> ApiResult {
    ensure(&auth, "domain.site.view")?;
    let rows = st
        .db
        .query_all(
            "SELECT * FROM sites ORDER BY updated_at DESC",
            vec![],
        )
        .await
        .map_err(crate::api::db_err("查询站点失败"))?;
    let items = rows.iter().map(|r| Value::Object(site_json(r))).collect::<Vec<_>>();
    let total = items.len();
    ok_list(items, total)
}

/// GET /api/v1/sites/:id
pub async fn get_one(
    State(st): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
) -> ApiResult {
    ensure(&auth, "domain.site.view")?;
    let r = resolve(&st, &id).await?;
    ok(Value::Object(site_json(&r)))
}

/// POST /api/v1/sites
pub async fn create(State(st): State<AppState>, auth: Auth, Json(body): Json<Value>) -> ApiResult {
    ensure(&auth, "domain.site.create")?;
    let name = b_str(&body, "name").ok_or_else(|| ApiError::bad("缺少 name"))?;
    let slug = b_str(&body, "slug").ok_or_else(|| ApiError::bad("缺少 slug"))?;
    let id = uuid::Uuid::new_v4().to_string();
    let ts = now();

    if st
        .db
        .query_one("SELECT id FROM sites WHERE slug = ? LIMIT 1", vec![s(slug.clone())])
        .await
        .map_err(crate::api::db_err("检查 slug 失败"))?
        .is_some()
    {
        return Err(ApiError::bad(format!("slug 已存在：{slug}")));
    }

    st.db
        .execute(
            "INSERT INTO sites
             (id, name, slug, domain, description, default_locale, timezone, status, settings_json, metadata_json, created_at, updated_at)
             VALUES (?,?,?,?,?,?,?,?,?,?,?,?)",
            vec![
                s(id.clone()),
                s(name),
                s(slug),
                so(b_str(&body, "domain").filter(|v| !v.is_empty())),
                so(b_str(&body, "description")),
                s(b_str(&body, "defaultLocale").unwrap_or_else(|| "en-US".into())),
                s(b_str(&body, "timezone").unwrap_or_else(|| "UTC".into())),
                s(b_str(&body, "status").unwrap_or_else(|| "draft".into())),
                s(crate::api::to_json_text(b_js(&body, "settings").as_ref())),
                s(crate::api::to_json_text(b_js(&body, "metadata").as_ref())),
                s(ts.clone()),
                s(ts),
            ],
        )
        .await
        .map_err(crate::api::db_err("创建站点失败"))?;

    ok_id(id)
}

/// PATCH /api/v1/sites/:id
pub async fn update(
    State(st): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> ApiResult {
    ensure(&auth, "domain.site.update")?;
    let cur = resolve(&st, &id).await?;
    let site_id = txt(&cur, "id");

    let mut sets: Vec<String> = vec![];
    let mut vals: Vec<sea_orm::Value> = vec![];
    if let Some(v) = b_str(&body, "name") {
        sets.push("name = ?".into());
        vals.push(s(v));
    }
    if let Some(v) = b_str(&body, "domain") {
        sets.push("domain = ?".into());
        vals.push(so(if v.is_empty() { None } else { Some(v) }));
    }
    if let Some(v) = b_js(&body, "description") {
        sets.push("description = ?".into());
        vals.push(so(v.as_str().map(|x| x.to_string())));
    }
    if let Some(v) = b_str(&body, "defaultLocale") {
        sets.push("default_locale = ?".into());
        vals.push(s(v));
    }
    if let Some(v) = b_str(&body, "timezone") {
        sets.push("timezone = ?".into());
        vals.push(s(v));
    }
    if let Some(v) = b_str(&body, "status") {
        sets.push("status = ?".into());
        vals.push(s(v));
    }
    if let Some(v) = b_js(&body, "settings") {
        sets.push("settings_json = ?".into());
        vals.push(s(crate::api::to_json_text(Some(&v))));
    }
    if let Some(v) = b_js(&body, "metadata") {
        sets.push("metadata_json = ?".into());
        vals.push(s(crate::api::to_json_text(Some(&v))));
    }

    if sets.is_empty() {
        return ok_id(site_id);
    }
    sets.push("updated_at = ?".into());
    vals.push(s(now()));
    vals.push(s(site_id.clone()));

    let sql = format!("UPDATE sites SET {} WHERE id = ?", sets.join(", "));
    st.db
        .execute(&sql, vals)
        .await
        .map_err(crate::api::db_err("更新站点失败"))?;

    ok_id(site_id)
}

/// DELETE /api/v1/sites/:id
pub async fn remove(
    State(st): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
) -> ApiResult {
    ensure(&auth, "domain.site.update")?;
    let cur = resolve(&st, &id).await?;
    st.db
        .execute("DELETE FROM sites WHERE id = ?", vec![s(txt(&cur, "id"))])
        .await
        .map_err(crate::api::db_err("删除站点失败"))?;
    ok_empty()
}

// ── Site ↔ Template 路由绑定 ──────────────────────────────────────────────

/// GET /api/v1/sites/:id/templates —— 路由 → 模板 绑定表
pub async fn list_templates(
    State(st): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
) -> ApiResult {
    ensure(&auth, "domain.site.view")?;
    let site = resolve(&st, &id).await?;
    let rows = st
        .db
        .query_all(
            "SELECT t.id AS id, t.name AS name, t.slug AS slug, t.type AS type,
                    st.route AS route, st.is_default AS is_default, st.config_json AS config_json,
                    (SELECT MAX(v.version) FROM template_versions v WHERE v.template_id = t.id) AS latest_version
               FROM site_templates st
               JOIN templates t ON t.id = st.template_id
              WHERE st.site_id = ?
              ORDER BY st.route ASC, st.is_default DESC",
            vec![s(txt(&site, "id"))],
        )
        .await
        .map_err(crate::api::db_err("查询路由绑定失败"))?;
    let items = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "templateId": txt(r, "id"),
                "templateName": txt(r, "name"),
                "templateSlug": txt(r, "slug"),
                "templateType": txt(r, "type"),
                "route": txt(r, "route"),
                "isDefault": crate::api::int(r, "is_default") != 0,
                "latestVersion": crate::api::int(r, "latest_version"),
                "config": js(r, "config_json"),
            })
        })
        .collect::<Vec<_>>();
    let total = items.len();
    ok_list(items, total)
}

#[derive(Deserialize)]
pub struct BindTemplateReq {
    pub template_id: String,
    pub route: String,
    pub is_default: Option<bool>,
    pub config: Option<Value>,
}

/// POST /api/v1/sites/:id/templates —— 绑定（已存在则更新）
pub async fn bind_template(
    State(st): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
    Json(body): Json<BindTemplateReq>,
) -> ApiResult {
    ensure(&auth, "domain.site.update")?;
    let site = resolve(&st, &id).await?;
    must_get(&st, "templates", &body.template_id).await?;
    if body.route.trim().is_empty() {
        return Err(ApiError::bad("route 不能为空"));
    }
    let ts = now();
    let is_default = if body.is_default.unwrap_or(true) { 1 } else { 0 };

    // 设为默认时，先清掉同一路由上的其它默认，保证一个路由只有一个生效模板
    if is_default == 1 {
        st.db
            .execute(
                "UPDATE site_templates SET is_default = 0, updated_at = ?
                  WHERE site_id = ? AND route = ?",
                vec![s(ts.clone()), s(txt(&site, "id")), s(body.route.clone())],
            )
            .await
            .map_err(crate::api::db_err("重置默认模板失败"))?;
    }

    st.db
        .execute(
            "INSERT INTO site_templates (site_id, template_id, route, is_default, config_json, created_at, updated_at)
             VALUES (?,?,?,?,?,?,?)
             ON CONFLICT(site_id, template_id, route)
             DO UPDATE SET is_default = excluded.is_default,
                           config_json = excluded.config_json,
                           updated_at = excluded.updated_at",
            vec![
                s(txt(&site, "id")),
                s(body.template_id.clone()),
                s(body.route.clone()),
                sea_orm::Value::BigInt(Some(is_default)),
                s(crate::api::to_json_text(body.config.as_ref())),
                s(ts.clone()),
                s(ts),
            ],
        )
        .await
        .map_err(crate::api::db_err("绑定模板失败"))?;

    ok_id(body.template_id)
}

#[derive(Deserialize)]
pub struct UnbindTemplateQuery {
    pub template_id: String,
    pub route: String,
}

/// DELETE /api/v1/sites/:id/templates?template_id=&route=
pub async fn unbind_template(
    State(st): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
    Query(q): Query<UnbindTemplateQuery>,
) -> ApiResult {
    ensure(&auth, "domain.site.update")?;
    let site = resolve(&st, &id).await?;
    st.db
        .execute(
            "DELETE FROM site_templates WHERE site_id = ? AND template_id = ? AND route = ?",
            vec![s(txt(&site, "id")), s(q.template_id), s(q.route)],
        )
        .await
        .map_err(crate::api::db_err("解绑模板失败"))?;
    ok_empty()
}

// ── Site ↔ Theme 绑定 ─────────────────────────────────────────────────────

/// GET /api/v1/sites/:id/themes
pub async fn list_themes(
    State(st): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
) -> ApiResult {
    ensure(&auth, "domain.site.view")?;
    let site = resolve(&st, &id).await?;
    let rows = st
        .db
        .query_all(
            "SELECT th.id AS id, th.name AS name, th.slug AS slug,
                    sth.is_default AS is_default, sth.config_json AS config_json,
                    (SELECT MAX(v.version) FROM theme_versions v WHERE v.theme_id = th.id) AS latest_version
               FROM site_themes sth
               JOIN themes th ON th.id = sth.theme_id
              WHERE sth.site_id = ?
              ORDER BY sth.is_default DESC, th.name ASC",
            vec![s(txt(&site, "id"))],
        )
        .await
        .map_err(crate::api::db_err("查询主题绑定失败"))?;
    let items = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "themeId": txt(r, "id"),
                "themeName": txt(r, "name"),
                "themeSlug": txt(r, "slug"),
                "isDefault": crate::api::int(r, "is_default") != 0,
                "latestVersion": crate::api::int(r, "latest_version"),
                "config": js(r, "config_json"),
            })
        })
        .collect::<Vec<_>>();
    let total = items.len();
    ok_list(items, total)
}

#[derive(Deserialize)]
pub struct BindThemeReq {
    pub theme_id: String,
    pub is_default: Option<bool>,
    pub config: Option<Value>,
}

/// POST /api/v1/sites/:id/themes —— 挂载主题（设为默认时先清其它默认）
pub async fn bind_theme(
    State(st): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
    Json(body): Json<BindThemeReq>,
) -> ApiResult {
    ensure(&auth, "domain.site.update")?;
    let site = resolve(&st, &id).await?;
    must_get(&st, "themes", &body.theme_id).await?;
    let ts = now();
    let is_default = if body.is_default.unwrap_or(true) { 1 } else { 0 };
    let site_id = txt(&site, "id");

    if is_default == 1 {
        st.db
            .execute(
                "UPDATE site_themes SET is_default = 0, updated_at = ? WHERE site_id = ?",
                vec![s(ts.clone()), s(site_id.clone())],
            )
            .await
            .map_err(crate::api::db_err("重置默认主题失败"))?;
    }

    st.db
        .execute(
            "INSERT INTO site_themes (site_id, theme_id, is_default, config_json, created_at, updated_at)
             VALUES (?,?,?,?,?,?)
             ON CONFLICT(site_id, theme_id)
             DO UPDATE SET is_default = excluded.is_default,
                           config_json = excluded.config_json,
                           updated_at = excluded.updated_at",
            vec![
                s(site_id),
                s(body.theme_id.clone()),
                sea_orm::Value::BigInt(Some(is_default)),
                s(crate::api::to_json_text(body.config.as_ref())),
                s(ts.clone()),
                s(ts),
            ],
        )
        .await
        .map_err(crate::api::db_err("挂载主题失败"))?;

    ok_id(body.theme_id)
}

/// DELETE /api/v1/sites/:id/themes?theme_id=
pub async fn unbind_theme(
    State(st): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> ApiResult {
    ensure(&auth, "domain.site.update")?;
    let site = resolve(&st, &id).await?;
    let theme_id = q
        .get("theme_id")
        .or_else(|| q.get("themeId"))
        .ok_or_else(|| ApiError::bad("缺少 theme_id"))?;
    st.db
        .execute(
            "DELETE FROM site_themes WHERE site_id = ? AND theme_id = ?",
            vec![s(txt(&site, "id")), s(theme_id.clone())],
        )
        .await
        .map_err(crate::api::db_err("解绑主题失败"))?;
    ok_empty()
}
