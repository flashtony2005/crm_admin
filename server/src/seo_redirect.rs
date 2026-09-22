//! 站点级重定向与 404 监控（P0-1，对标 Rank Math 免费版的「重定向管理 + 404 监控」）。
//!
//! 为什么必须做：我们正在**主动制造 URL 变动** —— 迁移 0011 加了 `kind` 列，
//! 文章可以在 `/post/<key>` 与 `/problems/<key>` 之间迁移；问题页体系正在铺开；
//! sitemap 刚从 7 条修到 12 条。任何一次改 slug、换 kind，旧地址就是 404，
//! 而这类流失**既不报警也查不到** —— 是持续性、静默的 SEO 与 GEO 资产流失。
//!
//! 两条链路：
//!   ① **出** —— `lookup()` 在 `templates::serve_file` 的 SPA 兜底**之前**查重定向，
//!      命中即 301/302/307/308/410。放在服务端而不是前端路由，是因为
//!      **爬虫不执行 JS**：前端路由层的重定向对搜索引擎与 AI 爬虫等于不存在。
//!   ② **入** —— `record_not_found()` 记录没找到的地址，后台可「一键转 301」。
//!      我们是 CSR（服务端对任意导航路径都回 index.html），**服务端根本
//!      收不到公开站的 404** —— 所以由前端在渲染「未找到」兜底态时主动上报。
//!
//! 路径口径：统一存**完整请求路径**（含 `/t/<slug>` 前缀）。理由：前端上报的就是
//! `location.pathname`，零转换；后台「一键转 301」直接把日志里的 path 落成
//! `from_path`，天然与匹配逻辑一致。匹配时先试带前缀、未命中再试剥掉前缀的
//! 站点相对路径 —— 覆盖将来用反代把 `/` 直接映射到模板目录的部署形态。

use axum::extract::State;
use axum::http::header::{HeaderValue, LOCATION, USER_AGENT};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use sea_orm::{DatabaseBackend, Statement, Value as SqlValue};
use serde_json::{json, Value};

use crate::error::{ok, ApiError, ApiResult};
use crate::state::AppState;

/// 单租户硬编码（与 `db.rs` 的种子、`resources::create` 的 tenant 口径一致）。
const TENANT: &str = "t_demo";

/// path 上限 —— 公开端点无鉴权，必须给输入一个硬边界，
/// 否则一条超长 path 就能撑大存储、污染后台列表。
const MAX_PATH_LEN: usize = 512;
const MAX_UA_LEN: usize = 200;
const MAX_REFERER_LEN: usize = 300;

fn sval(s: String) -> SqlValue {
    SqlValue::String(Some(s))
}

fn stmt(sql: &str, args: Vec<SqlValue>) -> Statement {
    Statement::from_sql_and_values(DatabaseBackend::Sqlite, sql.to_string(), args)
}

// ─────────────────────────── 路径规整 ───────────────────────────

/// 归一化：去空白、去查询串与锚点、补前导 `/`、去尾部 `/`（根路径除外）。
fn normalize(p: &str) -> String {
    let p = p.trim();
    let p = p.split(['?', '#']).next().unwrap_or("");
    let mut s = if p.starts_with('/') {
        p.to_string()
    } else {
        format!("/{p}")
    };
    while s.len() > 1 && s.ends_with('/') {
        s.pop();
    }
    s
}

/// 剥掉 `/t/<slug>` 前缀，得到站点相对路径。
/// `/t/coucouya/post/x` → `/post/x`；`/t/coucouya` → `/`；非 `/t/` 开头 → None。
fn strip_t_prefix(p: &str) -> Option<String> {
    let rest = p.strip_prefix("/t/")?;
    let mut it = rest.splitn(2, '/');
    it.next()?; // slug 本身丢弃
    match it.next() {
        Some(tail) if !tail.is_empty() => Some(format!("/{tail}")),
        _ => Some("/".to_string()),
    }
}

/// 候选匹配键：完整路径优先，其次剥掉 `/t/<slug>` 的站点相对路径。
fn candidate_keys(path: &str) -> Vec<String> {
    let p = normalize(path);
    let mut v = vec![p.clone()];
    if let Some(rel) = strip_t_prefix(&p) {
        if rel != p {
            v.push(rel);
        }
    }
    v
}

