//! Smart Links（P0-4，对标 FluentCRM / Bitly 的「点链接自动打标签」）。
//!
//! 做什么：后台建一条短链 `{token} → 目标 URL`，我们只对外发 `/go/{token}`。
//! 有人点开时 ——
//!   ① 302 跳到目标（功能）；
//!   ② 记一次点击（观测）；
//!   ③ 若访问者带着会员令牌，把链接上配置的标签**自动打到这个人身上**（动作）。
//! 第 ③ 步才是这条链路真正的价值：它把「我发过一条链接」变成
//! 「谁对我的链接有兴趣」——在私有化、没有第三方分析的站点上，
//! **这是唯一能自己拿到的行为信号**，也是往后做分群触达的前提。
//!
//! 为什么不做成 `/r/{token}` 之类的内部重定向：短链本质是「外部世界进来的入口」，
//! 必须放在**服务端根路径**且不带任何前缀 —— 它要能贴进邮件、微信群、短信，
//! 那些地方没有我们的 base 概念。
//!
//! 两个必须守住的细节：
//! - **预取过滤**：邮件网关、IM 的链接预览会先把链接抓一遍。若不过滤，
//!   一条群发邮件能在真人还没看到时就把点击数刷到几十，数据立刻失去意义。
//!   我们**照旧跳转**（跳错比不跳伤害大），只是不计入真人点击。
//! - **`Cache-Control: no-store`**：302 是可缓存的，一旦被中间层缓存，
//!   后续点击根本不回到我们这里 —— 计数会静默停在第一个人的数字上。

use axum::extract::{Path, State};
use axum::http::header::{HeaderValue, CACHE_CONTROL, LOCATION, USER_AGENT};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use sea_orm::{DatabaseBackend, Statement, Value as SqlValue};

use crate::comments::OptionalMember;
use crate::state::AppState;

/// 单租户硬编码（与 `db.rs` 种子、`resources::create`、`seo_redirect` 口径一致）。
const TENANT: &str = "t_demo";

const MAX_TOKEN_LEN: usize = 64;
const MAX_UA_LEN: usize = 200;
const MAX_REFERER_LEN: usize = 300;
/// 一条链接最多打几个标签 —— 上限的作用是防呆：一次点开打 50 个标签，
/// 会员档案会被噪声淹没，标签也就失去了筛选价值。
const MAX_TAGS: usize = 10;
const MAX_TAG_LEN: usize = 24;

fn sval(s: String) -> SqlValue {
    SqlValue::String(Some(s))
}

fn stmt(sql: &str, args: Vec<SqlValue>) -> Statement {
    Statement::from_sql_and_values(DatabaseBackend::Sqlite, sql.to_string(), args)
}

// ─────────────────────────── 纯函数 ───────────────────────────

