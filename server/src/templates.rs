//! 首页模板管理：磁盘目录托管 + 上传(zip) + 激活/切换 + 静态服务。
//!
//! 设计要点：
//! - 模板以「目录」为单位落在 `templates/<slug>/`（可用环境变量 TEMPLATES_DIR 覆盖），
//!   每个目录必须有 `index.html`（可附带 css/js/图片等资源）。
//! - 注册表用 `templates/manifest.json` 记录元数据：active（当前激活 slug）、
//!   items[]（slug/name/kind/port/note/created_at）。kind=app 为外部已注册应用
//!   （如 coucouya:5199、fastshot:5197），kind=upload 为后台上传解包的 zip 包。
//! - 静态服务：`GET /t/:slug/*path` 与 `GET /t/active/*path` 直接读目录文件返回，
//!   供公开站点 / 后台实时预览同域加载（数据接口同源，无需 CORS）。
//! - 上传：`POST /api/admin/templates`（multipart 字段 `file`=zip，可选
//!   `slug`/`name`/`port`/`note`）解包到 `templates/<slug>/` 并登记。
//! - 切换：`POST /api/admin/templates/:slug/activate` 设定 active，并同步写
//!   site_settings.home_template（页面角标/部署读取的单一真相源）。
//! - 所有写操作需 `site.settings.update` 权限。
//!
//! 注意：本模块不依赖 SeaORM 实体，注册表走 JSON 文件，降低耦合；只有激活时
//! 才回写 site_settings（KV 表），便于部署侧按 `home_template` 路由主端口。

use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::path::PathBuf;
use std::sync::Mutex;

use axum::extract::{Multipart, Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use serde_json::json;
use zip::ZipArchive;

use crate::auth::{ensure, Auth};
use crate::error::{ok, ApiError, ApiResult};
use crate::state::AppState;

/// 写操作串行化（admin 低频，简单互斥即可，避免清单并发写丢）
static WRITE_LOCK: Mutex<()> = Mutex::new(());

/// 模板目录：环境变量 TEMPLATES_DIR 优先，缺省 cwd 下的 templates/
pub fn templates_dir() -> PathBuf {
    std::env::var("TEMPLATES_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("templates"))
}

fn manifest_path() -> PathBuf {
    templates_dir().join("manifest.json")
}

#[derive(Serialize, Deserialize, Clone)]
struct TplMeta {
    slug: String,
    name: String,
    /// app=外部已注册应用；upload=后台上传解包的 zip 包
    kind: String,
    /// 外部应用端口（app 类用）；upload 类留空
    port: String,
    note: String,
    created_at: String,
}

#[derive(Serialize, Deserialize, Default)]
struct Manifest {
    active: String,
    items: Vec<TplMeta>,
}

/// 启动期确保模板目录存在，并在清单缺失时种入内置模板（coucouya / fastshot）。
pub fn ensure_templates_dir() {
    let _ = std::fs::create_dir_all(templates_dir());
    if !manifest_path().exists() {
        let seed = Manifest {
            active: "coucouya".into(),
            items: vec![
                TplMeta {
                    slug: "coucouya".into(),
                    name: "可可鸭（默认）".into(),
                    kind: "app".into(),
                    port: "5199".into(),
                    note: "默认数据驱动首页".into(),
                    created_at: crate::db::now_iso(),
                },
                TplMeta {
                    slug: "fastshot".into(),
                    name: "Fastshot 风格".into(),
                    kind: "app".into(),
                    port: "5197".into(),
                    note: "Fastshot 视觉克隆（数据来自后台）".into(),
                    created_at: crate::db::now_iso(),
                },
            ],
        };
        let _ = write_manifest(&seed);
    }
}

fn read_manifest() -> Manifest {
    match std::fs::read_to_string(manifest_path()) {
        Ok(txt) => serde_json::from_str::<Manifest>(&txt).unwrap_or_default(),
        Err(_) => Manifest::default(),
    }
}

fn write_manifest(m: &Manifest) -> Result<(), ApiError> {
    let txt = serde_json::to_string_pretty(m).map_err(|e| ApiError::bad(format!("清单序列化失败：{e}")))?;
    std::fs::write(manifest_path(), txt).map_err(|e| ApiError::bad(format!("写入清单失败：{e}")))?;
    Ok(())
}

/// 把模板条目投影为公开/后台列表用的轻量结构
fn meta_to_json(m: &TplMeta, active: &str) -> serde_json::Value {
    json!({
        "slug": m.slug,
        "name": m.name,
        "kind": m.kind,
        "port": m.port,
        "note": m.note,
        "createdAt": m.created_at,
        "active": m.slug == active,
        // 预览地址：upload 类走后端同域 /t/<slug>/，app 类走外部端口
        "previewUrl": if m.kind == "upload" {
            format!("/t/{}/", m.slug)
        } else if !m.port.is_empty() {
            format!("http://127.0.0.1:{}/", m.port)
        } else {
            format!("/t/{}/", m.slug)
        },
    })
}

/// GET /api/public/templates —— 免认证，列出全部模板 + 当前激活 slug
pub async fn public_list() -> ApiResult {
    let m = read_manifest();
    let items: Vec<serde_json::Value> = m.items.iter().map(|x| meta_to_json(x, &m.active)).collect();
    ok(json!({ "items": items, "active": m.active }))
}

/// GET /api/admin/templates —— 同公开列表（含完整元数据），需 site.settings.update
pub async fn admin_list(auth: Auth) -> ApiResult {
    ensure(&auth, "site.settings.update")?;
    public_list().await
}

/// 写单个 site_settings KV（激活时同步 home_template）
async fn set_site_kv(st: &AppState, key: &str, val: &str) -> Result<(), ApiError> {
    let now = crate::db::now_iso();
    use sea_orm::{Statement, Value as SqlValue};
    st.db
        .execute_statement(Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Sqlite,
            "INSERT INTO site_settings (key, value, tenant_id, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?) \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            vec![
                SqlValue::String(Some(key.into())),
                SqlValue::String(Some(val.into())),
                SqlValue::String(Some(st.tenant.clone())),
                SqlValue::String(Some(now.clone())),
                SqlValue::String(Some(now)),
            ],
        ))
        .await
        .map_err(|e| ApiError::bad(format!("更新站点设置失败：{e}")))?;
    Ok(())
}

