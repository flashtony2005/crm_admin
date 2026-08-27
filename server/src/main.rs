//! CMS 服务入口：组装路由（集成测试复用 build_router）。

use axum::{
    extract::{DefaultBodyLimit, Request},
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
    Router,
};
use serde_json::json;
use std::path::PathBuf;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

mod ai;
mod cmsdb;
mod auth;
mod automation;
mod db;
mod error;
mod notify;
mod oauth;
mod perm;
mod public_api;
mod public_forms;
mod resources;
mod scheduler;
mod seo;
mod site;
mod stats;
mod state;
mod members;
mod comments;
mod newsletter;
mod subscriptions;
mod webhooks_out;
mod graphql_api;
mod i18n;
mod team;
mod upload;

#[cfg(test)]
mod tests_api;

use error::{ok, ApiResult};
use state::AppState;

/// 生产态静态资源（web/dist 经 deploy 复制到 ./dist-app）的 SPA history 回退。
/// 仅对「无扩展名」或文件不存在的导航请求返回 index.html；带扩展名的缺失文件返回 404。
fn static_dir() -> PathBuf {
    std::env::var("STATIC_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("./dist-app"))
}

fn content_type_for(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("").to_ascii_lowercase().as_str() {
        "js" | "mjs" => "application/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "html" => "text/html; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff" | "woff2" => "font/woff2",
        "txt" => "text/plain; charset=utf-8",
        "xml" | "map" => "application/json",
        _ => "application/octet-stream",
    }
}

async fn spa_fallback(uri: Uri) -> Response {
    let path = uri.path();
    // 防目录穿越
    if path.contains("..") {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let dir = static_dir();
    // 带扩展名且文件真实存在 → 直接返回静态文件（/assets/*、favicon.ico 等）
    if path.contains('.') {
        let rel = path.trim_start_matches('/');
        let file = dir.join(rel);
        if let Ok(bytes) = tokio::fs::read(&file).await {
            return ([(header::CONTENT_TYPE, content_type_for(path))], bytes).into_response();
        }
        return StatusCode::NOT_FOUND.into_response();
    }
    // 导航类请求 → 返回 SPA 入口 index.html（client-side routing 兜底）
    match tokio::fs::read_to_string(dir.join("index.html")).await {
        Ok(html) => (
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            html,
        )
            .into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn healthz() -> ApiResult {
    ok(json!({ "service": "cms-server", "phase": 1, "status": "up" }))
}

/// 组装路由（集成测试复用同一构造）
pub fn build_router(st: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/api/auth/login", post(auth::login))
        .route("/api/user/me", get(auth::me))
        .route("/api/me/password", post(auth::change_password))
        .route("/api/me/profile", post(auth::update_profile))
        .route("/api/{table}", get(resources::list).post(resources::create))
        .route(
            "/api/{table}/{id}",
            get(resources::get_one).put(resources::update).delete(resources::remove),
        )
        .route("/api/approvals/{id}/decide", post(resources::decide))
        .route("/api/team/users", get(team::list).post(team::invite))
        .route("/api/team/users/{id}", put(team::update_member))
        .route("/api/ai/invoke", post(ai::invoke))
        .route("/api/ai/audit", get(ai::audit_list))
        .route("/api/automation/trigger", post(automation::trigger))
        .route("/api/plugins", get(automation::plugins))
        .route("/api/public/forms/{id}", get(public_forms::get_public))
        .route("/api/public/forms/{id}/submit", post(public_forms::submit))
        .route("/api/integrations/{id}/oauth/start", get(oauth::start))
        .route("/api/integrations/{id}/oauth/status", get(oauth::status))
        .route("/api/integrations/oauth/callback", get(oauth::callback))
        // 公开内容 API（免认证只读）：已发布文章列表与详情，
        // 供公开站点前端 / Jamstack / 第三方消费（headless 用法）
        .route("/api/public/articles", get(public_api::articles))
        .route("/api/public/articles/{id}", get(public_api::article_detail))
        .route("/api/public/tags", get(public_api::tags))
        // 站点级设置（主题/模板/品牌）：免认证只读，供公开站点套用
        .route("/api/public/site", get(site::site))
        .route("/api/public/track", post(stats::track))
        // ── P4 商业层：会员 / 评论 / 邮件 / 订阅 / 出站 Webhook / i18n ──
        // 会员（公开）
        .route("/api/public/members/register", post(members::register))
        .route("/api/public/members/login", post(members::login))
        .route("/api/public/members/me", get(members::me).post(members::update_me))
        .route("/api/public/members/plans", get(members::plans))
        // 评论（公开列表/发布 + Admin 审核）
        .route("/api/public/comments", get(comments::public_list).post(comments::public_create))
        .route("/api/comments", get(comments::admin_list))
        .route("/api/comments/{id}/status", post(comments::moderate))
        // 邮件订阅（公开订阅/退订 + Admin 群发）
        .route("/api/public/newsletter/subscribe", post(newsletter::subscribe))
        .route("/api/public/newsletter/unsubscribe", get(newsletter::unsubscribe))
        .route("/api/newsletter/subscribers", get(newsletter::admin_list))
        .route("/api/newsletter/send", post(newsletter::send))
        // 付费订阅（Stripe）
        .route("/api/public/tiers", get(subscriptions::tiers))
        .route("/api/public/checkout", post(subscriptions::checkout))
        .route("/api/stripe/webhook", post(subscriptions::stripe_webhook))
        // 出站 Webhook
        .route("/api/webhooks", get(webhooks_out::list).post(webhooks_out::create))
        .route("/api/webhooks/{id}", delete(webhooks_out::remove))
        .route("/api/webhooks/test", post(webhooks_out::test))
        // 多语言
        .route("/api/public/i18n/{locale}", get(i18n::messages))
        .route("/api/public/locales", get(i18n::locales))
        // 站点设置管理（Owner）：主题 / 模板 / 品牌
        .route("/api/admin/site", put(site::update_site))
        .route("/api/admin/stats", get(stats::stats))
        // GraphQL（只读）
        .route("/graphql", get(graphql_api::graphql_handler).post(graphql_api::graphql_handler))
        .route("/graphiql", get(graphql_api::graphiql))
        // SEO 公开端点（免认证）：sitemap / RSS / robots
        .route("/sitemap.xml", get(seo::sitemap))
        .route("/rss.xml", get(seo::rss))
        .route("/robots.txt", get(seo::robots))
        // 文件上传（需 content.media.upload 权限）：multipart → uploads/ 目录
        .route("/api/upload", post(upload::upload))
        // 静态资源：上传的文件公开可读（/uploads/*）
        .nest_service("/uploads", ServeDir::new(upload::uploads_dir()))
        // 提升 JSON 请求体上限：默认 2MB，文章正文内联 base64 图片易超限，
        // 放宽到 20MB（仍可被 Nginx/反代层再做最终限制）。
        // 生产态内置静态服务：未匹配到 /api、/uploads、SEO 等路由时，
        // 由 spa_fallback 提供 web/dist 静态文件，缺失的导航路径回退到 index.html。
        .fallback(get(spa_fallback))
        .layer(DefaultBodyLimit::max(20 * 1024 * 1024))
        .layer(CorsLayer::very_permissive())
        .with_state(st)
}

/// 最小化 .env 加载：仅把「当前进程环境中尚不存在」的变量注入（不覆盖容器/系统注入）。
/// 不引入外部依赖；优先级：系统/容器注入 > .env 文件。
fn load_dotenv() {
    let path = std::env::var("DOTENV_PATH").unwrap_or_else(|_| ".env".into());
    let Ok(text) = std::fs::read_to_string(&path) else {
        return;
    };
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line).trim();
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let k = k.trim();
        let v = v.trim().trim_matches('"').trim_matches('\'');
        if k.is_empty() {
            continue;
        }
        if std::env::var(k).is_err() {
            std::env::set_var(k, v);
        }
    }
}

#[tokio::main]
async fn main() {
    // 加载 .env（存在才加载；容器/系统注入的环境变量不会被覆盖）
    load_dotenv();
    let db = db::connect().await;
    db::bootstrap(&db).await;
    // A2 安全治理：生产模式禁止默认 JWT 密钥（违规直接 panic）
    auth::assert_jwt_secret();
    let st = AppState { db, tenant: "t_demo".into() };
    // B3 定时触发器：后台每分钟扫描 schedule.* 工作流
    scheduler::spawn(st.clone());
    // 确保上传目录存在（幂等）
    upload::ensure_uploads_dir();

    let app = build_router(st.clone());
    let addr = format!(
        "0.0.0.0:{}",
        std::env::var("PORT").unwrap_or_else(|_| "8088".into())
    );
    println!("cms-server listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind");
    axum::serve(listener, app).await.expect("serve");
}
