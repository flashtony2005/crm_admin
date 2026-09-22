//! Agent Layer —— 让公开站点同时满足「人可读 / 机器可读 / 机器可调用」。
//!
//! ## 为什么需要这个模块
//!
//! 公开主页是 Vite 构建的 CSR 产物（`server/templates/<slug>/index.html`，约 2 KB），
//! 正文全部靠 JS 运行时 fetch。结果：**机器人拿到的是一个空文档** ——
//! `og:*` / `ld+json` / `twitter:*` / `noscript` 全为 0，`<body>` 里只有
//! `<div id="root">`。sitemap 收录的每个 URL 都指向这个空壳，`/post/<slug>`
//! 还是客户端路由。于是「结构化数据」「机器可读事实单元」「问题页面」这些
//! 获客动作全部无从谈起；顺带连**人分享链接都没有预览卡**。
//!
//! ## 本模块提供两条能力（都从 DB 派生：零迁移、零新表）
//!
//! 1. **服务期头注** —— `build_site_head` / `build_article_head` 产出注入片段，
//!    `inject_head` 做纯字符串合并。`templates.rs` 在伺服 `index.html` 时调用，
//!    把 title / description / canonical / OG / Twitter / JSON-LD 写进 `<head>`，
//!    并在 `<body>` 首部追加 `<noscript>` 正文兜底。**磁盘文件不改**，页面
//!    永远反映 DB 当前事实，不会与后台配置漂移。
//! 2. **机器入口端点** —— `/llms.txt`、`/ai/product`（Markdown 说明书）、
//!    `/agent.json`（Agent Manifest）。三者与头注**共用同一份 `Facts` 快照**，
//!    因此不可能互相漂移。
//!
//! ## 两条硬边界
//!
//! - **付费内容绝不注入正文。** `paid_level > 0` 或 `visibility ∈ {paid, members}`
//!   时只注入标题 / 摘要 / URL，并在 JSON-LD 标 `isAccessibleForFree:false`。
//!   正文提取在这一层就短路 —— 宁可给爬虫一个空 body，也不让它绕过付费墙。
//!   这与 `/api/public/articles/{key}` 的 402 + preview 语义保持一致。
//! - **注入失败不影响页面。** DB 不可用 / 解析异常时只打 stderr 日志并
//!   **原样返回 HTML**。头注是增强，不是可用性依赖。
//!
//! ## 不做的事
//!
//! - 不做构建期静态快照：产物由 `coucouya/scripts/deploy_to_admin.py` 生成，
//!   在服务期注入可以让「后台改站点标题」立即生效，无需重新构建前端。
//! - 不加缓存：单实例 + 本地 SQLite，一次 HTML 请求只有 3～4 条小查询；
//!   加 TTL 缓存反而会让「刚改的标题没生效」变成难排查的问题。

use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use sea_orm::Value as SqlValue;
use serde_json::{json, Value};

use crate::cmsdb::Row;
use crate::state::AppState;

/// 注入标记：已含此注释的 HTML 直接跳过，避免重复注入（幂等）。
const MARKER: &str = "<!--agent-layer-->";

/// 明确欢迎的 **AI 检索型**爬虫 —— 它们决定「用户提问时 AI 会不会提到本站」，
/// 是 GEO 的主要发现通道，放行它们等于把站点放进 Agent 的候选集。
///
/// 名单会随厂商更名/新增而变化（历史上 OAI 就拆出过 OAI-SearchBot），
/// 因此这里只是**显式声明**，真正的兜底是 `User-agent: *` 那组规则 ——
/// 即使某天漏掉一个新爬虫，它仍按 `*` 组被正常放行。改名单只需改这一处。
pub(crate) const AI_SEARCH_CRAWLERS: &[&str] = &[
    "OAI-SearchBot",   // OpenAI 搜索索引
    "ChatGPT-User",    // ChatGPT 应用户请求即时抓取
    "PerplexityBot",   // Perplexity 索引
    "Perplexity-User", // Perplexity 即时抓取
    "Claude-SearchBot",
    "Claude-User",
    "DuckAssistBot",
    "YouBot",
    "Applebot",        // Siri / Spotlight（区别于 Applebot-Extended）
    "Amazonbot",
    "MistralAI-User",
    "Meta-ExternalFetcher",
];

/// **训练型**爬虫：一并放行。
///
/// 判断依据：本站是公开营销/内容站，被收录进训练数据有助于品牌被"知道"；
/// 若日后要收紧（例如上线付费内容后担心语料被免费吸收），只需把这里的名字
/// 移到上面的检索名单之外 —— 不改任何其它代码。
pub(crate) const AI_TRAINING_CRAWLERS: &[&str] = &[
    "GPTBot",
    "Google-Extended",
    "Applebot-Extended",
    "anthropic-ai",
    "ClaudeBot",
    "CCBot",
    "Bytespider",
    "Meta-ExternalAgent",
    "cohere-ai",
];

/// noscript 正文兜底里最多放多少字符（防止把页面撑大）。
const NOSCRIPT_TEXT_LIMIT: usize = 1200;
/// JSON-LD `articleBody` 最多放多少字符。
const JSONLD_BODY_LIMIT: usize = 800;

// ───────────────────────── 小工具 ─────────────────────────

/// 读 TEXT 列（NULL → 空串）
fn s(r: &Row, col: &str) -> String {
    r.try_get::<String>("", col).unwrap_or_default()
}

/// 读可空 TEXT 列（NULL → 空串）
fn so(r: &Row, col: &str) -> String {
    r.try_get::<Option<String>>("", col)
        .ok()
        .flatten()
        .unwrap_or_default()
}

/// 读整数列（NULL / 型别不符 → 0）
fn n(r: &Row, col: &str) -> i64 {
    r.try_get::<i64>("", col).unwrap_or(0)
}

/// 大小写不敏感的子串查找。
///
/// `to_ascii_lowercase` 只映射 ASCII，非 ASCII 字符（中文等）原样保留、
/// **字节长度不变**，因此返回的下标可以直接用于原始字符串切片。
fn find_ci(hay: &str, needle: &str) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    hay.to_ascii_lowercase()
        .find(&needle.to_ascii_lowercase())
}

/// HTML 文本转义（用于 nouscript 与属性值）
fn esc_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// JSON 字符串里的 `<` / `>` 转成 `\u003c` / `\u003e`。
///
/// 必须做：JSON-LD 内嵌在 `<script>` 里，正文若出现 `</script>` 会**提前
/// 终止脚本块**，把结构化数据变成注入点。`<` `>` 在 JSON 里只可能出现在
/// 字符串字面量中，所以这个替换是安全的。
fn json_inline(s: &str) -> String {
    s.replace('<', "\\u003c").replace('>', "\\u003e")
}

/// 按字符边界安全截断（不会切坏多字节字符）
fn truncate_chars(s: &str, limit: usize) -> String {
    if s.chars().count() <= limit {
        return s.to_string();
    }
    let mut out: String = s.chars().take(limit).collect();
    out.push('…');
    out
}