/// POST /api/admin/templates/:slug/activate —— 设为激活模板
pub async fn activate(
    State(st): State<AppState>,
    auth: Auth,
    Path(slug): Path<String>,
) -> ApiResult {
    ensure(&auth, "site.settings.update")?;
    // 注意：std::sync::MutexGuard 非 Send，不能跨 await 持有（否则 axum
    // Handler<_, _> 不成立）。先在同步块内完成 manifest 读写，再异步同步站点设置。
    {
        let _g = WRITE_LOCK.lock().unwrap();
        let mut m = read_manifest();
        if !m.items.iter().any(|x| x.slug == slug) {
            return Err(ApiError::not_found("模板不存在"));
        }
        m.active = slug.clone();
        write_manifest(&m)?;
    }
    // 同步单一真相源：页面角标/部署读取
    set_site_kv(&st, "home_template", &slug).await?;
    ok(json!({ "active": slug }))
}

/// PUT /api/admin/templates/:slug —— 更新 name/port/note（app 类常用）
pub async fn update_meta(
    State(_st): State<AppState>,
    auth: Auth,
    Path(slug): Path<String>,
    axum::Json(body): axum::Json<serde_json::Value>,
) -> ApiResult {
    ensure(&auth, "site.settings.update")?;
    let _g = WRITE_LOCK.lock().unwrap();
    let mut m = read_manifest();
    let Some(item) = m.items.iter_mut().find(|x| x.slug == slug) else {
        return Err(ApiError::not_found("模板不存在"));
    };
    if let Some(v) = body.get("name").and_then(|x| x.as_str()) {
        item.name = v.to_string();
    }
    if let Some(v) = body.get("port").and_then(|x| x.as_str()) {
        item.port = v.to_string();
    }
    if let Some(v) = body.get("note").and_then(|x| x.as_str()) {
        item.note = v.to_string();
    }
    write_manifest(&m)?;
    ok(json!({ "ok": true }))
}

