//! 分析归因：哪篇文章带来了会员，以及带来了多少钱。
//!
//! 数据链是三段拼起来的：
//! ```text
//!   匿名访客(localStorage uuid)  →  events.visitor_id（浏览轨迹）
//!        ↓ 注册时固化
//!   members.first_touch_article_id  →  与 orders 关联算 GMV
//! ```
//! 任何一段断掉，归因就只能靠感觉 —— 所以本模块**把缺口显式算出来**
//! （见 `gaps`），而不是让它们在外层看起来像 0。
//!
//! ## 为什么口径是「首次触达」
//! 首次触达回答**获客**：这一篇把人带进来了，所以该多写这一类。
//! 末次触达回答**促成**：这一篇让人下了决心，所以该在这类文里放转化位。
//! 两者都有用，但混成一个数字就没法决策。更关键的是末次触达**天生会被污染** ——
//! 站内推荐位、热门榜、「相关阅读」每一次内部跳转都在改写它，
//! 最终几乎所有转化都会归到少数几个流量入口上，而真正的获客入口被淹掉。
//! 首次触达不会被站内跳转改写，是唯一能直接指导选题的口径。
//!
//! ## 为什么在会员表上固化结果，而不是每次实时反查 events
//! 1. 首次触达是**既成事实**，不该随事件保留策略或清理而变；
//! 2. 事件量涨上来后实时 JOIN 代价高，而会员表天生就是归因结果的归属地；
//! 3. 事件表将来若归档，历史归因不会跟着丢。

use axum::extract::State;
use axum::Json;
use sea_orm::Value as SqlValue;
use serde_json::{json, Value};
use std::collections::HashMap;

use crate::auth::{ensure, Auth};
use crate::cmsdb::Row;
use crate::error::{ok, ApiResult};
use crate::state::AppState;

fn sval(s: String) -> SqlValue {
    SqlValue::String(Some(s))
}

fn s(r: &Row, col: &str) -> String {
    r.try_get::<String>("", col).unwrap_or_default()
}

fn i(r: &Row, col: &str) -> i64 {
    r.try_get::<i64>("", col).unwrap_or(0)
}

/// 单篇文章的归因累加器。
#[derive(Default)]
struct Acc {
    reads: i64,
    visitors: i64,
    signups: i64,
    payers: i64,
    gmv_cents: i64,
}