// ─────────────────────────── ① 出：查重定向 ───────────────────────────

/// 查一次重定向。命中返回 `(目标, 状态码)`，未命中返回 None。
///
/// 命中后异步累加 `hits` —— 这是「这条规则到底有没有在救流量」的唯一证据，
/// 失败不影响跳转本身（计数是观测，不是功能）。
pub async fn lookup(st: &AppState, path: &str) -> Option<(String, i32)> {
    for key in candidate_keys(path) {
        let row = st
            .db
            .query_one(
                "SELECT to_path, code FROM redirects
                  WHERE tenant_id = ? AND from_path = ? AND enabled = 1 LIMIT 1",
                vec![sval(TENANT.to_string()), sval(key.clone())],
            )
            .await
            .ok()
            .flatten();
        let Some(r) = row else { continue };
        let to = r.try_get::<String>("", "to_path").unwrap_or_default();
        if to.trim().is_empty() {
            continue;
        }
        // CmsDb 的 `FromDbVal` 只实现了 i64/f64/String 等，没有 i32 ——
        // 用 i32 会编译期报 E0277（try_get 的 trait bound 不满足）。
        let code = r.try_get::<i64>("", "code").unwrap_or(301) as i32;
        bump_hits(st, &key).await;
        return Some((to, code));
    }
    None
}

/// 只加计数，不动 `updated_at` —— 后者是「规则被谁改过」的痕迹，
/// 被访问量刷成当前时间会让审计失真。
async fn bump_hits(st: &AppState, from_path: &str) {
    let _ = st
        .db
        .execute_statement(stmt(
            "UPDATE redirects SET hits = hits + 1 WHERE tenant_id = ? AND from_path = ?",
            vec![sval(TENANT.to_string()), sval(from_path.to_string())],
        ))
        .await;
}