/// 折叠连续空白
fn collapse_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_space = false;
    for ch in s.chars() {
        if ch.is_whitespace() {
            if !prev_space && !out.is_empty() {
                out.push(' ');
            }
            prev_space = true;
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    out.trim().to_string()
}

/// 去掉 `<tag ...> ... </tag>` 整块（含内容）。用于剔除 script / style。
fn strip_blocks(h: &str, tag: &str) -> String {
    let open = format!("<{tag}");
    let close = format!("</{tag}");
    let mut out = String::with_capacity(h.len());
    let mut rest = h;
    loop {
        match find_ci(rest, &open) {
            None => {
                out.push_str(rest);
                break;
            }
            Some(i) => {
                out.push_str(&rest[..i]);
                let after = &rest[i..];
                match find_ci(after, &close) {
                    // 未闭合：丢弃剩余（保守，不把脚本内容当正文）
                    None => break,
                    Some(j) => {
                        let tail = &after[j..];
                        match tail.find('>') {
                            None => break,
                            Some(k) => rest = &tail[k + 1..],
                        }
                    }
                }
            }
        }
    }
    out
}

/// 去掉所有 `<...>` 标签（用深度计数，容忍孤立 `<` / `>`）。
fn strip_tags(h: &str) -> String {
    let mut out = String::with_capacity(h.len());
    let mut depth: u32 = 0;
    for ch in h.chars() {
        match ch {
            '<' => depth += 1,
            '>' => {
                if depth > 0 {
                    depth -= 1;
                    out.push(' '); // 用空格分隔，避免标签两侧文字粘连
                }
            }
            _ if depth == 0 => out.push(ch),
            _ => {}
        }
    }
    out
}

/// 常见 HTML 实体解码（够用即止）
fn decode_entities(s: &str) -> String {
    s.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&mdash;", "—")
        .replace("&ndash;", "–")
        .replace("&hellip;", "…")
        .replace("&ldquo;", "“")
        .replace("&rdquo;", "”")
        .replace("&middot;", "·")
}

/// HTML 正文 → 纯文本（供 noscript / JSON-LD `articleBody` 使用）。
///
/// 先剔 script/style 整块，再剥标签，最后解实体并折叠空白。标签剥离顺带
/// 带走内联 `data:image/png;base64,...` 这类属性 —— 否则会把页面撑到几百 KB。
pub fn html_to_text(html: &str, limit: usize) -> String {
    let no_script = strip_blocks(html, "script");
    let no_style = strip_blocks(&no_script, "style");
    let no_tags = strip_tags(&no_style);
    let decoded = decode_entities(&no_tags);
    truncate_chars(&collapse_ws(&decoded), limit)
}

/// tags 字段解析：兼容纯文本（`支付方式`）、逗号分隔、以及 JSON 数组字面量（`[]`）。
fn parse_tags(raw: &str) -> Vec<String> {
    let t = raw.trim();
    if t.is_empty() || t == "[]" {
        return Vec::new();
    }
    t.trim_matches(|c| c == '[' || c == ']')
        .split([',', '，', ';', '；'])
        .map(|x| x.trim().trim_matches(|c| c == '"' || c == '\'').trim())
        .filter(|x| !x.is_empty())
        .map(|x| x.to_string())
        .collect()
}

/// 该文章是否对匿名机器访客设了门槛。
///
/// 与 `public_api.rs::article_detail` 的门槛口径保持一致：`paid_level > 0`
/// 覆盖订阅/积分/邀请三档，`visibility` 是非空时作为旧字段兜底。
pub fn is_gated(visibility: &str, paid_level: i64) -> bool {
    paid_level > 0 || visibility == "paid" || visibility == "members"
}

// ───────────────────────── 事实快照 ─────────────────────────

/// 一篇公开文章的概要（不含正文 —— 正文只在使用时按需取，避免无谓 IO）
#[derive(Debug, Clone)]
pub struct ArticleBrief {
    pub id: String,
    pub slug: String,
    /// URL key：slug 非空用 slug，否则回落 id
    pub key: String,
    pub url: String,
    pub title: String,
    pub summary: String,
    pub published_at: String,
    pub updated_at: String,
    pub tags: Vec<String>,
    pub paid: bool,
}

/// 可售套餐
#[derive(Debug, Clone)]
pub struct TierBrief {
    pub slug: String,
    pub name: String,
    pub description: String,
    pub price_monthly: f64,
    pub price_yearly: f64,
    pub features: Vec<String>,
}

/// 站点事实快照 —— 头注与三个机器端点共用的唯一数据源
#[derive(Debug, Clone)]
pub struct Facts {
    /// 站点根 URL（无尾斜杠）
    pub site_url: String,
    /// 后端 origin（用于拼 `/rss.xml`、`/llms.txt` 等非页面端点）
    pub origin: String,
    pub brand: String,
    pub brand_en: String,
    pub title: String,
    pub tagline: String,
    /// 面向机器的长描述（优先取「关于」区块首段，回落 tagline）
    pub description: String,
    /// 内容主题（区块标题 + 文章标签去重）
    pub topics: Vec<String>,
    pub sections: Vec<(String, String, String)>,
    pub nav: Vec<(String, String)>,
    pub footer: Vec<(String, String)>,
    pub tiers: Vec<TierBrief>,
    pub articles: Vec<ArticleBrief>,
}

impl Facts {
    /// 公开文章数 / 其中付费或有门槛的数量
    pub fn counts(&self) -> (usize, usize) {
        let paid = self.articles.iter().filter(|a| a.paid).count();
        (self.articles.len(), paid)
    }
}

/// 站点根 URL：配了 `PUBLIC_HOME_URL` 用它（对外门面），否则用后端上的规范路径。
pub fn site_url_for_slug(slug: &str) -> String {
    if let Some(h) = crate::seo::public_home_url() {
        return h;
    }
    format!("{}/t/{}", crate::seo::base_url().trim_end_matches('/'), slug)
}

/// 未指定 slug 时（独立机器端点）的站点根：同样优先 `PUBLIC_HOME_URL`。
pub fn site_url_for_active(active: &str) -> String {
    site_url_for_slug(active)
}

/// 文章详情对外 URL。
///
/// 刻意**不复用** `seo::article_url`：那个函数的兜底是 `{base}/read/{key}`
/// （后台 SPA 的阅读页），而 agent_layer 永远知道主页 base，能给出规范深链。
/// 实测教训：`/read/<key>` 在未设 `STATIC_DIR` 的部署下直接 404 —— 让机器
/// 读的说明书指向死链，比不给 URL 更糟。
pub fn article_url_for(site_url: &str, key: &str) -> String {
    if let Ok(t) = std::env::var("PUBLIC_ARTICLE_URL") {
        let t = t.trim();
        if !t.is_empty() {
            return t.replace("{key}", key);
        }
    }
    format!("{}/post/{}", site_url.trim_end_matches('/'), key)
}

/// 从 DB 采集站点事实快照。任一步查询失败 → `Err`，由调用方决定降级。
pub async fn facts(st: &AppState, site_url: &str) -> Result<Facts, String> {
    let origin = crate::seo::base_url();
    let origin = origin.trim_end_matches('/').to_string();

    // ── 站点设置 ──
    let settings = st
        .db
        .query_all(
            "SELECT key, value FROM site_settings WHERE tenant_id = ?",
            vec![SqlValue::String(Some(st.tenant.clone()))],
        )
        .await
        .map_err(|e| format!("读取 site_settings 失败：{e}"))?;
    let mut kv = std::collections::HashMap::<String, String>::new();
    for r in &settings {
        kv.insert(s(r, "key"), s(r, "value"));
    }
    let get = |k: &str| kv.get(k).cloned().unwrap_or_default();

    let title = {
        let v = get("site_title");
        if v.trim().is_empty() {
            "LightPress".to_string()
        } else {
            v
        }
    };
    let tagline = get("site_tagline");

    // ── 首页区块（品牌 + 关于，用于 brand / description / topics）──
    let sec_rows = st
        .db
        .query_all(
            "SELECT slug, title, subtitle, body FROM sections WHERE tenant_id = ? \
             ORDER BY sort ASC, updated_at ASC",
            vec![SqlValue::String(Some(st.tenant.clone()))],
        )
        .await
        .map_err(|e| format!("读取 sections 失败：{e}"))?;

    let mut sections: Vec<(String, String, String)> = Vec::new();
    let mut brand = String::new();
    let mut brand_en = String::new();
    let mut nav_from_section: Vec<(String, String)> = Vec::new();
    let mut footer_from_section: Vec<(String, String)> = Vec::new();
    let mut description = String::new();

    for r in &sec_rows {
        let slug = s(r, "slug");
        let t = s(r, "title");
        let sub = s(r, "subtitle");
        sections.push((slug.clone(), t, sub));

        let body: Value = serde_json::from_str(&s(r, "body")).unwrap_or(Value::Null);
        if let Some(b) = body.get("brand").and_then(|x| x.as_str()) {
            brand = b.to_string();
        }
        if let Some(b) = body.get("brandEn").and_then(|x| x.as_str()) {
            brand_en = b.to_string();
        }
        // 导航 / 页脚：优先读区块内嵌（coucouya 的 site 区块），
        // 无则回落到 nav_links 表（后台「站点外观 → 导航与页脚」）。
        if let Some(links) = body.pointer("/nav/links").and_then(|x| x.as_array()) {
            for l in links {
                if let (Some(lb), Some(hr)) = (
                    l.get("label").and_then(|x| x.as_str()),
                    l.get("href").and_then(|x| x.as_str()),
                ) {
                    nav_from_section.push((lb.to_string(), hr.to_string()));
                }
            }
        }
        if let Some(links) = body.pointer("/footer/links").and_then(|x| x.as_array()) {
            for l in links {
                if let (Some(lb), Some(hr)) = (
                    l.get("label").and_then(|x| x.as_str()),
                    l.get("href").and_then(|x| x.as_str()),
                ) {
                    footer_from_section.push((lb.to_string(), hr.to_string()));
                }
            }
        }
        // 机器描述优先取「关于」区块的首段 bio —— 那是发布者自己写的定位陈述，
        // 比 tagline 更适合作为 Agent 判断「这是干什么的」的输入。
        if description.is_empty() && slug == "about" {
            if let Some(bio) = body.pointer("/hero/bio").and_then(|x| x.as_str()) {
                let first = bio.split("\n\n").next().unwrap_or(bio);
                description = collapse_ws(first);
            }
        }
    }
    if description.is_empty() {
        description = tagline.clone();
    }

    // ── nav_links 表兜底 ──
    let nav_rows = st
        .db
        .query_all(
            "SELECT grp, label, href FROM nav_links WHERE tenant_id = ? AND enabled = 1 \
             ORDER BY grp ASC, sort ASC, created_at ASC",
            vec![SqlValue::String(Some(st.tenant.clone()))],
        )
        .await
        .unwrap_or_default();
    let mut nav = nav_from_section;
    let mut footer = footer_from_section;
    for r in &nav_rows {
        let item = (s(r, "label"), s(r, "href"));
        if item.0.is_empty() || item.1.is_empty() {
            continue;
        }
        if s(r, "grp") == "footer" {
            if !footer.iter().any(|x| x.1 == item.1) {
                footer.push(item);
            }
        } else if !nav.iter().any(|x| x.1 == item.1) {
            nav.push(item);
        }
    }

    // ── 套餐 ──
    let tier_rows = st
        .db
        .query_all(
            "SELECT slug, name, description, price_monthly, price_yearly, features, active \
             FROM tiers WHERE tenant_id = ? ORDER BY price_monthly ASC",
            vec![SqlValue::String(Some(st.tenant.clone()))],
        )
        .await
        .unwrap_or_default();
    let mut tiers = Vec::new();
    for r in &tier_rows {
        if n(r, "active") == 0 {
            continue;
        }
        tiers.push(TierBrief {
            slug: s(r, "slug"),
            name: s(r, "name"),
            description: s(r, "description"),
            price_monthly: r.try_get::<f64>("", "price_monthly").unwrap_or(0.0),
            price_yearly: r.try_get::<f64>("", "price_yearly").unwrap_or(0.0),
            features: parse_tags(&s(r, "features")),
        });
    }

    // ── 已发布文章 ──
    // 注意：**包含付费内容**（只取标题/摘要/URL，不含正文）。付费内容仍需
    // 可被 Agent 发现与推荐，否则等于放弃转化；泄漏边界由「不给正文」保证。
    let art_rows = st
        .db
        .query_all(
            "SELECT id, title, slug, summary, tags, published_at, updated_at, \
                    visibility, paid_level FROM articles \
             WHERE tenant_id = ? AND status = 'published' \
             ORDER BY COALESCE(published_at, updated_at) DESC LIMIT 60",
            vec![SqlValue::String(Some(st.tenant.clone()))],
        )
        .await
        .map_err(|e| format!("读取 articles 失败：{e}"))?;

    let mut articles = Vec::new();
    for r in &art_rows {
        let id = s(r, "id");
        let slug = s(r, "slug");
        let key = if slug.trim().is_empty() {
            id.clone()
        } else {
            slug.clone()
        };
        let visibility = s(r, "visibility");
        let paid_level = n(r, "paid_level");
        articles.push(ArticleBrief {
            url: article_url_for(site_url, &key),
            key,
            id,
            slug,
            title: s(r, "title"),
            summary: collapse_ws(&s(r, "summary")),
            published_at: so(r, "published_at"),
            updated_at: s(r, "updated_at"),
            tags: parse_tags(&s(r, "tags")),
            paid: is_gated(&visibility, paid_level),
        });
    }

    // ── topics：区块标题 + 文章标签去重（喂给 Agent 做「适合问什么」的判断）──
    // topics 优先取**文章标签** —— 那是发布者自己标的内容主题，也是读者真正
    // 会去搜的东西。只有在完全没有标签时，才退回区块标题兜底。
    // （直接用区块标题会把「组织」「实验」这类站点结构名当成内容主题，
    //  让 Agent 误判「这个站点讲组织管理」。）
    let mut topics: Vec<String> = Vec::new();
    for a in &articles {
        for t in &a.tags {
            if !topics.contains(t) {
                topics.push(t.clone());
            }
        }
    }
    if topics.is_empty() {
        for (slug, t, _) in &sections {
            if matches!(slug.as_str(), "site" | "about") {
                continue;
            }
            if !t.trim().is_empty() && !topics.contains(t) {
                topics.push(t.clone());
            }
        }
    }
    topics.truncate(24);

    if brand.trim().is_empty() {
        brand = title.clone();
    }

    Ok(Facts {
        site_url: site_url.trim_end_matches('/').to_string(),
        origin,
        brand,
        brand_en,
        title,
        tagline,
        description,
        topics,
        sections,
        nav,
        footer,
        tiers,
        articles,
    })
}

/// 单篇文章完整行（含正文；仅文章详情页按需查询）
pub struct ArticleFull {
    pub brief: ArticleBrief,
    pub content: String,
    pub meta_title: String,
    pub meta_description: String,
    pub canonical_url: String,
    pub featured_image: String,
    pub author: String,
}

/// 按 id 或 slug 取单篇已发布文章。`site_url` 决定文章对外 URL（见 `article_url_for`）。
pub async fn article_full(
    st: &AppState,
    site_url: &str,
    key: &str,
) -> Result<Option<ArticleFull>, String> {
    let rows = st
        .db
        .query_all(
            "SELECT id, title, slug, summary, content, tags, author, published_at, updated_at, \
                    visibility, paid_level, meta_title, meta_description, canonical_url, \
                    featured_image FROM articles \
             WHERE tenant_id = ? AND status = 'published' AND (id = ? OR slug = ?) LIMIT 1",
            vec![
                SqlValue::String(Some(st.tenant.clone())),
                SqlValue::String(Some(key.to_string())),
                SqlValue::String(Some(key.to_string())),
            ],
        )
        .await
        .map_err(|e| format!("查询文章失败：{e}"))?;
    let Some(r) = rows.first() else {
        return Ok(None);
    };
    let id = s(r, "id");
    let slug = s(r, "slug");
    let k = if slug.trim().is_empty() {
        id.clone()
    } else {
        slug.clone()
    };
    Ok(Some(ArticleFull {
        brief: ArticleBrief {
            url: article_url_for(site_url, &k),
            key: k,
            id,
            slug,
            title: s(r, "title"),
            summary: collapse_ws(&s(r, "summary")),
            published_at: so(r, "published_at"),
            updated_at: s(r, "updated_at"),
            tags: parse_tags(&s(r, "tags")),
            paid: is_gated(&s(r, "visibility"), n(r, "paid_level")),
        },
        content: s(r, "content"),
        meta_title: s(r, "meta_title"),
        meta_description: s(r, "meta_description"),
        canonical_url: s(r, "canonical_url"),
        featured_image: so(r, "featured_image"),
        author: s(r, "author"),
    }))
}

// ───────────────────────── 头注片段 ─────────────────────────

/// 要注入 `<head>` 的一组事实。`None` 表示「保留模板原值」。
#[derive(Debug, Clone, Default)]
pub struct HeadInjection {
    /// 覆盖 `<title>`；None = 保留模板自带的
    pub title: Option<String>,
    /// 模板没有 `<title>` 时的兜底标题
    pub fallback_title: Option<String>,
    /// 覆盖 `<meta name="description">`；None = 保留模板自带的
    pub description: Option<String>,
    pub canonical: Option<String>,
    pub og_type: &'static str,
    pub og_site_name: Option<String>,
    pub image: Option<String>,
    pub published: Option<String>,
    pub locale: Option<String>,
    /// 额外的 `<meta property>` / `<meta name>` 对（如 article:tag）
    pub extra_meta: Vec<(String, String)>,
    /// 结构化数据（原始 JSON 字符串）
    pub jsonld: Vec<String>,
    /// `<body>` 首部的 noscript 兜底内容（已是 HTML 片段）
    pub noscript: Option<String>,
}

/// 站点首页头注
pub fn build_site_head(f: &Facts) -> HeadInjection {
    let (total, paid) = f.counts();
    let desc = if !f.description.is_empty() {
        f.description.clone()
    } else if !f.tagline.is_empty() {
        f.tagline.clone()
    } else {
        f.title.clone()
    };

    // WebSite + Organization + ItemList（站点知识图谱的骨架）
    let mut site_ld = json!({
        "@context": "https://schema.org",
        "@type": "WebSite",
        "name": f.title,
        "alternateName": f.brand,
        "url": f.site_url,
        "description": desc,
        "inLanguage": "zh-CN",
    });
    if !f.topics.is_empty() {
        site_ld["keywords"] = json!(f.topics.join(", "));
    }
    if !f.articles.is_empty() {
        site_ld["hasPart"] = json!(f
            .articles
            .iter()
            .take(20)
            .map(|a| json!({
                "@type": "Article",
                "headline": a.title,
                "url": a.url,
            }))
            .collect::<Vec<_>>());
    }

    let org_ld = json!({
        "@context": "https://schema.org",
        "@type": "Organization",
        "name": f.brand,
        "alternateName": f.brand_en,
        "url": f.site_url,
        "description": desc,
        "sameAs": f.nav.iter().chain(f.footer.iter())
            .map(|(_, h)| h.clone())
            .filter(|h| h.starts_with("http"))
            .collect::<Vec<_>>(),
    });

    let faq_ld = faq_from_facts(f);
    let mut json_ld = vec![site_ld.to_string(), org_ld.to_string()];
    if let Some(q) = faq_ld {
        json_ld.push(q.to_string());
    }

    HeadInjection {
        title: None,
        fallback_title: Some(if f.tagline.is_empty() {
            f.title.clone()
        } else {
            format!("{} · {}", f.title, f.tagline)
        }),
        description: None,
        canonical: Some(f.site_url.clone()),
        og_type: "website",
        og_site_name: Some(f.title.clone()),
        image: None,
        published: None,
        locale: Some("zh_CN".into()),
        // twitter:card 交给 inject_head 按"有没有图"决定，不在这里写死：
        // 声明 summary_large_image 却没有图，抓取器会退化成无图大卡，比 summary 更糟。
        extra_meta: Vec::new(),
        jsonld: json_ld,
        // 站点标题 > 摘要 > 最新文章 > 导航：给无 JS 的抓取者一份可读的门面，
        // 每个条目都带绝对 URL，Agent 可以直接取用。
        noscript: Some(noscript_site(f, total, paid)),
    }
}

/// 依据站点事实生成 FAQPage（问题取自区块标题与常见问法模板）。
///
/// 只产出**有事实支撑**的问答 —— 没套餐时不编造价格问题，避免"结构正确、
/// 内容为空/虚构"的实体（那会被判为低质结构化数据）。
fn faq_from_facts(f: &Facts) -> Option<Value> {
    let mut qa: Vec<Value> = Vec::new();
    if !f.description.is_empty() {
        qa.push(json!({
            "@type": "Question",
            "name": format!("{} 是什么？", f.brand),
            "acceptedAnswer": { "@type": "Answer", "text": f.description }
        }));
    }
    if !f.topics.is_empty() {
        qa.push(json!({
            "@type": "Question",
            "name": format!("{} 主要讲什么内容？", f.brand),
            "acceptedAnswer": {
                "@type": "Answer",
                "text": format!("主要内容主题：{}。", f.topics.join("、"))
            }
        }));
    }
    let (total, _) = f.counts();
    if total > 0 {
        qa.push(json!({
            "@type": "Question",
            "name": "在哪里可以阅读全部文章？",
            "acceptedAnswer": {
                "@type": "Answer",
                "text": format!("共 {} 篇公开文章，首页 {} 列出最新内容，完整清单见 {}。",
                    total, f.site_url, format!("{}/sitemap.xml", f.origin))
            }
        }));
    }
    if qa.is_empty() {
        return None;
    }
    Some(json!({ "@context": "https://schema.org", "@type": "FAQPage", "mainEntity": qa }))
}

/// 文章详情页头注
pub fn build_article_head(f: &Facts, a: &ArticleFull) -> HeadInjection {
    let title = if a.meta_title.trim().is_empty() {
        a.brief.title.clone()
    } else {
        a.meta_title.clone()
    };
    let desc = if !a.meta_description.trim().is_empty() {
        a.meta_description.clone()
    } else if !a.brief.summary.is_empty() {
        a.brief.summary.clone()
    } else if a.brief.paid {
        // 付费内容没摘要时 **不要** 用正文兜底，宁可退回站点一句话
        f.description.clone()
    } else {
        html_to_text(&a.content, 200)
    };
    let canonical = if a.canonical_url.trim().is_empty() {
        a.brief.url.clone()
    } else {
        a.canonical_url.clone()
    };

    let mut extra: Vec<(String, String)> = Vec::new();
    if !a.brief.updated_at.is_empty() {
        extra.push(("article:modified_time".into(), a.brief.updated_at.clone()));
    }
    for t in &a.brief.tags {
        extra.push(("article:tag".into(), t.clone()));
    }

    // 结构化数据：始终输出 Article，但**付费内容不写 articleBody**，
    // 并显式标 isAccessibleForFree:false —— 让 Agent 知道「有这篇、要付费」，
    // 而不是「内容为空」。
    // （作者名先算好：宏字面量里塞 if/else 会让 `$value:expr` 匹配变得脆弱。）
    let author_name = if a.author.trim().is_empty() {
        f.brand.clone()
    } else {
        a.author.clone()
    };
    let mut art = json!({
        "@context": "https://schema.org",
        "@type": "Article",
        "headline": title,
        "url": canonical,
        "description": desc,
        "inLanguage": "zh-CN",
        "isAccessibleForFree": !a.brief.paid,
        "author": { "@type": "Person", "name": author_name },
        "publisher": { "@type": "Organization", "name": f.brand },
    });
    if !a.brief.published_at.is_empty() {
        art["datePublished"] = json!(a.brief.published_at);
    }
    if !a.brief.updated_at.is_empty() {
        art["dateModified"] = json!(a.brief.updated_at);
    }
    if !a.brief.tags.is_empty() {
        art["keywords"] = json!(a.brief.tags.join(", "));
    }
    if !a.featured_image.is_empty() {
        art["image"] = json!(a.featured_image);
    }
    // 纯文本正文：noscript 用较大上限（抓取者能读到更多），
    // JSON-LD 的 articleBody 用小上限（结构化数据别把页面撑大）。
    // 付费内容两者都为空 —— 正文在这一层就短路，不给爬虫任何绕过付费墙的机会。
    let plain = if a.brief.paid {
        String::new()
    } else {
        html_to_text(&a.content, NOSCRIPT_TEXT_LIMIT)
    };
    if !plain.is_empty() {
        art["articleBody"] = json!(truncate_chars(&plain, JSONLD_BODY_LIMIT));
    }

    let crumbs = json!({
        "@context": "https://schema.org",
        "@type": "BreadcrumbList",
        "itemListElement": [
            { "@type": "ListItem", "position": 1, "name": f.title, "item": f.site_url },
            { "@type": "ListItem", "position": 2, "name": a.brief.title, "item": canonical },
        ]
    });

    HeadInjection {
        title: Some(title),
        fallback_title: None,
        description: Some(desc),
        canonical: Some(canonical),
        og_type: "article",
        og_site_name: Some(f.title.clone()),
        image: if a.featured_image.is_empty() {
            None
        } else {
            Some(a.featured_image.clone())
        },
        published: if a.brief.published_at.is_empty() {
            None
        } else {
            Some(a.brief.published_at.clone())
        },
        locale: Some("zh_CN".into()),
        extra_meta: extra,
        jsonld: vec![art.to_string(), crumbs.to_string()],
        noscript: Some(noscript_article(f, a, &plain)),
    }
}

/// 站点 noscript 兜底
fn noscript_site(f: &Facts, total: usize, paid: usize) -> String {
    let mut h = String::from("<noscript><div id=\"agent-layer-fallback\">");
    let name = if f.brand_en.is_empty() {
        f.brand.clone()
    } else {
        format!("{} {}", f.brand, f.brand_en)
    };
    h.push_str(&format!("<h1>{}</h1>", esc_html(&name)));
    if !f.tagline.is_empty() {
        h.push_str(&format!("<p>{}</p>", esc_html(&f.tagline)));
    }
    if !f.description.is_empty() && f.description != f.tagline {
        h.push_str(&format!("<p>{}</p>", esc_html(&f.description)));
    }
    if !f.articles.is_empty() {
        h.push_str("<h2>最新内容</h2><ul>");
        for a in f.articles.iter().take(20) {
            h.push_str(&format!(
                "<li><a href=\"{}\">{}</a>{}{}</li>",
                esc_html(&a.url),
                esc_html(&a.title),
                if a.summary.is_empty() {
                    String::new()
                } else {
                    format!(" — {}", esc_html(&a.summary))
                },
                if a.paid {
                    "（订阅会员专享）"
                } else {
                    ""
                }
            ));
        }
        h.push_str("</ul>");
    }
    if !f.tiers.is_empty() {
        h.push_str("<h2>订阅方案</h2><ul>");
        for t in &f.tiers {
            h.push_str(&format!(
                "<li>{}：月付 ¥{:.2}{}</li>",
                esc_html(&t.name),
                t.price_monthly,
                if t.price_yearly > 0.0 {
                    format!(" / 年付 ¥{:.2}", t.price_yearly)
                } else {
                    String::new()
                }
            ));
        }
        h.push_str("</ul>");
    }
    let mut links: Vec<(String, String)> = f.nav.clone();
    links.extend(f.footer.clone());
    if !links.is_empty() {
        h.push_str("<h2>链接</h2><ul>");
        for (label, href) in links {
            h.push_str(&format!(
                "<li><a href=\"{}\">{}</a></li>",
                esc_html(&href),
                esc_html(&label)
            ));
        }
        h.push_str("</ul>");
    }
    h.push_str(&format!(
        "<p>文章总数 {} 篇{}；<a href=\"{}/rss.xml\">RSS</a>；<a href=\"{}/sitemap.xml\">站点地图</a>；<a href=\"{}/llms.txt\">机器可读说明</a>。</p>",
        total,
        if paid > 0 {
            format!("，其中 {} 篇为订阅会员专享", paid)
        } else {
            String::new()
        },
        esc_html(&f.origin),
        esc_html(&f.origin),
        esc_html(&f.origin)
    ));
    h.push_str("</div></noscript>");
    h
}

/// 文章页 noscript 兜底。**付费内容只给标题/摘要/门槛说明，不给正文。**
fn noscript_article(f: &Facts, a: &ArticleFull, body: &str) -> String {
    let mut h = String::from("<noscript><div id=\"agent-layer-fallback\">");
    h.push_str(&format!(
        "<p><a href=\"{}\">{}</a></p>",
        esc_html(&f.site_url),
        esc_html(&f.title)
    ));
    h.push_str(&format!("<h1>{}</h1>", esc_html(&a.brief.title)));
    if !a.brief.published_at.is_empty() {
        h.push_str(&format!(
            "<p>发布于 {}</p>",
            esc_html(&a.brief.published_at)
        ));
    }
    if !a.brief.summary.is_empty() {
        h.push_str(&format!("<p>{}</p>", esc_html(&a.brief.summary)));
    }
    if a.brief.paid {
        h.push_str(
            "<p>本篇为订阅会员专享内容，需登录并订阅后阅读全文。未订阅访客仅可见上述摘要。</p>",
        );
    } else if !body.is_empty() {
        h.push_str(&format!("<div>{}</div>", esc_html(body)));
    }
    if !a.brief.tags.is_empty() {
        h.push_str(&format!(
            "<p>标签：{}</p>",
            esc_html(&a.brief.tags.join("、"))
        ));
    }
    h.push_str("</div></noscript>");
    h
}

// ───────────────────────── 注入器 ─────────────────────────

/// 取标签内某个属性的值（支持单/双引号，以及无引号形式）。
///
/// 要求属性名前是空白或标签起始 —— 否则 `data-name="x"` 会被误当成 `name="x"`。
fn attr_val(tag: &str, attr: &str) -> Option<String> {
    let tl = tag.to_ascii_lowercase();
    let pat = format!("{attr}=");
    let mut from = 0usize;
    loop {
        let i = tl[from..].find(&pat)? + from;
        let ok_before = i == 0
            || tl[..i]
                .chars()
                .last()
                .map(|c| c.is_whitespace() || c == '<')
                .unwrap_or(false);
        if ok_before {
            let rest = &tag[i + pat.len()..];
            let mut ch = rest.chars();
            match ch.next() {
                Some(q) if q == '"' || q == '\'' => {
                    let body = &rest[1..];
                    let end = body.find(q)?;
                    return Some(body[..end].to_string());
                }
                Some(_) => {
                    let end = rest
                        .find(|c: char| c.is_whitespace() || c == '>')
                        .unwrap_or(rest.len());
                    return Some(rest[..end].to_string());
                }
                None => return None,
            }
        }
        from = i + pat.len();
        if from >= tl.len() {
            return None;
        }
    }
}

/// 取页面已有的 `<title>` 文本（无则 None）
fn extract_title(html: &str) -> Option<String> {
    let i = find_ci(html, "<title")?;
    let j = html[i..].find('>')?;
    let start = i + j + 1;
    let k = find_ci(&html[start..], "</title")?;
    let t = collapse_ws(&decode_entities(&html[start..start + k]));
    if t.is_empty() {
        None
    } else {
        Some(t)
    }
}

/// 取页面已有的 `<meta name=... content=...>` 的 content（name / property / itemprop 都认）
fn meta_content(html: &str, key: &str) -> Option<String> {
    let mut rest = html;
    loop {
        let i = find_ci(rest, "<meta")?;
        let after = &rest[i..];
        let j = after.find('>')?;
        let tag = &after[..=j];
        let hit = ["name", "property", "itemprop"].iter().any(|a| {
            attr_val(tag, a)
                .map(|v| v.eq_ignore_ascii_case(key))
                .unwrap_or(false)
        });
        if hit {
            if let Some(c) = attr_val(tag, "content") {
                let c = c.trim().to_string();
                if !c.is_empty() {
                    return Some(c);
                }
            }
        }
        rest = &after[j + 1..];
    }
}

/// 把 `<meta ...>` 中 `property=` / `name=` / `itemprop=` 命中 `keys` 的整条移除。
fn strip_meta(html: &str, keys: &[String]) -> String {
    if keys.is_empty() {
        return html.to_string();
    }
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    loop {
        match find_ci(rest, "<meta") {
            None => {
                out.push_str(rest);
                break;
            }
            Some(i) => {
                out.push_str(&rest[..i]);
                let after = &rest[i..];
                match after.find('>') {
                    None => {
                        out.push_str(after);
                        break;
                    }
                    Some(j) => {
                        let tag = &after[..=j];
                        let tl = tag.to_ascii_lowercase();
                        let hit = keys.iter().any(|k| {
                            let k = k.to_ascii_lowercase();
                            tl.contains(&format!("property=\"{k}\""))
                                || tl.contains(&format!("name=\"{k}\""))
                                || tl.contains(&format!("itemprop=\"{k}\""))
                                || tl.contains(&format!("property='{k}'"))
                                || tl.contains(&format!("name='{k}'"))
                        });
                        if !hit {
                            out.push_str(tag);
                        }
                        rest = &after[j + 1..];
                    }
                }
            }
        }
    }
    out
}

/// 移除 `<link rel="canonical" ...>`
fn strip_canonical(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    loop {
        match find_ci(rest, "<link") {
            None => {
                out.push_str(rest);
                break;
            }
            Some(i) => {
                out.push_str(&rest[..i]);
                let after = &rest[i..];
                match after.find('>') {
                    None => {
                        out.push_str(after);
                        break;
                    }
                    Some(j) => {
                        let tag = &after[..=j];
                        let tl = tag.to_ascii_lowercase();
                        if !(tl.contains("rel=\"canonical\"") || tl.contains("rel='canonical'")) {
                            out.push_str(tag);
                        }
                        rest = &after[j + 1..];
                    }
                }
            }
        }
    }
    out
}

/// 移除已有的 `application/ld+json` 脚本块（避免注入后出现重复实体）
fn strip_ldjson(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    loop {
        match find_ci(rest, "<script") {
            None => {
                out.push_str(rest);
                break;
            }
            Some(i) => {
                out.push_str(&rest[..i]);
                let after = &rest[i..];
                let Some(j) = after.find('>') else {
                    out.push_str(after);
                    break;
                };
                let open_tag = &after[..=j];
                let is_ld = open_tag.to_ascii_lowercase().contains("ld+json");
                if !is_ld {
                    out.push_str(open_tag);
                    rest = &after[j + 1..];
                    continue;
                }
                // 跳过整个块
                match find_ci(&after[j + 1..], "</script") {
                    None => break,
                    Some(k) => {
                        let tail = &after[j + 1 + k..];
                        match tail.find('>') {
                            None => break,
                            Some(m) => rest = &tail[m + 1..],
                        }
                    }
                }
            }
        }
    }
    out
}

/// 把 `<title>` 内容替换掉。
fn replace_title(html: &str, title: &str) -> String {
    let Some(i) = find_ci(html, "<title") else {
        return html.to_string();
    };
    let Some(j) = html[i..].find('>') else {
        return html.to_string();
    };
    let content_start = i + j + 1;
    let Some(k) = find_ci(&html[content_start..], "</title") else {
        return html.to_string();
    };
    let mut out = String::with_capacity(html.len());
    out.push_str(&html[..content_start]);
    out.push_str(&esc_html(title));
    out.push_str(&html[content_start + k..]);
    out
}

/// 纯字符串注入：把 `HeadInjection` 合并进 HTML。
///
/// 顺序与语义：
/// 1. 已含 `MARKER` → 原样返回（幂等）；
/// 2. 标题：有 `title` 则替换 `<title>`；模板无 `<title>` 时用 `fallback_title` 插入；
/// 3. description：有 `description` 则先删旧 `<meta name="description">` 再插入；
/// 4. 删掉旧的 canonical 与 ld+json（我们总会重新给出）；
/// 5. 我们的 og/twitter 属性若模板已有，先删旧再插，避免重复；
/// 6. 所有片段插入 `</head>` 之前；
/// 7. noscript 插到 `<body>` 开标签之后（无 `<body>` 则跳过）。
pub fn inject_head(html: &str, h: &HeadInjection) -> String {
    if html.contains(MARKER) {
        return html.to_string();
    }
    let mut out = html.to_string();

    // ── 收集要覆盖的 meta key ──
    let mut keys: Vec<String> = Vec::new();
    if h.description.is_some() {
        keys.push("description".into());
    }
    // 我们总会重新给出的属性，先把模板里的旧值摘干净，避免同页重复声明。
    keys.push("og:type".into());
    keys.push("og:url".into());
    keys.push("og:title".into());
    keys.push("og:description".into());
    // Twitter/X 会回落 og:*，但显式给出可避免各抓取器实现差异。
    keys.push("twitter:card".into());
    keys.push("twitter:title".into());
    keys.push("twitter:description".into());
    keys.push("twitter:url".into());
    if h.og_site_name.is_some() {
        keys.push("og:site_name".into());
    }
    if h.image.is_some() {
        keys.push("og:image".into());
        keys.push("twitter:image".into());
    }
    if h.published.is_some() {
        keys.push("article:published_time".into());
    }
    for (k, _) in &h.extra_meta {
        keys.push(k.clone());
    }
    out = strip_meta(&out, &keys);
    out = strip_canonical(&out);
    out = strip_ldjson(&out);

    // ── title ──
    if let Some(t) = h.title.as_ref() {
        out = replace_title(&out, t);
    } else if let Some(fb) = h.fallback_title.as_ref() {
        if find_ci(&out, "<title").is_none() {
            if let Some(i) = find_ci(&out, "</head") {
                out.insert_str(i, &format!("<title>{}</title>\n", esc_html(fb)));
            }
        }
    }

    // ── 组装追加块 ──
    let mut block = String::new();
    block.push_str(MARKER);
    block.push('\n');
    if let Some(d) = h.description.as_ref() {
        if !d.is_empty() {
            block.push_str(&format!(
                "<meta name=\"description\" content=\"{}\" />\n",
                esc_html(d)
            ));
        }
    }
    if let Some(c) = h.canonical.as_ref() {
        if !c.is_empty() {
            block.push_str(&format!(
                "<link rel=\"canonical\" href=\"{}\" />\n",
                esc_html(c)
            ));
        }
    }
    // og:title / og:description 与页面自身的 <title> / description **取同一份文本**：
    // 站点根保留模板写的门面文案（不强行用 DB 覆盖"对人"的文案），文章页用 DB 值。
    // 两处若各取一个来源，同一个页面里 <title> 与 og:title 会说法不一 ——
    // 既像配置错乱，也是机器判断实体时的噪声。
    let page_title = h
        .title
        .clone()
        .or_else(|| extract_title(&out))
        .or_else(|| h.fallback_title.clone());
    let page_desc = h
        .description
        .clone()
        .or_else(|| meta_content(&out, "description"));
    if let Some(t) = page_title.as_ref() {
        block.push_str(&format!(
            "<meta property=\"og:title\" content=\"{}\" />\n",
            esc_html(t)
        ));
    }
    if let Some(d) = page_desc.as_ref() {
        if !d.is_empty() {
            block.push_str(&format!(
                "<meta property=\"og:description\" content=\"{}\" />\n",
                esc_html(d)
            ));
        }
    }
    if !h.og_type.is_empty() {
        block.push_str(&format!(
            "<meta property=\"og:type\" content=\"{}\" />\n",
            esc_html(h.og_type)
        ));
    }
    if let Some(u) = h.canonical.as_ref() {
        block.push_str(&format!(
            "<meta property=\"og:url\" content=\"{}\" />\n",
            esc_html(u)
        ));
    }
    if let Some(n) = h.og_site_name.as_ref() {
        block.push_str(&format!(
            "<meta property=\"og:site_name\" content=\"{}\" />\n",
            esc_html(n)
        ));
    }
    // Twitter 卡与 og 同源。card 类型按实际有无图片给值。
    block.push_str(&format!(
        "<meta name=\"twitter:card\" content=\"{}\" />\n",
        if h.image.is_some() {
            "summary_large_image"
        } else {
            "summary"
        }
    ));
    if let Some(t) = page_title.as_ref() {
        block.push_str(&format!(
            "<meta name=\"twitter:title\" content=\"{}\" />\n",
            esc_html(t)
        ));
    }
    if let Some(d) = page_desc.as_ref() {
        if !d.is_empty() {
            block.push_str(&format!(
                "<meta name=\"twitter:description\" content=\"{}\" />\n",
                esc_html(d)
            ));
        }
    }
    if let Some(u) = h.canonical.as_ref() {
        block.push_str(&format!(
            "<meta name=\"twitter:url\" content=\"{}\" />\n",
            esc_html(u)
        ));
    }
    if let Some(l) = h.locale.as_ref() {
        block.push_str(&format!(
            "<meta property=\"og:locale\" content=\"{}\" />\n",
            esc_html(l)
        ));
    }
    if let Some(i) = h.image.as_ref() {
        block.push_str(&format!(
            "<meta property=\"og:image\" content=\"{}\" />\n",
            esc_html(i)
        ));
        block.push_str(&format!(
            "<meta name=\"twitter:image\" content=\"{}\" />\n",
            esc_html(i)
        ));
    }
    if let Some(p) = h.published.as_ref() {
        if !p.is_empty() {
            block.push_str(&format!(
                "<meta property=\"article:published_time\" content=\"{}\" />\n",
                esc_html(p)
            ));
        }
    }
    for (k, v) in &h.extra_meta {
        let attr = if k.starts_with("og:") || k.starts_with("article:") {
            "property"
        } else {
            "name"
        };
        block.push_str(&format!(
            "<meta {}=\"{}\" content=\"{}\" />\n",
            attr,
            esc_html(k),
            esc_html(v)
        ));
    }
    for ld in &h.jsonld {
        block.push_str(&format!(
            "<script type=\"application/ld+json\">{}</script>\n",
            json_inline(ld)
        ));
    }

    match find_ci(&out, "</head") {
        Some(i) => out.insert_str(i, &block),
        None => out.push_str(&block),
    }

    // ── noscript 正文兜底 ──
    if let Some(ns) = h.noscript.as_ref() {
        if let Some(i) = find_ci(&out, "<body") {
            if let Some(j) = out[i..].find('>') {
                out.insert_str(i + j + 1, ns);
            }
        }
    }
    out
}

// ───────────────────────── 机器入口端点 ─────────────────────────

/// 取当前激活模板 slug（机器端点的站点根需要它）
async fn active_slug(st: &AppState) -> String {
    crate::templates::resolve_active(st).await
}

/// GET /llms.txt —— 低歧义产品说明书（Markdown）。
///
/// 这不是 SEO 黑魔法：它真正的价值是**给 Agent 一份高度压缩、单一事实源的
/// 产品说明**。因此内容必须全部由 DB 派生 —— 手工维护的副本一定会和后台
/// 配置漂移（本项目在预设清单上已经吃过一次手工同步的亏）。
pub async fn llms_markdown(st: &AppState) -> Result<String, String> {
    let slug = active_slug(st).await;
    let site_url = site_url_for_active(&slug);
    let f = facts(st, &site_url).await?;
    Ok(render_llms(&f))
}

/// 纯渲染（便于单测）
pub fn render_llms(f: &Facts) -> String {
    let (total, paid) = f.counts();
    let name = if f.brand_en.is_empty() {
        f.brand.clone()
    } else {
        format!("{} {}", f.brand, f.brand_en)
    };
    let mut o = String::new();
    o.push_str(&format!("# {}\n\n", name));
    if !f.tagline.is_empty() {
        o.push_str(&format!("> {}\n\n", f.tagline));
    }
    o.push_str("## What it is\n\n");
    o.push_str(&format!(
        "{} 是一个中文内容站点，发布面向普通读者的实操指南{}。\n\n",
        name,
        if f.topics.is_empty() {
            String::new()
        } else {
            format!("，主题集中在：{}", f.topics.join("、"))
        }
    ));
    if !f.description.is_empty() {
        o.push_str(&format!("{}\n\n", f.description));
    }

    o.push_str("## Best for\n\n");
    if f.topics.is_empty() {
        o.push_str("- 想了解上述主题实操方法的读者\n");
    } else {
        for t in &f.topics {
            o.push_str(&format!("- 需要「{}」相关资料与对比的读者\n", t));
        }
    }
    o.push('\n');

    o.push_str("## Content\n\n");
    o.push_str(&format!(
        "已发布 {} 篇{}。最近内容：\n\n",
        total,
        if paid > 0 {
            format!("，其中 {} 篇为订阅会员专享（仅标题与摘要公开）", paid)
        } else {
            String::new()
        }
    ));
    for a in f.articles.iter().take(30) {
        o.push_str(&format!(
            "- [{}]({}){}{}\n",
            a.title,
            a.url,
            if a.summary.is_empty() {
                String::new()
            } else {
                format!(" —— {}", a.summary)
            },
            if a.paid { "（会员专享）" } else { "" }
        ));
    }
    o.push('\n');

    o.push_str("## Pricing\n\n");
    if f.tiers.is_empty() {
        o.push_str("当前未配置付费方案，全部内容公开可读。\n\n");
    } else {
        for t in &f.tiers {
            o.push_str(&format!(
                "- {}：月付 {:.2} / 年付 {:.2}\n",
                t.name, t.price_monthly, t.price_yearly
            ));
            for x in &t.features {
                o.push_str(&format!("  - {}\n", x));
            }
        }
        o.push('\n');
    }

    o.push_str("## Machine endpoints\n\n");
    o.push_str(&format!("- Agent manifest: {}/agent.json\n", f.origin));
    o.push_str(&format!("- 站点地图: {}/sitemap.xml\n", f.origin));
    o.push_str(&format!("- 全量文章订阅: {}/rss.xml\n", f.origin));
    o.push_str(&format!("- 主页内容聚合（JSON）: {}/api/public/home\n", f.origin));
    o.push_str(&format!(
        "- 单篇正文（JSON；付费内容返回 402 与预览）: {}/api/public/articles/{{id|slug}}\n",
        f.origin
    ));
    o.push_str(&format!("- 标签列表: {}/api/public/tags\n", f.origin));
    o.push('\n');

    o.push_str("## Limits\n\n");
    o.push_str("- 站点不提供写入 / 发布 / 支付类接口给外部 Agent，全部写操作需站内权限与人工审批。\n");
    o.push_str("- 付费文章正文不对匿名请求开放，也不进入 RSS；标题与摘要可公开引用。\n");
    o.push_str(&format!("- 本文件与 {}/agent.json 由同一份站点数据生成，更新时间为站点数据的更新时间。\n", f.origin));
    o
}

/// GET /agent.json —— Agent Manifest（机器可调用面的显式声明）。
///
/// `capabilities` 只声明**今天真实存在**的端点 —— 声明一个不存在的接口，
/// 会让 Agent 调用失败并把整个站点判为不可信，比不声明更糟。
pub async fn agent_json(st: &AppState) -> Result<Value, String> {
    let slug = active_slug(st).await;
    let site_url = site_url_for_active(&slug);
    let f = facts(st, &site_url).await?;
    Ok(render_agent_json(&f))
}

/// 纯渲染（便于单测）
pub fn render_agent_json(f: &Facts) -> Value {
    let (total, paid) = f.counts();

    let capabilities = json!([
        {
            "name": "read_home",
            "description": "读取主页聚合内容：站点信息、首页区块、导航、页脚、置顶与文章列表",
            "kind": "read",
            "method": "GET",
            "endpoint": format!("{}/api/public/home", f.origin),
            "auth": "none"
        },
        {
            "name": "list_articles",
            "description": "读取已发布文章列表（不含正文）；支持 tag / locale / author / featured / limit 过滤",
            "kind": "read",
            "method": "GET",
            "endpoint": format!("{}/api/public/articles", f.origin),
            "params": ["tag", "locale", "author", "featured", "limit"],
            "auth": "none"
        },
        {
            "name": "read_article",
            "description": "读取单篇文章（含正文）。付费或会员内容返回 HTTP 402 与结构化 preview，不返回正文",
            "kind": "read",
            "method": "GET",
            "endpoint": format!("{}/api/public/articles/{{id|slug}}", f.origin),
            "auth": "none"
        },
        {
            "name": "list_tags",
            "description": "读取标签列表及各自已发布文章数",
            "kind": "read",
            "method": "GET",
            "endpoint": format!("{}/api/public/tags", f.origin),
            "auth": "none"
        },
        {
            "name": "list_plans",
            "description": "读取对外可售的订阅方案；未配置时返回空数组",
            "kind": "read",
            "method": "GET",
            "endpoint": format!("{}/api/public/tiers", f.origin),
            "auth": "none"
        },
        {
            "name": "read_feed",
            "description": "RSS 2.0 订阅源（仅公开内容，付费内容不进入）",
            "kind": "read",
            "method": "GET",
            "endpoint": format!("{}/rss.xml", f.origin),
            "auth": "none"
        }
    ]);

    let mut actions = vec![
        json!({ "name": "home", "description": "站点首页", "url": f.site_url }),
        json!({ "name": "manifest", "description": "本清单", "url": format!("{}/agent.json", f.origin) }),
        json!({ "name": "llms_txt", "description": "面向语言模型的站点说明", "url": format!("{}/llms.txt", f.origin) }),
        json!({ "name": "sitemap", "url": format!("{}/sitemap.xml", f.origin) }),
        json!({ "name": "rss", "url": format!("{}/rss.xml", f.origin) }),
        json!({ "name": "articles", "url": format!("{}/api/public/articles", f.origin) }),
    ];
    for (label, href) in f.nav.iter().chain(f.footer.iter()) {
        actions.push(json!({ "name": label, "url": href }));
    }
    if !f.tiers.is_empty() {
        actions.push(json!({
            "name": "subscribe",
            "description": "订阅付费方案",
            "url": f.site_url
        }));
    }

    let mut limits = vec![
        "站内写操作（发布、编辑、审批、支付）不对外部 Agent 开放，需后台权限与人工审批。",
        "付费文章正文不对匿名请求开放；标题与摘要可公开引用。",
        "不支持跨站写入或代发布；站点不提供 OAuth 提供商能力。",
    ];
    if f.tiers.is_empty() {
        limits.push("当前未配置订阅方案，无付费内容与付费动作可用。");
    }

    json!({
        "specVersion": "0.1",
        "kind": "agent-manifest",
        "generatedFrom": "site_settings + sections + tiers + articles (live DB)",
        "name": f.title,
        "alternateName": f.brand,
        "alternateNameEn": f.brand_en,
        "url": f.site_url,
        "description": f.description,
        "locale": "zh-CN",
        "topics": f.topics,
        "content": {
            "publishedCount": total,
            "gatedCount": paid,
            "articles": f.articles.iter().map(|a| {
                let access = if a.paid { "subscription" } else { "free" };
                json!({
                    "id": a.id,
                    "slug": a.slug,
                    "key": a.key,
                    "title": a.title,
                    "url": a.url,
                    "summary": a.summary,
                    "tags": a.tags,
                    "publishedAt": a.published_at,
                    "access": access
                })
            }).collect::<Vec<_>>(),
        },
        "plans": f.tiers.iter().map(|t| json!({
            "slug": t.slug,
            "name": t.name,
            "description": t.description,
            "price": { "monthly": t.price_monthly, "yearly": t.price_yearly, "currency": "CNY" },
            "features": t.features,
        })).collect::<Vec<_>>(),
        "capabilities": capabilities,
        "actions": actions,
        "limits": limits,
        "sections": f.sections.iter().map(|(s2, t, sub)| json!({
            "slug": s2, "title": t, "subtitle": sub
        })).collect::<Vec<_>>(),
        "generatedAt": crate::db::now_iso(),
    })
}

/// 文本响应（llms.txt / ai/product 共用）：`text/plain` + `charset=utf-8`。
fn text_response(body: String, ct: &'static str) -> axum::response::Response {
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, ct),
            (header::CACHE_CONTROL, "public, max-age=300"),
        ],
        body,
    )
        .into_response()
}

