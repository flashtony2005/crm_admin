//! CMS 服务入口：组装路由（集成测试复用 build_router）。

use axum::{
    extract::DefaultBodyLimit,
    routing::{get, post, put},
    Router,
};
use serde_json::json;
use tower_http::cors::CorsLayer;

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
mod state;
mod team;

#[cfg(test)]
mod tests_api;

use error::{ok, ApiResult};
use state::AppState;

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
        // SEO 公开端点（免认证）：sitemap / RSS / robots
        .route("/sitemap.xml", get(seo::sitemap))
        .route("/rss.xml", get(seo::rss))
        .route("/robots.txt", get(seo::robots))
        // 提升 JSON 请求体上限：默认 2MB，文章正文内联 base64 图片易超限，
        // 放宽到 20MB（仍可被 Nginx/反代层再做最终限制）。
        .layer(DefaultBodyLimit::max(20 * 1024 * 1024))
        .layer(CorsLayer::very_permissive())
        .with_state(st)
}

#[tokio::main]
async fn main() {
    let db = db::connect().await;
    db::bootstrap(&db).await;
    // A2 安全治理：生产模式禁止默认 JWT 密钥（违规直接 panic）
    auth::assert_jwt_secret();
    let st = AppState { db, tenant: "t_demo".into() };
    // B3 定时触发器：后台每分钟扫描 schedule.* 工作流
    scheduler::spawn(st.clone());

    let app = build_router(st.clone());
    let addr = format!(
        "0.0.0.0:{}",
        std::env::var("PORT").unwrap_or_else(|_| "8088".into())
    );
    println!("cms-server listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind");
    axum::serve(listener, app).await.expect("serve");
}
