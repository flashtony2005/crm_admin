//! 首页模板管理：磁盘目录托管 + 上传(zip) + 激活/切换 + 静态服务。
//!
//! 设计要点：
//! - 模板以「目录」为单位落在 `templates/<slug>/`（可用环境变量 TEMPLATES_DIR 覆盖），
//!   每个目录必须有 `index.html`（可附带 css/js/图片等资源）。
//! - 注册表用 `templates/manifest.json` 记录元数据：active（当前激活 slug）、
//!   items[]（slug/name/kind/port/note/created_at）。kind=app 为外部已注册应用
//!   （如 coucouya、fastshot），kind=upload 为后台上传解包的 zip 包。
//! - 静态服务：`GET /t/:slug/*path` 直接读目录文件返回，供公开站点 / 后台实时预览
//!   同域加载（数据接口同源，无需 CORS）。`GET /t/active/*path` 则是 **302 跳转**
//!   到 `/t/<active>/*path`（激活 slug 由 `site_settings.home_template` 决定，
//!   回落 manifest.active）—— 页面只在规范路径下渲染，避免前端 base 与
//!   pathname 不一致导致深链被判 404，同时消除重复内容。
//! - 上传：`POST /api/admin/templates`（multipart 字段 `file`=zip，可选
//!   `slug`/`name`/`port`/`note`）解包到 `templates/<slug>/` 并登记。
//! - 切换：`POST /api/admin/templates/:slug/activate` 设定 active，并同步写
//!   site_settings.home_template（页面角标/部署读取的单一真相源）。
//! - 所有写操作需 `site.settings.update` 权限。
//!
//! 注意：本模块不依赖 SeaORM 实体，注册表走 JSON 文件，降低耦合；只有激活时
//! 才回写 site_settings（KV 表），便于部署侧按 `home_template` 路由主端口。
//!
//! ## 预览地址解析（previewUrl）—— app 类模板
//!
//! 历史上 app 类固定返回 `http://127.0.0.1:<port>/`，这在生产环境是错的
//! （把服务器本机回环地址泄漏给浏览器）。现在按优先级解析：
//!   1. 环境变量 `TEMPLATES_ORIGIN`（如 `https://home.example.com`）→ `{origin}/`
//!      —— 模板部署在独立域名/端口时的正解；
//!   2. 若 `templates/<slug>/index.html` 存在（构建产物已部署）→ 相对路径 `/t/<slug>`
//!      —— 由后端同源伺服，无跨域、无回环地址；
//!   3. 兜底 `http://127.0.0.1:<port>/` —— 仅本地开发便利（dev server 场景）。
//!
//! ## 静态响应头
//!
//! `/t/*` 现在会带 `Cache-Control`（HTML 为 no-cache，子资源 24h）、`Last-Modified`
//! 并支持 `If-Modified-Since` → 304，另附 `X-Content-Type-Options: nosniff` 与
//! `Referrer-Policy`。
//!
//! ## SPA 兜底边界（looks_like_asset）
//!
//! 读不到文件时不无脑回退 index.html，而是按路径类型分流：
//! - 导航型（无扩展名，如 `/t/coucouya/post/xxx`）→ 回退 index.html，前端路由接管；
//! - 资源型（`assets/*.js`、`*.css`、`favicon.ico` …）→ **404**。
//!
//! 资源型必须 404：若用 HTML 冒充缺失的 JS/CSS，浏览器会拿到
//! `200 + text/html` 并报 `Refused to execute script … MIME type ('text/html')`，
//! 把干净的 404 变成难定位的脚本错误（典型触发：换模板后浏览器缓存着旧哈希资源）。
//! 与 `main.rs::spa_fallback` 的语义一致（那边用 `contains('.')`，这边用扩展名白名单，
//! 以便文章 slug 带点时仍走前端路由）。
//!
//! 未做（仍需基础设施层解决）：上传模板与 API 同源，模板内 JS 仍运行在 API 源上。
//! 彻底方案是把模板托管挪到独立 origin（子域/独立端口）；本轮只加了不破坏
//! 后台 iframe 实时预览的轻量头（未加 X-Frame-Options / CSP frame-ancestors，
//! 因为开发态后台 5188 需要跨源 iframe 8088 的模板页面）。

use std::io::{Cursor, Read};
use std::path::PathBuf;
use std::sync::Mutex;

