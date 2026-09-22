//! SEO 公开端点：/sitemap.xml、/rss.xml、/robots.txt（免认证，供搜索引擎/订阅器抓取）。
//!
//! 说明：
//! - 仅输出 status='published' 的文章；
//! - **付费边界（2026-09-22 定的策略，见 `published_articles` 注释）**：
//!   sitemap **收录**付费/会员内容（只暴露 URL，保持可发现性与可索引性），
//!   RSS **排除**付费/会员内容（RSS 是免费分发通道，订阅器无法完成付费，
//!   把付费项塞进去等于对外承诺一份拿不到的内容）；
//! - 公开 URL 基准见 `home_base()`：`PUBLIC_HOME_URL` 优先，未配置时自动取
//!   后端上的规范主页 `/t/<激活模板slug>`（而非过去那个会 404 的 `{base}`）；
//!   文章链接见 `article_url()` = `{home}/post/{key}`，`PUBLIC_ARTICLE_URL`
//!   可整体覆盖；
//! - robots.txt 屏蔽 `/api/`、`/graphql`、`/graphiql`，并**显式声明**欢迎
//!   AI 检索型爬虫（`agent_layer::AI_CRAWLERS`）；
//! - 纯文本 XML 响应，不依赖 ApiResult JSON 包装。
//!
//! 部署：本文件为新增模块，需在本地 `cargo build` 后随新二进制生效。

use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use sea_orm::Value as SqlValue;

use crate::state::AppState;
use chrono::DateTime;

/// 站点根地址：环境变量 PUBLIC_BASE_URL（如 https://blog.example.com），缺省本地
pub(crate) fn base_url() -> String {
    std::env::var("PUBLIC_BASE_URL").unwrap_or_else(|_| "http://localhost:8088".into())
}

/// 主页对外根地址（可选）：形如 `https://home.example.com`（自动去尾斜杠）。
///
/// 设置后：sitemap 首条 loc 用它、文章详情链接默认挂在它下面 —— 让**主页**
/// （而非后台内置的 /read 阅读页）成为搜索引擎与订阅器看到的门面。
/// 未设置时保持旧行为，完全向后兼容。
pub(crate) fn public_home_url() -> Option<String> {
    let v = std::env::var("PUBLIC_HOME_URL").ok()?;
    let v = v.trim().trim_end_matches('/').to_string();
    if v.is_empty() {
        None
    } else {
        Some(v)
    }
}

/// 站点对外主页的基准地址（**无尾斜杠**）。
///
/// 优先级：`PUBLIC_HOME_URL` > 后端上的规范主页路径 `/t/<激活模板slug>`。
///
/// 2026-09-22 修正：原兜底是 `{base}`（即后端根路径），但后端根路径是**后台
/// SPA**（`spa_fallback` 伺服 `dist-app`），不是对外门面。实测后果很严重 ——
/// sitemap 发布的 7 条 URL 里 6 条是 404（`/read/<key>` 亦然，见下），等于让
/// 搜索引擎和 Agent 拿到一份死链清单。现在改为指向真正伺服主页的 `/t/<slug>`。
async fn home_base(st: &AppState, base: &str) -> String {
    if let Some(h) = public_home_url() {
        return h;
    }
    let slug = crate::templates::resolve_active(st).await;
    let b = base.trim_end_matches('/');
    if slug.is_empty() {
        b.to_string()
    } else {
        format!("{b}/t/{slug}")
    }
}

/// 文章详情公开 URL：`PUBLIC_ARTICLE_URL` 模板优先，否则 `{home}/post/{key}`。
///
/// 与主页前端路由（`coucouya/src/router.ts` 的 `/post/<key>`）及
/// `PUBLIC_HOME_URL` 的约定一致。**不再兜底 `{base}/read/{key}`** —— 见
/// `home_base` 的实测说明；`read/` 仍作为**入站**别名被 `agent_layer` 识别。
pub(crate) fn article_url(home: &str, key: &str) -> String {
    if let Ok(t) = std::env::var("PUBLIC_ARTICLE_URL") {
        let t = t.trim();
        if !t.is_empty() {
            return t.replace("{key}", key);
        }
    }
    format!("{}/post/{}", home.trim_end_matches('/'), key)
}

/// XML 转义
pub(crate) fn esc_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// 已发布文章：(id, title, summary, updated_at, featured_image, slug, paid)
///
/// **付费边界**：`only_free = true` 时只返回完全公开的内容（`visibility='public'`
/// 且 `paid_level=0`）。为什么两个端点用不同口径：
///
/// - sitemap 只列 URL，不含正文/摘要 —— 让付费内容**可被发现、可被索引**，
///   付费墙由 `/api/public/articles/{key}` 的 402 + preview 在页面层守住；
///   若把付费项排除出 sitemap，等于让这些内容对搜索与 Agent 完全隐形，
///   直接放弃转化入口。
/// - RSS 的 `<description>` 是**可自由转发的摘要**，而订阅器天然无法完成付费。
///   把付费项写进 RSS，等于对外发一份永远拿不到全文的承诺，既是体验问题，
///   也让"摘要是否算免费内容"的边界变得含糊 —— 所以一律排除。
///
/// 口径与 `agent_layer::is_gated`、`public_api::article_detail` 保持一致。
async fn published_articles(
    st: &AppState,
    only_free: bool,
) -> Result<Vec<(String, String, String, String, String, String, bool)>, String> {
    let mut sql = String::from(
        "SELECT id, title, summary, updated_at, featured_image, slug, \
                visibility, paid_level \
         FROM articles WHERE tenant_id = ? AND status = 'published'",
    );
    if only_free {
        sql.push_str(" AND COALESCE(visibility, 'public') = 'public' AND COALESCE(paid_level, 0) = 0");
    }
    sql.push_str(" ORDER BY updated_at DESC LIMIT 200");
    let rows = st
        .db
        .query_all(sql.as_str(), vec![SqlValue::String(Some(st.tenant.clone()))])
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
        let slug = r.try_get::<String>("", "slug").unwrap_or_default();
        let visibility = r.try_get::<String>("", "visibility").unwrap_or_default();
        let paid_level = r.try_get::<i64>("", "paid_level").unwrap_or(0);
        let paid = crate::agent_layer::is_gated(&visibility, paid_level);
        out.push((id, title, summary, updated, img, slug, paid));
    }
    Ok(out)
}