/// GET /llms.txt
pub async fn llms_txt(State(st): State<AppState>) -> axum::response::Response {
    match llms_markdown(&st).await {
        Ok(body) => text_response(body, "text/plain; charset=utf-8"),
        Err(e) => {
            eprintln!("[agent] /llms.txt 生成失败：{e}");
            text_response(
                format!("# 暂时不可用\n\n站点数据读取失败：{e}\n"),
                "text/plain; charset=utf-8",
            )
        }
    }
}

/// GET /ai/product —— 与 /llms.txt 同一份渲染结果，仅 Content-Type 不同。
///
/// 刻意复用同一个函数而不是复制内容：两份手工维护的说明一定会漂移。
pub async fn ai_product(State(st): State<AppState>) -> axum::response::Response {
    match llms_markdown(&st).await {
        Ok(body) => text_response(body, "text/markdown; charset=utf-8"),
        Err(e) => {
            eprintln!("[agent] /ai/product 生成失败：{e}");
            text_response(
                format!("# 暂时不可用\n\n站点数据读取失败：{e}\n"),
                "text/markdown; charset=utf-8",
            )
        }
    }
}

/// GET /agent.json
pub async fn agent_manifest(State(st): State<AppState>) -> axum::response::Response {
    match agent_json(&st).await {
        Ok(v) => (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "application/json; charset=utf-8"),
                (header::CACHE_CONTROL, "public, max-age=300"),
            ],
            v.to_string(),
        )
            .into_response(),
        Err(e) => {
            eprintln!("[agent] /agent.json 生成失败：{e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                [(header::CONTENT_TYPE, "application/json; charset=utf-8")],
                json!({ "ok": false, "error": e }).to_string(),
            )
                .into_response()
        }
    }
}