use axum::extract::{Multipart, Path, State};
use axum::http::{header, HeaderMap, HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use zip::ZipArchive;

use crate::auth::{ensure, Auth};
use crate::error::{ok, ApiError, ApiResult};
use crate::state::AppState;

/// 写操作串行化（admin 低频，简单互斥即可，避免清单并发写丢）
static WRITE_LOCK: Mutex<()> = Mutex::new(());

/// 模板目录：环境变量 TEMPLATES_DIR 优先；否则从可执行文件所在目录逐级向上找
/// 第一个真实存在的 `templates/`（避免依赖 cwd —— 从别处启动会导致模板全 404）；
/// 都找不到才回落到相对路径 `templates`。
pub fn templates_dir() -> PathBuf {
    if let Ok(v) = std::env::var("TEMPLATES_DIR") {
        let v = v.trim();
        if !v.is_empty() {
            return PathBuf::from(v);
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        let mut cur = exe.parent().map(|p| p.to_path_buf());
        for _ in 0..5 {
            let Some(dir) = cur else { break };
            let cand = dir.join("templates");
            if cand.is_dir() {
                return cand;
            }
            cur = dir.parent().map(|p| p.to_path_buf());
        }
    }
    PathBuf::from("templates")
}

fn manifest_path() -> PathBuf {
    templates_dir().join("manifest.json")
}

/// 上一版清单备份（write_manifest 每次覆盖前留存），解析失败时用于回滚
fn manifest_backup_path() -> PathBuf {
    templates_dir().join("manifest.json.bak")
}

// 版式预设清单不再在此处硬编码 —— 统一从 `crate::presets`（单一权威）派生。
// 此前这里、后台 UI、主页前端各存一份，靠人工同步，容易漂移。

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
    /// 该模板支持的版式预设清单（仅 app 类有意义，upload 类留空）。
    ///
    /// 这是**目录性**信息：运行时真正生效的预设由 `site_settings.home_preset`
    /// 决定（见 site.rs），清单本身从 `crate::presets`（单一权威）派生。
    /// 放在清单里是为了列出模板时能一眼看到它有哪些版式可选。
    ///
    /// `#[serde(default)]` 是必需的：早先写入的清单没有这个字段，
    /// 而 write_manifest 会整体重序列化 —— 少了默认值，读旧清单会直接失败，
    /// 等于把模板列表读空。
    #[serde(default)]
    presets: Vec<String>,
}

#[derive(Serialize, Deserialize, Default)]
struct Manifest {
    active: String,
    items: Vec<TplMeta>,
}

/// 启动期确保模板目录存在，并在清单缺失时种入内置模板（coucouya / fastshot）；
/// 随后做一次自检（目录可解析、active 目录存在），异常打日志而不是静默失败。
pub fn ensure_templates_dir() {
    let dir = templates_dir();
    let _ = std::fs::create_dir_all(&dir);
    if !manifest_path().exists() {
        let seed = Manifest {
            active: "coucouya".into(),
            items: vec![
                TplMeta {
                    slug: "coucouya".into(),
                    name: "可可鸭（默认）".into(),
                    kind: "app".into(),
                    port: "5199".into(),
                    note: "默认数据驱动首页；构建产物可部署到 templates/coucouya/ 由后端同源伺服；支持多套版式预设".into(),
                    created_at: crate::db::now_iso(),
                    presets: crate::presets::ids(),
                },
                TplMeta {
                    slug: "fastshot".into(),
                    name: "Fastshot 风格".into(),
                    kind: "app".into(),
                    port: "5197".into(),
                    note: "Fastshot 视觉克隆（数据来自后台）".into(),
                    created_at: crate::db::now_iso(),
                    presets: Vec::new(),
                },
            ],
        };
        let _ = write_manifest(&seed);
    }

    // 启动期自检：把「模板全部 404」这类静默故障提前暴露到启动日志
    match std::fs::read_to_string(manifest_path()) {
        Ok(txt) => match serde_json::from_str::<Manifest>(&txt) {
            Ok(m) => {
                if !m.active.is_empty() && !dir.join(&m.active).is_dir() {
                    eprintln!(
                        "[templates] WARN active='{}' 但目录 {} 不存在，/t/active 将返回 404",
                        m.active,
                        dir.join(&m.active).display()
                    );
                }
                println!(
                    "[templates] dir={} active={} items={}",
                    dir.display(),
                    m.active,
                    m.items.len()
                );
            }
            Err(e) => eprintln!(
                "[templates] ERROR manifest.json 解析失败（{}）：{e}",
                manifest_path().display()
            ),
        },
        Err(e) => eprintln!(
            "[templates] ERROR 无法读取清单 {}：{e}",
            manifest_path().display()
        ),
    }
}

/// 读注册表；主文件读不到或解析失败时，自动回落到上一版备份（避免清单损坏导致模板全丢）
fn read_manifest() -> Manifest {
    if let Ok(txt) = std::fs::read_to_string(manifest_path()) {
        if let Ok(m) = serde_json::from_str::<Manifest>(&txt) {
            return m;
        }
        eprintln!("[templates] manifest.json 解析失败，尝试回落到 .bak");
    }
    if let Ok(txt) = std::fs::read_to_string(manifest_backup_path()) {
        if let Ok(m) = serde_json::from_str::<Manifest>(&txt) {
            eprintln!("[templates] 已从 manifest.json.bak 恢复注册表");
            return m;
        }
    }
    Manifest::default()
}

/// 写注册表：写 .tmp → 备份旧文件为 .bak → 删旧 → 改名。
///
/// 环境限制：打开已存在的 manifest.json 直接写入会 os error 5（拒绝访问），
/// 而「创建新文件 / 删除 / 改名」均允许，故走三步替换。相比旧实现多了一步
/// `.bak` 备份 —— 万一替换过程中断，read_manifest 可以自动回滚。
fn write_manifest(m: &Manifest) -> Result<(), ApiError> {
    let txt =
        serde_json::to_string_pretty(m).map_err(|e| ApiError::bad(format!("清单序列化失败：{e}")))?;
    let live = manifest_path();
    let tmp = live.with_extension("json.tmp");
    std::fs::write(&tmp, txt).map_err(|e| ApiError::bad(format!("写入清单失败：{e}")))?;
    if live.exists() {
        let _ = std::fs::copy(&live, manifest_backup_path());
        std::fs::remove_file(&live)
            .map_err(|e| ApiError::bad(format!("替换清单失败：{e}")))?;
    }
    std::fs::rename(&tmp, &live).map_err(|e| ApiError::bad(format!("替换清单失败：{e}")))?;
    Ok(())
}

/// 该 slug 是否已有构建产物落在模板目录里（由后端同源伺服）
fn is_hosted(slug: &str) -> bool {
    templates_dir().join(slug).join("index.html").is_file()
}

/// 外部应用域（生产用）：TEMPLATES_ORIGIN，形如 https://home.example.com
fn templates_origin() -> Option<String> {
    let v = std::env::var("TEMPLATES_ORIGIN").ok()?;
    let v = v.trim().trim_end_matches('/').to_string();
    if v.is_empty() {
        None
    } else {
        Some(v)
    }
}

/// 把模板条目投影为公开/后台列表用的轻量结构
fn meta_to_json(m: &TplMeta, active: &str) -> serde_json::Value {
    let hosted = is_hosted(&m.slug);
    // 预览地址优先级见模块头注释：环境变量 > 已部署产物同源路径 > 本地端口兜底
    let preview_url = if m.kind == "app" {
        if let Some(origin) = templates_origin() {
            format!("{origin}/")
        } else if hosted {
            format!("/t/{}", m.slug)
        } else if !m.port.is_empty() {
            format!("http://127.0.0.1:{}/", m.port)
        } else {
            format!("/t/{}", m.slug)
        }
    } else {
        // upload 类走后端同域 /t/<slug>（无尾斜杠——axum 通配路由对尾斜杠结尾返回 404）
        format!("/t/{}", m.slug)
    };
    json!({
        "slug": m.slug,
        "name": m.name,
        "kind": m.kind,
        "port": m.port,
        "note": m.note,
        "createdAt": m.created_at,
        "active": m.slug == active,
        // 是否已有构建产物由后端同源伺服（app 类部署到 templates/<slug>/ 后为 true）
        "hosted": hosted,
        "previewUrl": preview_url,
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

/// 校验模板元数据（长度 / 端口范围），避免脏数据进注册表
fn check_meta(name: &str, note: &str, port: &str) -> Result<(), ApiError> {
    if name.chars().count() > 60 {
        return Err(ApiError::bad("name 过长（上限 60 字符）"));
    }
    if note.chars().count() > 200 {
        return Err(ApiError::bad("note 过长（上限 200 字符）"));
    }
    let p = port.trim();
    if !p.is_empty() {
        let n: u32 = p
            .parse()
            .map_err(|_| ApiError::bad("port 必须是数字（1-65535）"))?;
        if n == 0 || n > 65535 {
            return Err(ApiError::bad("port 超出 1-65535 范围"));
        }
    }
    Ok(())
}

/// 写单个 site_settings KV（激活/删除后同步 home_template）
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
    let Some(idx) = m.items.iter().position(|x| x.slug == slug) else {
        return Err(ApiError::not_found("模板不存在"));
    };
    {
        let item = &mut m.items[idx];
        if let Some(v) = body.get("name").and_then(|x| x.as_str()) {
            item.name = v.to_string();
        }
        if let Some(v) = body.get("port").and_then(|x| x.as_str()) {
            item.port = v.to_string();
        }
        if let Some(v) = body.get("note").and_then(|x| x.as_str()) {
            item.note = v.to_string();
        }
    }
    let item = m.items[idx].clone();
    check_meta(&item.name, &item.note, &item.port)?;
    write_manifest(&m)?;
    ok(json!({ "ok": true }))
}

/// DELETE /api/admin/templates/:slug —— 仅允许删除上传类（upload）模板
pub async fn remove(
    State(st): State<AppState>,
    auth: Auth,
    Path(slug): Path<String>,
) -> ApiResult {
    ensure(&auth, "site.settings.update")?;
    let new_active = {
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
        m.active
    };
    // 关键修复：删除可能改变 active，必须同步 site_settings，否则
    // manifest.active（/t/active 的真相源）与 site_settings.home_template
    // （主页跳转的真相源）会长期漂移。
    set_site_kv(&st, "home_template", &new_active).await?;
    ok(json!({ "ok": true, "active": new_active }))
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
    check_meta(&name, &note, &port)?;

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
        // 上传型模板是独立 HTML 包，没有"版式预设"这一层
        presets: Vec::new(),
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

/// 模板 slug 白名单：仅允许小写字母、数字、连字符、下划线，且必须以字母或数字开头。
///
/// F5 修复：此前只校验了 `rest`（子路径）的 `starts_with`，slug 本身未做任何校验。
/// `templates_dir().join("../../etc")` 能顺利通过 `base.exists()` 并读到模板目录
/// 之外的任意文件 —— 同一份代码里 spa_fallback 做了 `contains("..")` 而 /t/ 没有，
/// 属于防护遗漏而非有意设计。
fn slug_ok(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 64
        && slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
        && slug.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
}

/// 请求路径是否「看起来是静态资源」（末段带已知扩展名）。
///
/// 用于 SPA 兜底的边界判定：命中不到文件时，
/// - **导航型**路径（无扩展名，如 `/t/coucouya/post/my-article`）→ 回退 `index.html`，
///   交给前端路由接管；
/// - **资源型**路径（`assets/*.js`、`*.css`、`favicon.ico` …）→ **必须 404**。
///
/// 后者不是洁癖而是实际故障：若用 HTML 冒充缺失的 JS/CSS，浏览器拿到的是
/// `200 + Content-Type: text/html`，控制台报
/// `Refused to execute script … MIME type ('text/html') is not executable`，
/// 把一次干净的 404 变成难以定位的脚本错误。典型触发场景是**换模板后浏览器
/// 仍缓存着旧哈希资源**（`index-<oldhash>.js` 已不在磁盘上）。
///
/// 用「扩展名白名单」而非 `contains('.')`：文章 slug 允许带点（如 `ai-2.0`），
/// 那类路径应当走前端路由，不能被误判成资源。
///
/// `pub(crate)`：`main.rs::spa_fallback`（后台 SPA 的 dist-app 兜底）复用同一判定，
/// 两处语义保持一致 —— 后台唯一带参数的公开路由是 `/author/$name`，作者名带点时
/// 同样需要回退到 index.html 而不是 404。
pub(crate) fn looks_like_asset(path: &str) -> bool {
    let last = path.rsplit('/').next().unwrap_or("");
    let Some(ext) = last.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()) else {
        return false;
    };
    matches!(
        ext.as_str(),
        "js" | "mjs"
            | "css"
            | "map"
            | "json"
            | "xml"
            | "txt"
            | "png"
            | "jpg"
            | "jpeg"
            | "gif"
            | "webp"
            | "avif"
            | "bmp"
            | "svg"
            | "ico"
            | "woff"
            | "woff2"
            | "ttf"
            | "otf"
            | "eot"
            | "mp4"
            | "webm"
            | "mp3"
            | "wasm"
            | "pdf"
    )
}

/// 子路径安全校验：拒绝空段、`.`、`..`（同时覆盖 Windows 的 `\` 分隔与
/// `%5c` 解码结果）。
///
/// **为什么不能只靠 `Path::starts_with`**：它是**按分量前缀**比较，而 `..`
/// 在 Rust 里就是一个普通分量、不参与归一化 —— `base.join("../x")` 得到
/// `<base>/../x`，其分量序列仍以 `<base>` 为前缀，`starts_with(&base)`
/// 照样返回 `true`。结果是 `GET /t/coucouya/../manifest.json` 能读到模板目录
/// 之外的文件（2026-09-22 实测：可读到 `templates/manifest.json` 与
/// `server/Cargo.toml`，且 `%2e%2e%2f` 编码变体同样有效 —— 公开路由、无需认证）。
///
/// 对比 `main.rs::spa_fallback` 之所以没这个问题：它用的是 `uri.path()`（**未解码**）
/// 且只做 `contains("..")`，`%2e%2e` 不等于 `..`，join 后只是普通文件名；
/// 而这边经由 axum `Path` 提取器，**已经百分号解码**，必须显式按分量判。
fn rel_is_safe(rel: &str) -> bool {
    if rel.is_empty() {
        // 入口请求：调用方直取 index.html
        return true;
    }
    rel.split(|c| c == '/' || c == '\\')
        .all(|seg| !seg.is_empty() && seg != "." && seg != "..")
}

/// 读取模板目录下某个相对路径的文件（含 index.html 兜底 + 缓存/条件请求头）。
///
/// `st` 用于「服务期头注」：伺服**模板入口 index.html** 时，由
/// `agent_layer::head_for` 从 DB 派生 title/description/canonical/OG/JSON-LD
/// 与 `<noscript>` 正文兜底，注入后再返回。原因见 `agent_layer` 模块头注释
/// （CSR 产物对机器人是空文档）。`None` 表示不注入（测试或非 DB 场景）。
///
/// **注入后的响应不发 `Last-Modified`、不做 304**：内容来自 DB 而非文件，
/// 文件 mtime 无法代表内容是否变化 —— 若沿用文件时间戳做条件请求，
/// 「后台改了标题但客户端拿到 304」会变成无法排查的陈旧内容。
async fn serve_file(slug: &str, rest: &str, req: &HeaderMap, st: Option<&AppState>) -> Response {
    if !slug_ok(slug) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let base = templates_dir().join(slug);
    if !base.exists() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let idx_path = base.join("index.html");
    // 防穿越（两道）：
    //   ① 按分量拒绝 `.` / `..` / 空段 —— 这是真正拦住 `..` 的一道，
    //      因为 `starts_with` 对 `..` 无效（见 rel_is_safe 注释）；
    //   ② `starts_with` 前缀兜底 —— 拦绝对路径注入等（如 `%2f` 解出的前导 `/`）。
    let rel = rest.trim_matches('/');
    if !rel_is_safe(rel) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let target = if rel.is_empty() {
        idx_path.clone()
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

    // 命中文件；读不到时按路径类型分流（见 looks_like_asset 注释）：
    // 导航型 → SPA 深链兜底回退 index.html；资源型 → 404，绝不用 HTML 冒充 JS/CSS。
    let (hit, bytes) = match tokio::fs::read(&target).await {
        Ok(b) => (target, b),
        Err(_) => {
            if looks_like_asset(rel) {
                return StatusCode::NOT_FOUND.into_response();
            }
            match tokio::fs::read(&idx_path).await {
                Ok(b) => (idx_path.clone(), b),
                Err(_) => return StatusCode::NOT_FOUND.into_response(),
            }
        }
    };

    let is_html = hit
        .extension()
        .map(|e| e.eq_ignore_ascii_case("html") || e.eq_ignore_ascii_case("htm"))
        .unwrap_or(true);

    // ── 服务期头注：只对模板入口 index.html 生效（子页面 HTML 不碰）──
    let mut injected = false;
    let bytes = if is_html && hit == idx_path {
        match st {
            Some(st) => match crate::agent_layer::head_for(st, slug, rel).await {
                Some(head) => {
                    let text = String::from_utf8_lossy(&bytes);
                    injected = true;
                    crate::agent_layer::inject_head(&text, &head).into_bytes()
                }
                None => bytes,
            },
            None => bytes,
        }
    } else {
        bytes
    };

    let last_modified = if injected {
        None
    } else {
        tokio::fs::metadata(&hit)
            .await
            .ok()
            .and_then(|m| m.modified().ok())
            .map(|t| {
                let dt: DateTime<Utc> = t.into();
                dt.format("%a, %d %b %Y %H:%M:%S GMT").to_string()
            })
    };

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(ct_of(&hit.to_string_lossy())),
    );
    // HTML 不缓存（换模板/改内容即时生效）；子资源（通常带内容哈希）可长缓存
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(if is_html {
            "no-cache"
        } else {
            "public, max-age=86400"
        }),
    );
    headers.insert(
        HeaderName::from_static("x-content-type-options"),
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        HeaderName::from_static("referrer-policy"),
        HeaderValue::from_static("strict-origin-when-cross-origin"),
    );
    if let Some(lm) = last_modified.as_ref() {
        if let Ok(v) = HeaderValue::from_str(lm) {
            headers.insert(header::LAST_MODIFIED, v);
        }
    }

    // 条件请求：If-Modified-Since 与 Last-Modified 字面相等 → 304（无需重新解析时间戳）
    if let (Some(lm), Some(ims)) = (
        last_modified.as_ref(),
        req.get(header::IF_MODIFIED_SINCE).and_then(|v| v.to_str().ok()),
    ) {
        if ims == lm {
            let mut h304 = HeaderMap::new();
            if let Ok(v) = HeaderValue::from_str(lm) {
                h304.insert(header::LAST_MODIFIED, v);
            }
            h304.insert(
                header::CACHE_CONTROL,
                HeaderValue::from_static(if is_html {
                    "no-cache"
                } else {
                    "public, max-age=86400"
                }),
            );
            return (StatusCode::NOT_MODIFIED, h304).into_response();
        }
    }

    (headers, bytes).into_response()
}

/// GET /t/:slug —— 提供模板入口 index.html（无尾斜杠）
///
/// 2026-09-22 起带 `State`：入口 HTML 与深链兜底都会先过一遍**服务期头注**
/// （见 `serve_file` 与 `agent_layer`），让 2 KB 的 CSR 空壳对机器人也有
/// title / description / canonical / OG / JSON-LD / noscript 正文。
pub async fn serve_index(
    State(st): State<AppState>,
    headers: HeaderMap,
    Path(slug): Path<String>,
) -> Response {
    if slug == "manifest.json" {
        return StatusCode::NOT_FOUND.into_response();
    }
    serve_file(&slug, "", &headers, Some(&st)).await
}

/// GET /t/:slug/*rest —— 按 slug 提供模板静态文件
pub async fn serve_one(
    State(st): State<AppState>,
    headers: HeaderMap,
    Path((slug, rest)): Path<(String, String)>,
) -> Response {
    // 拒绝清单文件本身被直接访问
    if slug == "manifest.json" {
        return StatusCode::NOT_FOUND.into_response();
    }
    serve_file(&slug, &rest, &headers, Some(&st)).await
}

/// 解析「当前激活模板 slug」。
///
/// 单一真相源是 `site_settings.home_template`（后台「站点外观」写入；主页跳转、
/// 模板角标、部署脚本读的都是它）。activate()/remove() 已双写 manifest 与 DB，
/// 但历史上有过「只写一侧」的记录，本次把**读路径也收敛到 DB**，让两侧彻底配对。
///
/// 回退策略：DB 读失败 / 值为空 / 该 slug 不在注册表中 → 回落 `manifest.active`，
/// 避免 DB 异常导致 `/t/active` 直接 404（宁可服务旧模板也不要白屏）。
///
/// `pub(crate)`：`agent_layer` 的机器端点（/llms.txt、/agent.json）需要它来拼
/// 站点根地址，避免两处各写一遍「读 DB → 回落 manifest」的逻辑而漂移。
pub(crate) async fn resolve_active(st: &AppState) -> String {
    let m = read_manifest();
    let db_val = st
        .db
        .query_one(
            "SELECT value FROM site_settings WHERE key = 'home_template' LIMIT 1",
            vec![],
        )
        .await
        .ok()
        .flatten()
        .and_then(|r| r.try_get::<String>("", "value").ok())
        .map(|v| v.trim().to_string())
        .unwrap_or_default();
    if !db_val.is_empty() {
        if m.items.iter().any(|x| x.slug == db_val) {
            return db_val;
        }
        eprintln!(
            "[templates] WARN site_settings.home_template='{db_val}' 不在注册表中，回落 manifest.active='{}'",
            m.active
        );
    }
    m.active
}

/// GET /t/active —— 302 跳到规范地址 /t/<active>
pub async fn serve_active_index(State(st): State<AppState>) -> Response {
    redirect_active(&st, "").await
}

/// GET /t/active/*rest —— 302 跳到 /t/<active>/<rest>
pub async fn serve_active(State(st): State<AppState>, Path(rest): Path<String>) -> Response {
    redirect_active(&st, &rest).await
}

/// 302 到 `/t/<slug>[/rest]`。
///
/// 为什么是重定向而不是直接伺服：模板产物是按 `base=/t/<slug>/` 构建的
/// （`<script src="/t/coucouya/assets/…">`），前端路由从 `import.meta.url`
/// 反推 base 来解析深链。若从别名路径 `/t/active` 直接吐内容，页面的 base
/// 是 `/t/coucouya` 而 pathname 是 `/t/active`，两者对不上 —— 深链会被判成
/// 404（实测踩到过）。重定向让页面只在自己的规范路径下渲染，顺带消除
/// `/t/active` 与 `/t/<slug>` 的重复内容（SEO）。
///
/// 用 302 而非 301：激活模板是可变的，不能被浏览器永久缓存。
async fn redirect_active(st: &AppState, rest: &str) -> Response {
    let slug = resolve_active(st).await;
    if slug.is_empty() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let rest = rest.trim_start_matches('/');
    // 不在 Location 里反射 `..`：浏览器会把 `/t/coucouya/../../x` 归一化成 `/x`，
    // 虽仍是同源、不构成开放重定向，但让别名入口只接受规整路径更不易被误用。
    if !rel_is_safe(rest.trim_matches('/')) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let loc = if rest.is_empty() {
        format!("/t/{slug}")
    } else {
        format!("/t/{slug}/{rest}")
    };
    match HeaderValue::from_str(&loc) {
        Ok(v) => (StatusCode::FOUND, [(header::LOCATION, v)]).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::{looks_like_asset, rel_is_safe};

    /// 子路径校验：必须拦住 2026-09-22 实测可复现的穿越载荷
    #[test]
    fn rel_rejects_traversal() {
        for bad in [
            "../manifest.json",
            "../../Cargo.toml",
            "a/../../b",
            "./a",
            "a/./b",
            "a//b",
            "..\\manifest.json",
            "a\\..\\b",
        ] {
            assert!(!rel_is_safe(bad), "{bad} 应被拒绝");
        }
        for good in ["", "assets/index-abc.js", "post/ai-2.0", "a/b/c-d_e.png"] {
            assert!(rel_is_safe(good), "{good} 应被放行");
        }
    }

    /// SPA 兜底边界：资源型缺失必须 404，导航型（含带点 slug）必须放行走前端路由
    #[test]
    fn asset_boundary() {
        for a in [
            "assets/index-abc123.js",
            "assets/index-abc.css",
            "favicon.ico",
            "img/a.webp",
            "fonts/x.woff2",
            "a/b/c.map",
        ] {
            assert!(looks_like_asset(a), "{a} 应判为资源");
        }
        for n in [
            "",
            "post/u-card-comparison",
            "post/ai-2.0",
            "post/hello.world.tar",
            "post/2024-09-21",
        ] {
            assert!(!looks_like_asset(n), "{n} 应判为导航路径");
        }
    }
}