/// GET /api/admin/attribution —— 内容 → 会员 → 收入
pub async fn overview(State(st): State<AppState>, auth: Auth) -> ApiResult {
    ensure(&auth, "analytics.attribution.view")?;
    let tenant = st.tenant.clone();

    // ── ① 阅读面：从事件表按文章聚合 ──────────────────────────────
    // visitors 用 `CASE WHEN visitor_id <> ''` 过滤掉空串 —— 升级前的老事件
    // visitor_id 是空串而**不是** NULL，不过滤的话所有老事件会被算成
    // 「同一个访客」，独立访客数直接变成一个常数。
    let read_rows = st
        .db
        .query_all(
            "SELECT ref_id AS aid, \
                    COUNT(*) AS reads, \
                    COUNT(DISTINCT CASE WHEN visitor_id <> '' THEN visitor_id END) AS visitors \
             FROM events \
             WHERE tenant_id = ? AND type = 'article_view' AND ref_id <> '' \
             GROUP BY ref_id",
            vec![sval(tenant.clone())],
        )
        .await
        .map_err(|e| crate::error::ApiError::bad(format!("归因查询失败：{e}")))?;

    // ── ② 注册面：首次触达归因 ────────────────────────────────────
    let signup_rows = st
        .db
        .query_all(
            "SELECT first_touch_article_id AS aid, COUNT(*) AS signups \
             FROM members \
             WHERE tenant_id = ? AND first_touch_article_id <> '' \
             GROUP BY first_touch_article_id",
            vec![sval(tenant.clone())],
        )
        .await
        .map_err(|e| crate::error::ApiError::bad(format!("归因查询失败：{e}")))?;

    // ── ③ 收入面：归因会员的已支付订单 ────────────────────────────
    // JOIN 条件里带 tenant_id：orders 与 members 都有租户列，
    // 只按 member_id 关联在单租户下看不出问题，多租户下会串账。
    let gmv_rows = st
        .db
        .query_all(
            "SELECT m.first_touch_article_id AS aid, \
                    COUNT(DISTINCT m.id) AS payers, \
                    COALESCE(SUM(o.amount_cents), 0) AS gmv \
             FROM members m \
             JOIN orders o ON o.tenant_id = m.tenant_id AND o.member_id = m.id AND o.status = 'paid' \
             WHERE m.tenant_id = ? AND m.first_touch_article_id <> '' \
             GROUP BY m.first_touch_article_id",
            vec![sval(tenant.clone())],
        )
        .await
        .map_err(|e| crate::error::ApiError::bad(format!("归因查询失败：{e}")))?;

    // ── 合并到同一张累加表 ────────────────────────────────────────
    let mut map: HashMap<String, Acc> = HashMap::new();
    for r in &read_rows {
        let e = map.entry(s(r, "aid")).or_default();
        e.reads += i(r, "reads");
        e.visitors += i(r, "visitors");
    }
    for r in &signup_rows {
        map.entry(s(r, "aid")).or_default().signups += i(r, "signups");
    }
    for r in &gmv_rows {
        let e = map.entry(s(r, "aid")).or_default();
        e.payers += i(r, "payers");
        e.gmv_cents += i(r, "gmv");
    }

    // ── 文章标题/链接（一次全量取，避免 IN 占位符拼接）──────────────
    // 归因是低频分析页，全量读一次文章索引的代价可以忽略；
    // 相比按 aid 拼 `IN (?,?,...)`，这样少一处容易写错的动态 SQL。
    let art_rows = st
        .db
        .query_all(
            "SELECT id, title, slug, status FROM articles WHERE tenant_id = ?",
            vec![sval(tenant.clone())],
        )
        .await
        .map_err(|e| crate::error::ApiError::bad(format!("归因查询失败：{e}")))?;
    let mut meta: HashMap<String, (String, String, String)> = HashMap::new();
    for r in &art_rows {
        meta.insert(s(r, "id"), (s(r, "title"), s(r, "slug"), s(r, "status")));
    }

    let mut rows: Vec<Value> = map
        .iter()
        .map(|(aid, a)| {
            let (title, slug, status) = meta
                .get(aid)
                .cloned()
                .unwrap_or_else(|| (String::new(), String::new(), String::new()));
            json!({
                "articleId": aid,
                // 文章可能已被删除 —— 归因记录留在会员身上，标题取不到就留空，
                // 由前端显示「（已删除）」而不是把这一行丢掉：丢掉会让总数对不上。
                "title": title,
                "slug": slug,
                "status": status,
                "missing": title.is_empty(),
                "reads": a.reads,
                "visitors": a.visitors,
                "signups": a.signups,
                "payers": a.payers,
                "gmvCents": a.gmv_cents,
            })
        })
        .collect();
    // 排序：先看带来注册最多的（选题价值），再看向上付费（商业价值）。
    rows.sort_by(|a, b| {
        let ka = (
            -a["signups"].as_i64().unwrap_or(0),
            -a["gmvCents"].as_i64().unwrap_or(0),
            -a["reads"].as_i64().unwrap_or(0),
        );
        let kb = (
            -b["signups"].as_i64().unwrap_or(0),
            -b["gmvCents"].as_i64().unwrap_or(0),
            -b["reads"].as_i64().unwrap_or(0),
        );
        ka.cmp(&kb)
    });

    // ── 缺口：归因断层必须显式暴露 ────────────────────────────────
    // 把「未归因」当 0 藏起来，会让每一篇文章的数字都虚高，
    // 而且掩盖了「链路根本没接上」这种需要立刻处理的问题。
    let sum = st
        .db
        .query_one(
            "SELECT \
               (SELECT COUNT(*) FROM members WHERE tenant_id = ?) AS total_members, \
               (SELECT COUNT(*) FROM members WHERE tenant_id = ? AND first_touch_article_id = '' AND visitor_id = '') AS gap_no_visitor, \
               (SELECT COUNT(*) FROM members WHERE tenant_id = ? AND first_touch_article_id = '' AND visitor_id <> '') AS gap_no_read, \
               (SELECT COUNT(*) FROM orders WHERE tenant_id = ? AND status = 'paid') AS paid_orders, \
               (SELECT COALESCE(SUM(amount_cents), 0) FROM orders WHERE tenant_id = ? AND status = 'paid') AS total_gmv, \
               (SELECT COUNT(DISTINCT visitor_id) FROM events \
                 WHERE tenant_id = ? AND type = 'article_view' AND visitor_id <> '') AS site_visitors",
            vec![
                sval(tenant.clone()),
                sval(tenant.clone()),
                sval(tenant.clone()),
                sval(tenant.clone()),
                sval(tenant.clone()),
                sval(tenant.clone()),
            ],
        )
        .await
        .map_err(|e| crate::error::ApiError::bad(format!("归因查询失败：{e}")))?;

    let (total_members, gap_no_visitor, gap_no_read, paid_orders, total_gmv, site_visitors) = match &sum {
        Some(r) => (
            i(r, "total_members"),
            i(r, "gap_no_visitor"),
            i(r, "gap_no_read"),
            i(r, "paid_orders"),
            i(r, "total_gmv"),
            i(r, "site_visitors"),
        ),
        None => (0, 0, 0, 0, 0, 0),
    };

    let total_reads: i64 = rows.iter().map(|r| r["reads"].as_i64().unwrap_or(0)).sum();
    // 注意这两个数**不是一回事**，混用会把转化率算错：
    //   · site_visitors      —— 全站去重的访客数（同一个人看 10 篇只算 1）
    //   · sum_article_visitors —— 每篇文章各自去重后再相加（同一个人看 10 篇算 10）
    // 只有前者能当转化率的分母。后者可以 > 会员总数，看起来像「访客比人多」，
    // 那是正常的 —— 它衡量的是「内容触达总量」而不是「人群规模」。
    let sum_article_visitors: i64 = rows.iter().map(|r| r["visitors"].as_i64().unwrap_or(0)).sum();
    let attributed_signups: i64 = rows.iter().map(|r| r["signups"].as_i64().unwrap_or(0)).sum();
    let attributed_gmv: i64 = rows.iter().map(|r| r["gmvCents"].as_i64().unwrap_or(0)).sum();
    let attributed_payers: i64 = rows.iter().map(|r| r["payers"].as_i64().unwrap_or(0)).sum();

    ok(json!({
        "rows": rows,
        "summary": {
            "articles": rows.len(),
            "totalReads": total_reads,
            // 全站去重访客：转化率的分母用这个。
            "siteVisitors": site_visitors,
            // 各文章独立访客之和：衡量内容触达总量，**不能**当分母。
            "sumArticleVisitors": sum_article_visitors,
            "totalMembers": total_members,
            "attributedSignups": attributed_signups,
            "unattributedSignups": gap_no_visitor + gap_no_read,
            "totalPayers": attributed_payers,
            "paidOrders": paid_orders,
            "attributedGmvCents": attributed_gmv,
            "totalGmvCents": total_gmv,
        },
        // 两个缺口分开报：一个是「链路没接上」（前端没埋点 / 后台建号 / 老数据），
        // 一个是「接了但这人没看过文章就注册」。处理方式完全不同，
        // 合成一个数字就只能靠猜。
        "gaps": {
            "noVisitor": gap_no_visitor,
            "noRead": gap_no_read,
        },
    }))
}