// ───────────────────────── 注入入口（供 templates.rs 调用）─────────────────────────

/// 为某个模板的某个页面产出头注。返回 `None` 表示「无需注入」（非 HTML 入口、
/// 站点不存在、DB 异常 —— 后两者都应让页面照常打开，只打日志）。
///
/// `rest` 是模板目录内的相对路径（`""` 表示入口 index.html，`post/<key>` 表示
/// 文章深链）。识别不出路由时按站点首页处理。
pub async fn head_for(st: &AppState, slug: &str, rest: &str) -> Option<HeadInjection> {
    let site_url = site_url_for_slug(slug);
    let f = match facts(st, &site_url).await {
        Ok(f) => f,
        Err(e) => {
            eprintln!("[agent] 采集站点事实失败（{slug}）：{e}");
            return None;
        }
    };
    let rel = rest.trim_matches('/');
    if let Some(key) = article_key_from_path(rel) {
        match article_full(st, site_url.as_str(), &key).await {
            Ok(Some(a)) => return Some(build_article_head(&f, &a)),
            Ok(None) => {
                // 文章不存在（可能是前端路由的其它深链）→ 退回站点头注，
                // 总比让爬虫拿到空壳好。
                return Some(build_site_head(&f));
            }
            Err(e) => {
                eprintln!("[agent] 查询文章失败（{key}）：{e}");
                return Some(build_site_head(&f));
            }
        }
    }
    Some(build_site_head(&f))
}

