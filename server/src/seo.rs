//! SEO 公开端点：/sitemap.xml、/rss.xml、/robots.txt（免认证，供搜索引擎/订阅器抓取）。
//!
//! 说明：
//! - 仅输出 status='published' 的文章；
//! - 公开阅读 URL 采用 `{base}/read/{id}` 占位（未来公开站点路由），
//!   可通过环境变量 PUBLIC_BASE_URL 配置站点根地址；
//! - 纯文本 XML 响应，不依赖 ApiResult JSON 包装。
//!
//! 部署：本文件为新增模块，需在本地 `cargo build` 后随新二进制生效。

use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use sea_orm::Value as SqlValue;

use crate::state::AppState;

/// 站点根地址：环境变量 PUBLIC_BASE_URL（如 https://blog.example.com），缺省本地
fn base_url() -> String {
    std::env::var("PUBLIC_BASE_URL").unwrap_or_else(|_| "http://localhost:8088".into())
}

/// XML 转义
fn esc_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// 已发布文章：(id, title, summary, updated_at, featured_image)
async fn published_articles(
    st: &AppState,
) -> Result<Vec<(String, String, String, String, String)>, String> {
    let sql = "SELECT id, title, summary, updated_at, featured_image \
               FROM articles WHERE tenant_id = ? AND status = 'published' \
               ORDER BY updated_at DESC LIMIT 200";
    let rows = st
        .db
        .query_all(sql, vec![SqlValue::String(Some(st.tenant.clone()))])
        .await
        .map_err(|e| format!("查询文章失败：{e}"))?;
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let (Some(id), Some(title)) = (
            r.try_get::<String>("", "id").ok(),
            r.try_get::<String>("", "title").ok(),
        ) else {
            continue;
        };
        let summary = r.try_get::<String>("", "summary").unwrap_or_default();
        let updated = r.try_get::<String>("", "updated_at").unwrap_or_default();
        let img = r
            .try_get::<Option<String>>("", "featured_image")
            .ok()
            .flatten()
            .unwrap_or_default();
        out.push((id, title, summary, updated, img));
    }
    Ok(out)
}

/// GET /sitemap.xml
pub async fn sitemap(State(st): State<AppState>) -> impl IntoResponse {
    let base = base_url();
    let urls = match published_articles(&st).await {
        Ok(list) => list,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                [(header::CONTENT_TYPE, "application/xml; charset=utf-8")],
                format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><error>{}</error>", esc_xml(&e)),
            )
        }
    };
    let mut body = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n",
    );
    body.push_str(&format!(
        "  <url><loc>{}/</loc><changefreq>daily</changefreq><priority>1.0</priority></url>\n",
        esc_xml(&base)
    ));
    for (id, _title, _summary, updated, _img) in urls {
        body.push_str(&format!(
            "  <url><loc>{}/read/{}</loc><lastmod>{}</lastmod><changefreq>weekly</changefreq><priority>0.8</priority></url>\n",
            esc_xml(&base),
            esc_xml(&id),
            esc_xml(&updated)
        ));
    }
    body.push_str("</urlset>\n");
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/xml; charset=utf-8")],
        body,
    )
}

/// GET /rss.xml（RSS 2.0）
pub async fn rss(State(st): State<AppState>) -> impl IntoResponse {
    let base = base_url();
    let items = match published_articles(&st).await {
        Ok(list) => list,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                [(header::CONTENT_TYPE, "application/rss+xml; charset=utf-8")],
                format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><error>{}</error>", esc_xml(&e)),
            )
        }
    };
    let mut body = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<rss version=\"2.0\" xmlns:atom=\"http://www.w3.org/2005/Atom\">\n<channel>\n",
    );
    body.push_str(&format!(
        "  <title>{}</title>\n  <link>{}</link>\n  <description>{}</description>\n  <atom:link href=\"{}/rss.xml\" rel=\"self\" type=\"application/rss+xml\" />\n",
        esc_xml("CMS 博客"),
        esc_xml(&base),
        esc_xml("基于 Rust 的开源 CMS 内容发布"),
        esc_xml(&base)
    ));
    for (id, title, summary, updated, _img) in items {
        let pub_date = DateTime::parse_from_rfc3339(&updated)
            .map(|d| d.to_rfc2822())
            .unwrap_or_else(|_| updated.clone());
        body.push_str("  <item>\n");
        body.push_str(&format!("    <title>{}</title>\n", esc_xml(&title)));
        body.push_str(&format!("    <link>{}/read/{}</link>\n", esc_xml(&base), esc_xml(&id)));
        body.push_str(&format!("    <guid isPermaLink=\"false\">{}</guid>\n", esc_xml(&id)));
        body.push_str(&format!("    <pubDate>{}</pubDate>\n", esc_xml(&pub_date)));
        if !summary.is_empty() {
            body.push_str(&format!("    <description>{}</description>\n", esc_xml(&summary)));
        }
        body.push_str("  </item>\n");
    }
    body.push_str("</channel>\n</rss>\n");
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/rss+xml; charset=utf-8")],
        body,
    )
}

/// GET /robots.txt
pub async fn robots() -> impl IntoResponse {
    let base = base_url();
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        format!(
            "User-agent: *\nAllow: /\n\nSitemap: {}/sitemap.xml\n",
            esc_xml(&base)
        ),
    )
}