/// token 合法性：只允许 `[A-Za-z0-9_-]`，长度 1..=64。
///
/// 收紧字符集不是为了安全（token 不携带权限），而是为了**避免歧义**：
/// 带 `%2F`、空格、非 ASCII 的 token 在邮件客户端、IM、二维码里
/// 会被不同程度地改写或截断，发出去就找不回来了。
pub fn valid_token(t: &str) -> bool {
    !t.is_empty()
        && t.len() <= MAX_TOKEN_LEN
        && t.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// 把逗号分隔的标签串规整成去重列表：去空白、丢空项、按字符截断、保持原序。
pub fn parse_tags(csv: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in csv.split([',', '，']) {
        let t: String = raw.trim().chars().take(MAX_TAG_LEN).collect();
        if t.is_empty() || out.contains(&t) {
            continue;
        }
        out.push(t);
        if out.len() >= MAX_TAGS {
            break;
        }
    }
    out
}

/// UA 里确定性属于「机器预取 / 爬虫」的关键词。
///
/// **刻意不包含 `headless`**：我们的端到端验证本身就跑在 headless Chromium 上，
/// 把它列进黑名单会让真实点击在自测里也统计不到（验证手段把被测行为屏蔽掉，
/// 是最容易蒙混过关的一种错）。判真人主要靠下面的 Fetch Metadata。
const BOT_UA: &[&str] = &[
    "bot", "spider", "crawl", "facebookexternalhit", "whatsapp", "telegram",
    "slack", "discord", "linkedin", "twitter", "skype", "preview",
    "curl/", "wget", "python-requests", "go-http-client", "okhttp",
    "pingdom", "uptimerobot", "monitor", "scanner",
];

/// 判定这次请求是不是「真人点开链接」。
///
/// 判定失败只会让点击数少记，**绝不改变跳转行为** —— 观测可以缺，
/// 功能不能缺。三道判据从硬到软：
/// ① `Purpose: prefetch` 一类头 —— 现代浏览器/网关的显式预取声明，最硬；
/// ② `Sec-Fetch-Mode` —— 真人点链接必然是 `navigate`，脚本请求是 `cors`/`no-cors`；
///    （该头由浏览器自己写、页面改不了，所以可信）
/// ③ UA 黑名单 —— 兜住不认识 Fetch Metadata 的老式邮件网关。
pub fn looks_like_click(headers: &HeaderMap) -> bool {
    for h in ["purpose", "x-purpose", "sec-purpose", "x-moz"] {
        if let Some(v) = headers.get(h).and_then(|v| v.to_str().ok()) {
            let v = v.to_ascii_lowercase();
            if v.contains("prefetch") || v.contains("prerender") || v.contains("preview") {
                return false;
            }
        }
    }
    if let Some(m) = headers.get("sec-fetch-mode").and_then(|v| v.to_str().ok()) {
        if !m.eq_ignore_ascii_case("navigate") {
            return false;
        }
    }
    let ua = headers
        .get(USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    !BOT_UA.iter().any(|k| ua.contains(k))
}

/// 目标 URL 白名单：只允许 http/https。
///
/// `Location` 头一旦能塞进 `javascript:` / `data:`，就是一条**由后台配置触发的
/// 开放重定向 + XSS 面**。配置者是管理员不代表这类能力该留着 ——
/// 与 `seo_redirect::redirect_response` 同一套口径。
pub fn go_response(url: &str) -> Response {
    let u = url.trim();
    if !(u.starts_with("http://") || u.starts_with("https://")) {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    match HeaderValue::from_str(u) {
        Ok(v) => (
            StatusCode::FOUND,
            [
                (LOCATION, v),
                // 必须禁缓存：302 一旦被中间层缓存，后续点击不再回到我们这里，
                // 计数会停在第一个人的数字上，而跳转照旧成功 —— 最难发现的那种坏。
                (
                    CACHE_CONTROL,
                    HeaderValue::from_static("no-store, no-cache, must-revalidate"),
                ),
            ],
        )
            .into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

fn gone() -> Response {
    (
        StatusCode::NOT_FOUND,
        [(CACHE_CONTROL, HeaderValue::from_static("no-store"))],
        "link not found",
    )
        .into_response()
}

// ─────────────────────────── 数据访问 ───────────────────────────

struct Hit {
    id: String,
    url: String,
    tags: String,
}

async fn load(st: &AppState, token: &str) -> Option<Hit> {
    let r = st
        .db
        .query_one(
            "SELECT id, url, tags FROM smart_links
              WHERE tenant_id = ? AND token = ? AND enabled = 1 LIMIT 1",
            vec![sval(TENANT.to_string()), sval(token.to_string())],
        )
        .await
        .ok()
        .flatten()?;
    Some(Hit {
        id: r.try_get::<String>("", "id").unwrap_or_default(),
        url: r.try_get::<String>("", "url").unwrap_or_default(),
        tags: r.try_get::<String>("", "tags").unwrap_or_default(),
    })
}

/// 记一次链接访问。`human=false` 只累加预取计数，不落明细行 ——
/// 预取量级可以是真人的几十倍，落明细会把表撑成噪声。
async fn record_click(
    st: &AppState,
    hit: &Hit,
    member_id: &str,
    referer: &str,
    ua: &str,
    human: bool,
) {
    let now = crate::db::now_iso();
    if !human {
        let _ = st
            .db
            .execute_statement(stmt(
                "UPDATE smart_links SET prefetch = prefetch + 1 WHERE tenant_id = ? AND id = ?",
                vec![sval(TENANT.to_string()), sval(hit.id.clone())],
            ))
            .await;
        return;
    }
    // 先加计数再落明细：计数是列表页要用的，明细是排查用的，前者优先。
    let _ = st
        .db
        .execute_statement(stmt(
            "UPDATE smart_links SET clicks = clicks + 1, last_click_at = ?
              WHERE tenant_id = ? AND id = ?",
            vec![sval(now.clone()), sval(TENANT.to_string()), sval(hit.id.clone())],
        ))
        .await;
    let _ = st
        .db
        .execute_statement(stmt(
            "INSERT INTO link_clicks
                (id, tenant_id, link_id, member_id, referer, ua, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            vec![
                sval(uuid::Uuid::new_v4().to_string()),
                sval(TENANT.to_string()),
                sval(hit.id.clone()),
                sval(member_id.to_string()),
                sval(referer.chars().take(MAX_REFERER_LEN).collect()),
                sval(ua.chars().take(MAX_UA_LEN).collect()),
                sval(now.clone()),
                sval(now),
            ],
        ))
        .await;
}

/// 把链接上的标签打到会员身上（幂等）。
///
/// 用「直接 INSERT、忽略结果」而不是先查后插：唯一索引
/// `(tenant_id, member_id, tag)` 已经保证不会重复，而两条后端的**报错方式不同**
/// （Turso 把唯一键冲突吞成 `Ok(0)`，本地 SQLite 返回 `Err`）——
/// 这里两种结果都无差别，索性不看返回值。这与 `seo_redirect::record_not_found`
/// 的取舍不同：那里的 UPDATE/INSERT 分支决定了「新增还是累加」，必须判 `rows_affected`。
pub async fn apply_tags(st: &AppState, member_id: &str, tags_csv: &str) {
    if member_id.is_empty() {
        return;
    }
    let now = crate::db::now_iso();
    for tag in parse_tags(tags_csv) {
        let _ = st
            .db
            .execute_statement(stmt(
                "INSERT INTO member_tags (id, tenant_id, member_id, tag, source, created_at, updated_at)
                 VALUES (?, ?, ?, ?, 'smart_link', ?, ?)",
                vec![
                    sval(uuid::Uuid::new_v4().to_string()),
                    sval(TENANT.to_string()),
                    sval(member_id.to_string()),
                    sval(tag),
                    sval(now.clone()),
                    sval(now.clone()),
                ],
            ))
            .await;
    }
}

/// 某个会员已有哪些标签（供会员 360° 档案时间线展示）。
pub async fn member_tags(st: &AppState, member_id: &str) -> Vec<String> {
    let rows = st
        .db
        .query_all(
            "SELECT tag FROM member_tags WHERE tenant_id = ? AND member_id = ? ORDER BY tag ASC",
            vec![sval(TENANT.to_string()), sval(member_id.to_string())],
        )
        .await
        .unwrap_or_default();
    rows.iter()
        .filter_map(|r| r.try_get::<String>("", "tag").ok())
        .collect()
}

// ─────────────────────────── 端点 ───────────────────────────

/// GET /go/{token} —— 公开短链入口。
///
/// 无鉴权是**要求**而非妥协：这条链接就是要发给未登录的人。
/// 三道输入边界：token 字符集+长度、referer/ua 截断、目标 URL 协议白名单。
pub async fn go(
    State(st): State<AppState>,
    om: OptionalMember,
    Path(token): Path<String>,
    headers: HeaderMap,
) -> Response {
    let tok = token.trim().to_ascii_lowercase();
    if !valid_token(&tok) {
        return gone();
    }
    let Some(hit) = load(&st, &tok).await else {
        return gone();
    };
    let member_id = om.0.as_ref().map(|c| c.sub.clone()).unwrap_or_default();
    let referer = headers
        .get(axum::http::header::REFERER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let ua = headers
        .get(USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let human = looks_like_click(&headers);

    // 记录与打标签都**不能**影响跳转：目标已经确定，剩下的是观测与副作用。
    record_click(&st, &hit, &member_id, &referer, &ua, human).await;
    if human {
        apply_tags(&st, &member_id, &hit.tags).await;
    }
    go_response(&hit.url)
}

#[cfg(test)]
mod tests {
    use super::{go_response, looks_like_click, parse_tags, valid_token};
    use axum::http::{HeaderMap, HeaderValue, StatusCode};

    fn hm(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.insert(
                axum::http::HeaderName::from_bytes(k.as_bytes()).unwrap(),
                HeaderValue::from_str(v).unwrap(),
            );
        }
        h
    }

    #[test]
    fn token_charset_is_tight() {
        assert!(valid_token("ab12_-"));
        assert!(!valid_token(""));
        assert!(!valid_token("a b"));
        assert!(!valid_token("a/b"));
        assert!(!valid_token("中文"));
        assert!(!valid_token(&"x".repeat(65)));
    }

    #[test]
    fn tags_are_cleaned_and_capped() {
        assert_eq!(parse_tags(" vip , 高意向 ,, vip "), vec!["vip", "高意向"]);
        let many = (0..20).map(|i| format!("t{i}")).collect::<Vec<_>>().join(",");
        assert_eq!(parse_tags(&many).len(), 10);
        // 中文逗号也要认 —— 后台是人在填，不该因为输入法被吞标签
        assert_eq!(parse_tags("a，b"), vec!["a", "b"]);
    }

    /// 预取必须被识别：否则一封群发邮件就能在无人点开时把计数刷高。
    #[test]
    fn prefetch_and_bots_are_not_clicks() {
        assert!(!looks_like_click(&hm(&[("purpose", "prefetch")])));
        assert!(!looks_like_click(&hm(&[("sec-purpose", "prefetch;prerender")])));
        assert!(!looks_like_click(&hm(&[("sec-fetch-mode", "no-cors")])));
        assert!(!looks_like_click(&hm(&[("user-agent", "Mozilla/5.0 facebookexternalhit/1.1")])));
        assert!(!looks_like_click(&hm(&[("user-agent", "curl/8.0")])));
        // 真人导航
        assert!(looks_like_click(&hm(&[
            ("sec-fetch-mode", "navigate"),
            ("user-agent", "Mozilla/5.0 (iPhone) Safari/604.1"),
        ])));
        // 老客户端不带 Fetch Metadata，也不该被误杀（不识别即放行）
        assert!(looks_like_click(&hm(&[("user-agent", "Mozilla/4.0 (compatible)")])));
    }

    /// 302 必须禁缓存：被缓存后点击不再回来，计数静默失效而跳转照旧成功。
    #[test]
    fn redirect_is_uncacheable_and_protocol_whitelisted() {
        let r = go_response("https://example.com/a");
        assert_eq!(r.status(), StatusCode::FOUND);
        assert_eq!(
            r.headers().get("cache-control").unwrap().to_str().unwrap(),
            "no-store, no-cache, must-revalidate"
        );
        assert_eq!(
            r.headers().get("location").unwrap().to_str().unwrap(),
            "https://example.com/a"
        );
        assert_eq!(go_response("javascript:alert(1)").status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(go_response("/relative").status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