/// 从模板内相对路径里提取文章 key：`post/<key>`、`read/<key>`、`articles/<key>`。
///
/// 只认单段 key，避免把 `/post/a/b` 这类误当文章；额外拒绝空段与 `..`。
fn article_key_from_path(rel: &str) -> Option<String> {
    let mut it = rel.split('/');
    let head = it.next()?;
    if !matches!(head, "post" | "read" | "articles") {
        return None;
    }
    let key = it.next()?;
    if key.is_empty() || it.next().is_some() {
        return None;
    }
    Some(key.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts_fixture() -> Facts {
        Facts {
            site_url: "https://example.com".into(),
            origin: "https://api.example.com".into(),
            brand: "可可鸭".into(),
            brand_en: "CouCouYa".into(),
            title: "可可鸭 KeKeYa".into(),
            tagline: "把连接，变成看得见的机会。".into(),
            description: "围绕 AI 工具、Web3 与跨境金融的实操指南。".into(),
            topics: vec!["支付方式".into(), "外币卡".into()],
            sections: vec![("about".into(), "关于".into(), String::new())],
            nav: vec![("文章".into(), "#writing".into())],
            footer: vec![("X".into(), "https://x.com/KeKeYa88".into())],
            tiers: Vec::new(),
            articles: vec![ArticleBrief {
                id: "a1".into(),
                slug: "u-card".into(),
                key: "u-card".into(),
                url: "https://example.com/post/u-card".into(),
                title: "U 卡横评".into(),
                summary: "6 张主流 U 卡对比。".into(),
                published_at: "2026-09-01T00:00:00Z".into(),
                updated_at: "2026-09-02T00:00:00Z".into(),
                tags: vec!["U 卡".into()],
                paid: false,
            }],
        }
    }

    fn art(paid: bool) -> ArticleFull {
        ArticleFull {
            brief: ArticleBrief {
                id: "b1".into(),
                slug: "paid-post".into(),
                key: "paid-post".into(),
                url: "https://example.com/post/paid-post".into(),
                title: "会员专享：港卡开户".into(),
                summary: "开户门槛与审核速度对比。".into(),
                published_at: "2026-09-01T00:00:00Z".into(),
                updated_at: "2026-09-02T00:00:00Z".into(),
                tags: vec!["港卡".into()],
                paid,
            },
            content: "<p>正文第一段 SECRET_BODY。</p><script>var x=1</script>".into(),
            meta_title: String::new(),
            meta_description: String::new(),
            canonical_url: String::new(),
            featured_image: String::new(),
            author: "可可鸭".into(),
        }
    }

    /// 空壳（与真实 coucouya 产物同构）注入后必须带上机器可读信息
    #[test]
    fn injects_into_csr_shell() {
        let html = "<!doctype html><html lang=\"zh-CN\"><head><title>旧标题</title>\
                    <meta name=\"description\" content=\"旧描述\" />\
                    <script type=\"module\" src=\"/t/x/assets/a.js\"></script></head>\
                    <body><div id=\"root\"></div></body></html>";
        let f = facts_fixture();
        let out = inject_head(html, &build_site_head(&f));

        assert!(out.contains(MARKER));
        assert!(out.contains("rel=\"canonical\""));
        assert!(out.contains("og:type"));
        assert!(out.contains("application/ld+json"));
        assert!(out.contains("FAQPage"));
        assert!(out.contains("<noscript>"));
        assert!(out.contains("U 卡横评"));
        // 站点根保留模板自己写的 title / description（那是"对人"的门面文案），
        // 但 og:* 必须与它们**同源**：每个值恰好出现 2 次
        // （<title> + og:title / meta description + og:description）。
        assert_eq!(out.matches("name=\"description\"").count(), 1);
        // description 共 3 处：meta description / og:description / twitter:description（三处同源）
        assert_eq!(
            out.matches("旧描述").count(),
            3,
            "og/twitter description 应与 meta description 同文本"
        );
        assert!(out.contains("<title>旧标题</title>"));
        // title 共 3 处：<title> / og:title / twitter:title
        assert_eq!(out.matches("旧标题").count(), 3, "og/twitter title 应与 <title> 同文本");
        // Twitter 卡必须显式给出（X 对 og 的回落不可靠）
        assert!(out.contains("twitter:card"));
        assert!(out.contains("twitter:title"));
        assert!(out.contains("twitter:description"));
        assert!(out.contains("twitter:url"));
        // 没有封面时不能用 summary_large_image —— 抓取器会渲染成无图大卡，比 summary 更糟
        assert!(out.contains("content=\"summary\""), "无封面时应给 summary");
        assert!(!out.contains("summary_large_image"), "无封面时不该声明大卡");
        // 幂等
        assert_eq!(inject_head(&out, &build_site_head(&f)), out);
    }

    /// 有封面时 Twitter 卡升级为大卡，并与 og:image 一致
    #[test]
    fn twitter_card_upgrades_with_image() {
        let f = facts_fixture();
        let html = "<html><head><title>t</title></head><body></body></html>";
        let mut h = build_site_head(&f);
        h.image = Some("https://example.com/cover.png".into());
        let out = inject_head(html, &h);
        assert!(out.contains("content=\"summary_large_image\""));
        assert!(out.contains("twitter:image"));
        assert!(out.contains("og:image"));
    }

    /// 站点根：模板缺 <title> 时用 fallback 补上
    #[test]
    fn fallback_title_when_missing() {
        let html = "<html><head></head><body></body></html>";
        let f = facts_fixture();
        let out = inject_head(html, &build_site_head(&f));
        assert!(out.contains("<title>可可鸭 KeKeYa · 把连接，变成看得见的机会。</title>"));
    }

    /// 付费文章：正文绝不出现在注入结果里
    #[test]
    fn gated_article_never_leaks_body() {
        let f = facts_fixture();
        let html = "<html><head><title>t</title></head><body><div id=\"root\"></div></body></html>";
        let out = inject_head(html, &build_article_head(&f, &art(true)));
        assert!(!out.contains("SECRET_BODY"), "付费正文泄漏进了头注");
        assert!(out.contains("会员专享"));
        assert!(out.contains("\"isAccessibleForFree\":false"));
    }

    /// 免费文章：正文进 noscript 与 articleBody
    #[test]
    fn free_article_includes_body() {
        let f = facts_fixture();
        let html = "<html><head><title>t</title></head><body><div id=\"root\"></div></body></html>";
        let out = inject_head(html, &build_article_head(&f, &art(false)));
        assert!(out.contains("SECRET_BODY"));
        assert!(out.contains("articleBody"));
        assert!(out.contains("\"isAccessibleForFree\":true"));
        // script 块内容不得混进正文
        assert!(!out.contains("var x=1"));
    }

    /// JSON-LD 里的 `</script>` 必须被转义，否则会提前终止脚本块
    #[test]
    fn jsonld_escapes_script_close() {
        let html = "<html><head></head><body></body></html>";
        let mut h = build_site_head(&facts_fixture());
        h.jsonld = vec!["{\"a\":\"</script><b>\"}".to_string()];
        let out = inject_head(html, &h);
        assert!(!out.contains("</script><b>"));
        assert!(out.contains("\\u003c/script\\u003e"));
    }

    /// 正文提取：去标签、剔 script、解实体、超长截断
    #[test]
    fn text_extraction() {
        let t = html_to_text("<p>你好 &amp; 世界</p><script>bad()</script><img src=\"data:image/png;base64,AAAA\">", 100);
        assert_eq!(t, "你好 & 世界");
        let long = format!("<p>{}</p>", "字".repeat(500));
        let cut = html_to_text(&long, 10);
        assert_eq!(cut.chars().count(), 11); // 10 字 + 省略号
    }

    /// tags 解析：纯文本 / 逗号 / JSON 空数组
    #[test]
    fn tag_parsing() {
        assert_eq!(parse_tags("支付方式"), vec!["支付方式"]);
        assert_eq!(parse_tags("a, b，c"), vec!["a", "b", "c"]);
        assert!(parse_tags("[]").is_empty());
        assert!(parse_tags("  ").is_empty());
    }

    /// 付费门槛口径与 public_api.rs 一致
    #[test]
    fn gate_detection() {
        assert!(!is_gated("public", 0));
        assert!(is_gated("public", 1));
        assert!(is_gated("paid", 0));
        assert!(is_gated("members", 0));
    }

    /// 深链识别：只认 post/read/articles 的单段 key
    #[test]
    fn article_path_detection() {
        assert_eq!(article_key_from_path("post/u-card").as_deref(), Some("u-card"));
        assert_eq!(article_key_from_path("read/ai-2.0").as_deref(), Some("ai-2.0"));
        assert!(article_key_from_path("").is_none());
        assert!(article_key_from_path("post/a/b").is_none());
        assert!(article_key_from_path("post/").is_none());
        assert!(article_key_from_path("about").is_none());
    }

    /// agent.json / llms.txt 必须自洽：无套餐时不编造价格
    #[test]
    fn manifest_without_tiers() {
        let f = facts_fixture();
        let v = render_agent_json(&f);
        assert_eq!(v["plans"].as_array().unwrap().len(), 0);
        assert_eq!(v["content"]["publishedCount"], 1);
        let md = render_llms(&f);
        assert!(md.contains("当前未配置付费方案"));
        assert!(md.contains("u-card"));
    }
}