/// DELETE /api/admin/templates/:slug —— 仅允许删除上传类（upload）模板
pub async fn remove(
    State(_st): State<AppState>,
    auth: Auth,
    Path(slug): Path<String>,
) -> ApiResult {
    ensure(&auth, "site.settings.update")?;
    let _g = WRITE_LOCK.lock().unwrap();
    let mut m = read_manifest();
    let Some(pos) = m.items.iter().position(|x| x.slug == slug) else {
        return Err(ApiError::not_found("模板不存在"));
    };
    if m.items[pos].kind != "upload" {
        return Err(ApiError::bad("内置/外部模板不可删除（仅上传类可删）"));
    }
    // 删目录
    let dir = templates_dir().join(&slug);
    let _ = std::fs::remove_dir_all(&dir);
    m.items.remove(pos);
    if m.active == slug {
        m.active = m
            .items
            .iter()
            .find(|x| x.kind == "app")
            .map(|x| x.slug.clone())
            .unwrap_or_default();
    }
    write_manifest(&m)?;
    ok(json!({ "ok": true }))
}

/// POST /api/admin/templates —— multipart 上传 zip 包并解包登记
pub async fn upload(
    State(_st): State<AppState>,
    auth: Auth,
    mut mp: Multipart,
) -> ApiResult {
    ensure(&auth, "site.settings.update")?;

    // 文本字段（slug/name/port/note）与文件字段一并解析
    let mut slug = String::new();
    let mut name = String::new();
    let mut port = String::new();
    let mut note = String::new();
    let mut zip_bytes: Option<Vec<u8>> = None;
    let mut file_name = String::new();

    while let Some(field) = mp
        .next_field()
        .await
        .map_err(|e| ApiError::bad(format!("解析上传失败：{e}")))?
    {
        let fname = field.name().unwrap_or("").to_string();
        if fname == "file" {
            file_name = field.file_name().unwrap_or("template.zip").to_string();
            let bytes = field
                .bytes()
                .await
                .map_err(|e| ApiError::bad(format!("读取文件失败：{e}")))?;
            if bytes.is_empty() {
                return Err(ApiError::bad("上传文件为空"));
            }
            if bytes.len() > 20 * 1024 * 1024 {
                return Err(ApiError::bad("模板包超过 20MB 上限"));
            }
            zip_bytes = Some(bytes.to_vec());
        } else if fname == "slug" {
            slug = field.text().await.unwrap_or_default();
        } else if fname == "name" {
            name = field.text().await.unwrap_or_default();
        } else if fname == "port" {
            port = field.text().await.unwrap_or_default();
        } else if fname == "note" {
            note = field.text().await.unwrap_or_default();
        }
    }

    let zip_bytes = zip_bytes.ok_or_else(|| ApiError::bad("未收到模板包（请使用字段名 file，zip 格式）"))?;

    // slug：显式提供则校验；否则取文件名主干（去扩展名、转安全 slug）
    let slug = if slug.trim().is_empty() {
        let stem = file_name.rsplit('/').next().unwrap_or("template");
        let stem = stem.rsplit('.').next().unwrap_or("template");
        sanitize_slug(stem)
    } else {
        sanitize_slug(&slug)
    };
    if slug.is_empty() {
        return Err(ApiError::bad("无法生成有效 slug"));
    }

    let _g = WRITE_LOCK.lock().unwrap();
    let mut m = read_manifest();
    if m.items.iter().any(|x| x.slug == slug) {
        return Err(ApiError::bad(format!("slug 已存在：{slug}（请换名或先删除）")));
    }

    // 解包 zip 到 templates/<slug>/
    let dest = templates_dir().join(&slug);
    extract_zip(&zip_bytes, &dest)?;
    if !dest.join("index.html").exists() {
        let _ = std::fs::remove_dir_all(&dest);
        return Err(ApiError::bad("模板包缺少 index.html（须含入口文件）"));
    }

    let display_name = if name.trim().is_empty() { slug.clone() } else { name };
    m.items.push(TplMeta {
        slug: slug.clone(),
        name: display_name.clone(),
        kind: "upload".into(),
        port,
        note,
        created_at: crate::db::now_iso(),
    });
    write_manifest(&m)?;

    ok(json!({ "slug": slug, "name": display_name, "kind": "upload" }))
}