/// 构造重定向响应。
///
/// 目标口径与 `candidate_keys` 对偶：
/// - `http(s)://…` 绝对地址 → 原样跳（外链迁移、站点合并）；
/// - `/xxx` 站点相对路径 → 拼回当前模板前缀 `/t/<slug>/xxx`
///   （因为前端产物按 `base=/t/<slug>/` 构建，只认自己的规范路径）；
/// - 其它（`javascript:`、`data:` 等）→ 500。
///
/// 之所以要白名单协议：`Location` 头一旦能塞进 `javascript:`，就是一个
/// 由后台配置触发的 XSS 面 —— 虽然配置者是管理员，但「不该有的能力」
/// 不该因为「自己人用」就留着。
pub fn redirect_response(to: &str, code: i32, slug: &str) -> Response {
    // 410 Gone：内容永久移除，**不带 Location**（告诉爬虫别再来了）。
    if code == 410 {
        return StatusCode::GONE.into_response();
    }
    let to = to.trim();
    let loc = if to.starts_with("http://") || to.starts_with("https://") {
        to.to_string()
    } else if to.starts_with('/') {
        let rel = to.trim_end_matches('/');
        if rel.is_empty() {
            format!("/t/{slug}")
        } else {
            format!("/t/{slug}{rel}")
        }
    } else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let status = match code {
        302 => StatusCode::FOUND,
        307 => StatusCode::TEMPORARY_REDIRECT,
        308 => StatusCode::PERMANENT_REDIRECT,
        _ => StatusCode::MOVED_PERMANENTLY,
    };
    match HeaderValue::from_str(&loc) {
        Ok(v) => (status, [(LOCATION, v)]).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

// ─────────────────────────── ② 入：404 上报 ───────────────────────────

/// 记一次「访问了不存在的地址」。
///
/// 用「先 UPDATE 再 INSERT」而不是 `INSERT … ON CONFLICT`：
/// Turso 后端把唯一键冲突吞成 `Ok(0)` 而非 `Err`（见 `TOPIC_BILLING.md` 的
/// 幂等硬规则），依赖 `ON CONFLICT` 的写法在两条后端上行为不一致。
/// 这里用 `rows_affected == 0` 判「还没有这行」，两条后端语义一致。
pub async fn record_not_found(
    st: &AppState,
    path: &str,
    referer: &str,
    ua: &str,
) -> Result<(), ApiError> {
    let p = normalize(path);
    if p.len() > MAX_PATH_LEN {
        return Err(ApiError::bad(format!("path 过长（上限 {MAX_PATH_LEN}）")));
    }
    let now = crate::db::now_iso();
    let referer: String = referer.chars().take(MAX_REFERER_LEN).collect();
    let ua: String = ua.chars().take(MAX_UA_LEN).collect();

    let n = st
        .db
        .execute_statement(stmt(
            "UPDATE not_found_log SET hits = hits + 1, last_seen = ?, referer = ?, ua = ?
              WHERE tenant_id = ? AND path = ?",
            vec![
                sval(now.clone()),
                sval(referer.clone()),
                sval(ua.clone()),
                sval(TENANT.to_string()),
                sval(p.clone()),
            ],
        ))
        .await
        .map_err(|e| ApiError::bad(format!("记录 404 失败：{e}")))?;
    if n > 0 {
        return Ok(());
    }

    let id = uuid::Uuid::new_v4().to_string();
    // 并发下两个请求可能同时走到这里，其中一个会撞唯一索引 ——
    // 忽略即可：两边只是计数，不存在「必须成功」的语义。
    let _ = st
        .db
        .execute_statement(stmt(
            "INSERT INTO not_found_log
                (id, tenant_id, path, referer, ua, hits, last_seen, resolved, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, 1, ?, 0, ?, ?)",
            vec![
                sval(id),
                sval(TENANT.to_string()),
                sval(p),
                sval(referer),
                sval(ua),
                sval(now.clone()),
                sval(now.clone()),
                sval(now),
            ],
        ))
        .await;
    Ok(())
}

/// POST /api/public/not-found —— 公开端点，前端在「未找到」兜底态时上报。
///
/// 无鉴权是必要的：匿名访客（也包括爬虫）才是 404 的主要产生者。
/// 代价是这个端点可被刷，所以三道约束：path 长度上限、字段截断、
/// 同路径只累加 hits 不新增行（存储增长是「不同死链条数」级别，可控）。
pub async fn not_found_report(
    State(st): State<AppState>,
    headers: HeaderMap,
    axum::Json(body): axum::Json<Value>,
) -> ApiResult {
    let path = body
        .get("path")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if path.trim().is_empty() {
        return Err(ApiError::bad("path 不能为空"));
    }
    let referer = body
        .get("referer")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let ua = headers
        .get(USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    record_not_found(&st, &path, &referer, &ua).await?;
    ok(json!({ "recorded": true }))
}

#[cfg(test)]
mod tests {
    use super::{candidate_keys, normalize, strip_t_prefix};

    /// 尾部斜杠与查询串必须归一 —— 否则 `/post/a/`、`/post/a?utm=x`
    /// 会被当成三条不同的死链，后台列表很快被同一条链接刷满。
    #[test]
    fn normalize_collapses_variants() {
        assert_eq!(normalize("/post/a/"), "/post/a");
        assert_eq!(normalize("/post/a?utm_source=x"), "/post/a");
        assert_eq!(normalize("post/a#frag"), "/post/a");
        assert_eq!(normalize("/"), "/");
        // 根路径不能被去成空串
        assert_eq!(normalize("///"), "/");
    }

    #[test]
    fn strip_prefix_only_for_t_paths() {
        assert_eq!(strip_t_prefix("/t/coucouya/post/a").as_deref(), Some("/post/a"));
        assert_eq!(strip_t_prefix("/t/coucouya").as_deref(), Some("/"));
        assert_eq!(strip_t_prefix("/post/a"), None);
        // 顶层 `t` 目录不算前缀（避免把 /tags/x 误剥）
        assert_eq!(strip_t_prefix("/tags/x"), None);
        // 空 slug 的 `/t//x` 也不应剥出怪异结果
        assert_eq!(strip_t_prefix("/t/").as_deref(), Some("/"));
    }

    /// 两条候选键的顺序很重要：**先完整路径，后相对路径**。
    /// 反过来会让「同名相对路径」抢走更精确的完整路径规则。
    #[test]
    fn candidate_order_prefers_full_path() {
        assert_eq!(
            candidate_keys("/t/coucouya/post/old"),
            vec!["/t/coucouya/post/old".to_string(), "/post/old".to_string()]
        );
        // 非 /t 路径只有一条候选，不该重复
        assert_eq!(candidate_keys("/problems/x"), vec!["/problems/x".to_string()]);
    }
}