/// 语义化 URL key：slug 非空用 slug，否则回退 id
fn url_key(id: &str, slug: &str) -> String {
    if slug.trim().is_empty() {
        id.to_string()
    } else {
        slug.to_string()
    }
}

/// GET /sitemap.xml
///
/// 收录**全部**已发布文章（含付费项）—— 只暴露 URL，让付费内容保持可发现。
pub async fn sitemap(State(st): State<AppState>) -> impl IntoResponse {
    let base = base_url();
    let home = home_base(&st, &base).await;
    let urls = match published_articles(&st, false).await {
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
        "  <url><loc>{}</loc><changefreq>daily</changefreq><priority>1.0</priority></url>\n",
        // 不带尾斜杠：`/t/<slug>/` 会被 matchit 判成另一条路径（`/t/<slug>` 才是
        // 200 的规范地址）。sitemap 必须列**能解析成功**的 URL，否则等于提交死链。
        // （`/t/<slug>/` 现在也能用了 —— 见 `main.rs::spa_fallback` 的归一化分支。）
        esc_xml(&home)
    ));
    for (id, _title, _summary, updated, _img, slug, _paid) in urls {
        let key = url_key(&id, &slug);
        body.push_str(&format!(
            "  <url><loc>{}</loc><lastmod>{}</lastmod><changefreq>weekly</changefreq><priority>0.8</priority></url>\n",
            esc_xml(&article_url(&home, &key)),
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
///
/// 只含**完全公开**的文章（`visibility='public'` 且 `paid_level=0`）——
/// 理由见 `published_articles` 的策略注释。
pub async fn rss(State(st): State<AppState>) -> impl IntoResponse {
    let base = base_url();
    let home = home_base(&st, &base).await;
    let items = match published_articles(&st, true).await {
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
        esc_xml(&home),
        esc_xml("基于 Rust 的开源 CMS 内容发布"),
        esc_xml(&base)
    ));
    for (id, title, summary, updated, _img, slug, _paid) in items {
        let key = url_key(&id, &slug);
        let pub_date = DateTime::parse_from_rfc3339(&updated)
            .map(|d| d.to_rfc2822())
            .unwrap_or_else(|_| updated.clone());
        body.push_str("  <item>\n");
        body.push_str(&format!("    <title>{}</title>\n", esc_xml(&title)));
        body.push_str(&format!(
            "    <link>{}</link>\n",
            esc_xml(&article_url(&home, &key))
        ));
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
///
/// 此前只写 `Allow: /`，等于把 `/api/*`、`/graphql`、`/graphiql` 全部欢迎来抓 ——
/// 对搜索引擎毫无价值，却会带来无意义的爬取与索引。这里显式屏蔽后台接口与
/// GraphQL 入口，只放行站点内容与静态资源。
///
/// 2026-09-22 追加 **AI 爬虫显式声明段**：把「欢迎检索型、也允许训练型」这条
/// 政策写进协议，而不是靠 `*` 隐式放行。功能上与 `*` 组等价（同一个
/// allow/disallow 集合），价值在于**声明意图**——日后若要收紧 `*` 组，
/// 这些爬虫不会被动连带失联。
///
/// 另注：这里**不**放行 `/api/`。机器读内容走 HTML（已有服务期头注）与
/// `/llms.txt` / `/agent.json`，不必让爬虫把 API 当抓取入口压。
pub async fn robots() -> impl IntoResponse {
    let base = base_url();
    // 注意：robots.txt 是纯文本，**不能**做 XML 转义 —— 否则 `&` 会变成 `&amp;`
    // 混进 Sitemap 行的 URL 里。esc_xml 只用于 /sitemap.xml 与 /rss.xml。
    let rules = "Allow: /\nDisallow: /api/\nDisallow: /graphql\nDisallow: /graphiql\n";
    let mut body = String::new();
    body.push_str("User-agent: *\n");
    body.push_str(rules);
    body.push_str(
        "\n# ── AI 检索型爬虫：显式欢迎（用户提问时 AI 能否引用本站，取决于此）──\n",
    );
    // RFC 9309：同一组内连续多条 User-agent 共享后面的规则，
    // 因此不必为每个爬虫重复一遍规则块。
    for b in crate::agent_layer::AI_SEARCH_CRAWLERS {
        body.push_str(&format!("User-agent: {b}\n"));
    }
    body.push_str(
        "\n# ── AI 训练型爬虫：同样放行（公开内容，进入语料有助于品牌被认识）──\n",
    );
    for b in crate::agent_layer::AI_TRAINING_CRAWLERS {
        body.push_str(&format!("User-agent: {b}\n"));
    }
    body.push_str(rules);
    body.push_str("\n# 机器可读入口\n");
    body.push_str(&format!("# {base}/llms.txt   面向语言模型的站点说明\n"));
    body.push_str(&format!("# {base}/agent.json Agent Manifest（能力 / 套餐 / 动作）\n"));
    body.push_str(&format!("\nSitemap: {base}/sitemap.xml\n"));
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        body,
    )
}