/// 文件名/输入 → 安全 slug（仅保留字母数字与 -_，小写化）
fn sanitize_slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if c == ' ' || c == '-' || c == '_' || c == '.' {
            out.push('-');
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

/// 把 zip 字节解包到 dest（防目录穿越）
fn extract_zip(bytes: &[u8], dest: &std::path::Path) -> Result<(), ApiError> {
    std::fs::create_dir_all(dest).map_err(|e| ApiError::bad(format!("创建目录失败：{e}")))?;
    let reader = Cursor::new(bytes);
    let mut archive = ZipArchive::new(reader).map_err(|e| ApiError::bad(format!("ZIP 解析失败：{e}")))?;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| ApiError::bad(format!("读取 ZIP 条目失败：{e}")))?;
        let Some(rel) = entry.enclosed_name().map(|p| p.to_path_buf()) else {
            continue;
        };
        let out = dest.join(&rel);
        // 目录穿越防护
        if !out.starts_with(dest) {
            continue;
        }
        if entry.is_dir() {
            let _ = std::fs::create_dir_all(&out);
            continue;
        }
        if let Some(parent) = out.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let mut buf = Vec::with_capacity(entry.size() as usize);
        entry
            .read_to_end(&mut buf)
            .map_err(|e| ApiError::bad(format!("解包条目失败：{e}")))?;
        std::fs::write(&out, &buf).map_err(|e| ApiError::bad(format!("写入文件失败：{e}")))?;
    }
    Ok(())
}

/// 简易 content-type 推断（与 main.rs 的静态服务对齐）
fn ct_of(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("").to_ascii_lowercase().as_str() {
        "js" | "mjs" => "application/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "html" | "htm" => "text/html; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff" | "woff2" => "font/woff2",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// 读取模板目录下某个相对路径的文件（含 index.html 兜底）
async fn serve_file(slug: &str, rest: &str) -> Response {
    let base = templates_dir().join(slug);
    if !base.exists() {
        return StatusCode::NOT_FOUND.into_response();
    }
    // 防穿越：解析后必须仍在 base 内
    let rel = rest.trim_start_matches('/');
    let target = if rel.is_empty() {
        base.join("index.html")
    } else {
        base.join(rel)
    };
    if target != base && !target.starts_with(&base) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    // 目录 → index.html
    let target = if target.is_dir() {
        target.join("index.html")
    } else {
        target
    };
    match tokio::fs::read(&target).await {
        Ok(bytes) => (
            [(header::CONTENT_TYPE, ct_of(&target.to_string_lossy()))],
            bytes,
        )
            .into_response(),
        Err(_) => {
            // SPA 深链兜底：回退到 index.html
            let idx = base.join("index.html");
            match tokio::fs::read(&idx).await {
                Ok(bytes) => (
                    [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
                    bytes,
                )
                    .into_response(),
                Err(_) => StatusCode::NOT_FOUND.into_response(),
            }
        }
    }
}

/// GET /t/:slug —— 提供模板入口 index.html（无尾斜杠）
pub async fn serve_index(Path(slug): Path<String>) -> Response {
    if slug == "manifest.json" {
        return StatusCode::NOT_FOUND.into_response();
    }
    serve_file(&slug, "").await
}

/// GET /t/:slug/*rest —— 按 slug 提供模板静态文件
pub async fn serve_one(Path((slug, rest)): Path<(String, String)>) -> Response {
    // 拒绝清单文件本身被直接访问
    if slug == "manifest.json" {
        return StatusCode::NOT_FOUND.into_response();
    }
    serve_file(&slug, &rest).await
}

/// GET /t/active —— 提供当前激活模板入口 index.html（无尾斜杠）
pub async fn serve_active_index() -> Response {
    let m = read_manifest();
    if m.active.is_empty() {
        return StatusCode::NOT_FOUND.into_response();
    }
    serve_file(&m.active, "").await
}

/// GET /t/active/*rest —— 提供当前激活模板的静态文件
pub async fn serve_active(Path(rest): Path<String>) -> Response {
    let m = read_manifest();
    if m.active.is_empty() {
        return StatusCode::NOT_FOUND.into_response();
    }
    serve_file(&m.active, &rest).await
}
