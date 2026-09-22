//! 事件生态 + 统计看板。
//!
//! - `POST /api/public/track`  免认证，写入一条事件（如 `article_view`），并同步累加
//!   `articles.views` 缓存计数；其它类型事件（注册 / 评论 / 订阅等）亦可写入，供后续扩展。
//! - `GET /api/admin/stats`     需 `content.articles.view` 权限，聚合看板所需的全部指标。
//!
//! events 表是统一事件日志（事件生态），统计看板只负责从中读取，互不耦合。

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use sea_orm::Value as SqlValue;
use serde_json::{json, Value};

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::auth::{ensure, Auth};
use crate::cmsdb::Row;
use crate::error::{ok, ApiError, ApiResult};
use crate::state::AppState;

/// 读 TEXT 列（NULL→空串）
fn s(r: &Row, col: &str) -> String {
    r.try_get::<String>("", col).unwrap_or_default()
}

/// 读 INTEGER 列（NULL→0）
fn i(r: &Row, col: &str) -> i64 {
    r.try_get::<i64>("", col).unwrap_or(0)
}

/// POST /api/public/track —— 免认证写入一条事件
pub async fn track(State(st): State<AppState>, Json(body): Json<Value>) -> ApiResult {
    let etype = body
        .get("type")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    if etype.is_empty() {
        return Err(ApiError::bad("缺少事件类型 type"));
    }
    let ref_id = body
        .get("refId")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let ref_key = body
        .get("refKey")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let payload = body
        .get("payload")
        .cloned()
        .map(|v| v.to_string())
        .unwrap_or_default();
    // 匿名访客标识（分析归因）。前端 localStorage 里的持久 uuid，
    // 注册时原样带给 members::register 即可把「匿名浏览 → 注册」接上。
    // **刻意不做格式校验**：它只是本站自用的去重键，不是安全边界 ——
    // 校验只会把某些隐私模式的浏览器挡在外面，换不回任何安全性。
    // 但要限长，否则一个恶意客户端能往库里灌超长字符串。
    let visitor_id = body
        .get("visitorId")
        .and_then(|x| x.as_str())
        .map(|x| x.trim().chars().take(64).collect::<String>())
        .unwrap_or_default();
    let now = crate::db::now_iso();
    let id = uuid::Uuid::new_v4().to_string();

    st.db
        .execute_statement(sea_orm::Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Sqlite,
            "INSERT INTO events (id, tenant_id, type, ref_id, ref_key, payload, visitor_id, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            vec![
                SqlValue::String(Some(id)),
                SqlValue::String(Some(st.tenant.clone())),
                SqlValue::String(Some(etype.clone())),
                SqlValue::String(Some(ref_id.clone())),
                SqlValue::String(Some(ref_key.clone())),
                SqlValue::String(Some(payload)),
                SqlValue::String(Some(visitor_id)),
                SqlValue::String(Some(now)),
            ],
        ))
        .await
        .map_err(|e| ApiError::bad(format!("写入事件失败：{e}")))?;

    // 文章阅读：views 缓存计数改内存累加（P2：消除每次阅读一条 UPDATE 写放大），
    // 由 scheduler 每分钟经 flush_views 批量落库；进程重启最多丢一个 tick 窗口的计数。
    if etype == "article_view" && !ref_id.is_empty() {
        bump_view(&st.tenant, &ref_id);
    }

    Ok((StatusCode::OK, Json(json!({ "ok": true }))).into_response())
}

// ── views 批处理（P2）：读多写少的缓存计数合并写 ──
type ViewKey = (String, String); // (tenant_id, article_id)

fn view_board() -> &'static Mutex<HashMap<ViewKey, u64>> {
    static BOARD: OnceLock<Mutex<HashMap<ViewKey, u64>>> = OnceLock::new();
    BOARD.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 内存累加一次阅读（events 明细仍即时落库，统计时序不受影响）
fn bump_view(tenant: &str, article_id: &str) {
    let mut b = view_board().lock().unwrap();
    *b.entry((tenant.to_string(), article_id.to_string())).or_insert(0) += 1;
}

/// scheduler tick 调用：把内存计数批量落库。多实例下各实例各自累加、增量 UPDATE，天然安全。
pub async fn flush_views(st: &AppState) -> usize {
    let drained: Vec<(ViewKey, u64)> = {
        let mut b = view_board().lock().unwrap();
        b.drain().collect()
    };
    let mut n = 0usize;
    for ((tenant, aid), delta) in drained {
        if delta == 0 {
            continue;
        }
        let r = st
            .db
            .execute(
                "UPDATE articles SET views = views + ? WHERE id = ? AND tenant_id = ?",
                vec![
                    SqlValue::BigInt(Some(delta as i64)),
                    SqlValue::String(Some(aid)),
                    SqlValue::String(Some(tenant)),
                ],
            )
            .await;
        if r.is_ok() {
            n += 1;
        }
    }
    n
}

/// GET /api/admin/stats —— 聚合看板指标
pub async fn stats(State(st): State<AppState>, auth: Auth) -> ApiResult {
    ensure(&auth, "content.articles.view")?;
    let t = st.tenant.clone();

    // 最近 14 天日期（含今天），用于时间序列横轴
    let days: Vec<String> = (0..14)
        .map(|d| {
            (chrono::Utc::now() - chrono::Duration::days((13 - d) as i64))
                .format("%Y-%m-%d")
                .to_string()
        })
        .collect();
    let since = format!("{}T00:00:00.000Z", days[0]);

    // 每日阅读量（来自事件日志）
    let rows = st
        .db
        .query_all_statement(sea_orm::Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT date(created_at) AS d, COUNT(*) AS n FROM events \
             WHERE tenant_id = ? AND type = 'article_view' AND created_at >= ? \
             GROUP BY d",
            vec![
                SqlValue::String(Some(t.clone())),
                SqlValue::String(Some(since.clone())),
            ],
        ))
        .await
        .map_err(|e| ApiError::bad(format!("统计失败：{e}")))?;
    let mut by_day: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    for r in &rows {
        by_day.insert(s(&r, "d"), i(&r, "n"));
    }
    let views_series: Vec<i64> = days.iter().map(|d| *by_day.get(d).unwrap_or(&0)).collect();

    // 总计类指标
    let total_views = count_sql(&st, &t, "SELECT COUNT(*) AS n FROM events WHERE tenant_id = ? AND type = 'article_view'").await?;
    let total_articles = count_sql(&st, &t, "SELECT COUNT(*) AS n FROM articles WHERE tenant_id = ? AND status = 'published'").await?;
    let total_members = count_sql(&st, &t, "SELECT COUNT(*) AS n FROM members WHERE tenant_id = ?").await?;
    let total_comments = count_sql(&st, &t, "SELECT COUNT(*) AS n FROM comments WHERE tenant_id = ?").await?;

    // 热门文章 Top5（按阅读事件数）
    let top_rows = st
        .db
        .query_all_statement(sea_orm::Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT a.title AS title, a.slug AS slug, COUNT(*) AS n FROM events e \
             JOIN articles a ON a.id = e.ref_id \
             WHERE e.tenant_id = ? AND e.type = 'article_view' \
             GROUP BY e.ref_id ORDER BY n DESC LIMIT 5",
            vec![SqlValue::String(Some(t.clone()))],
        ))
        .await
        .map_err(|e| ApiError::bad(format!("统计失败：{e}")))?;
    let top_articles: Vec<Value> = top_rows
        .iter()
        .map(|r| json!({ "title": s(r, "title"), "slug": s(r, "slug"), "views": i(r, "n") }))
        .collect();

    // ---- 社区经营指标（订单 / 收入 / 积分 / 付费会员 / 到期提醒） ----
    let now = chrono::Utc::now();
    let now_rfc = now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let in7_rfc = (now + chrono::Duration::days(7)).to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

    let revenue_total = count_sql(&st, &t, "SELECT COALESCE(SUM(amount_cents), 0) AS n FROM orders WHERE tenant_id = ? AND status = 'paid'").await?;
    let orders_paid = count_sql(&st, &t, "SELECT COUNT(*) AS n FROM orders WHERE tenant_id = ? AND status = 'paid'").await?;
    let orders_pending = count_sql(&st, &t, "SELECT COUNT(*) AS n FROM orders WHERE tenant_id = ? AND status = 'pending'").await?;
    let plan_members = count_sql(&st, &t, "SELECT COUNT(*) AS n FROM members WHERE tenant_id = ? AND plan != 'free'").await?;
    let points_issued = count_sql(&st, &t, "SELECT COALESCE(SUM(delta), 0) AS n FROM points_ledger WHERE tenant_id = ? AND delta > 0").await?;
    let points_spent = count_sql(&st, &t, "SELECT COALESCE(SUM(-delta), 0) AS n FROM points_ledger WHERE tenant_id = ? AND delta < 0").await?;

    // 近 14 天收入（按支付时间）与新增会员（按注册时间）
    let paid_rows = st
        .db
        .query_all_statement(sea_orm::Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT date(paid_at) AS d, COALESCE(SUM(amount_cents), 0) AS n FROM orders \
             WHERE tenant_id = ? AND status = 'paid' AND paid_at >= ? GROUP BY d",
            vec![
                SqlValue::String(Some(t.clone())),
                SqlValue::String(Some(since.clone())),
            ],
        ))
        .await
        .map_err(|e| ApiError::bad(format!("统计失败：{e}")))?;
    let mut paid_by_day: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    for r in &paid_rows {
        paid_by_day.insert(s(&r, "d"), i(&r, "n"));
    }
    let revenue_series: Vec<i64> = days.iter().map(|d| *paid_by_day.get(d).unwrap_or(&0)).collect();

    let mem_rows = st
        .db
        .query_all_statement(sea_orm::Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT date(created_at) AS d, COUNT(*) AS n FROM members \
             WHERE tenant_id = ? AND created_at >= ? GROUP BY d",
            vec![
                SqlValue::String(Some(t.clone())),
                SqlValue::String(Some(since.clone())),
            ],
        ))
        .await
        .map_err(|e| ApiError::bad(format!("统计失败：{e}")))?;
    let mut mem_by_day: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    for r in &mem_rows {
        mem_by_day.insert(s(&r, "d"), i(&r, "n"));
    }
    let new_members_series: Vec<i64> = days.iter().map(|d| *mem_by_day.get(d).unwrap_or(&0)).collect();

    // 临期/已过期付费会员（7 天内到期 + 已过期，按到期时间升序，最多 12 条）
    let exp_rows = st
        .db
        .query_all_statement(sea_orm::Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT email, name, plan, plan_expires_at FROM members \
             WHERE tenant_id = ? AND plan != 'free' AND plan_expires_at != '' AND plan_expires_at <= ? \
             ORDER BY plan_expires_at ASC LIMIT 12",
            vec![
                SqlValue::String(Some(t.clone())),
                SqlValue::String(Some(in7_rfc)),
            ],
        ))
        .await
        .map_err(|e| ApiError::bad(format!("统计失败：{e}")))?;
    let expiring_members: Vec<Value> = exp_rows
        .iter()
        .map(|r| {
            json!({
                "email": s(r, "email"),
                "name": s(r, "name"),
                "plan": s(r, "plan"),
                "planExpiresAt": s(r, "plan_expires_at"),
            })
        })
        .collect();

    ok(json!({
        "totalViews": total_views,
        "totalArticles": total_articles,
        "totalMembers": total_members,
        "totalComments": total_comments,
        "days": days,
        "viewsSeries": views_series,
        "topArticles": top_articles,
        "community": {
            "revenueTotal": revenue_total,
            "ordersPaid": orders_paid,
            "ordersPending": orders_pending,
            "planMembers": plan_members,
            "pointsIssued": points_issued,
            "pointsSpent": points_spent,
            "revenueSeries": revenue_series,
            "newMembersSeries": new_members_series,
            "expiringMembers": expiring_members,
            "now": now_rfc,
        },
    }))
}

/// 单行 COUNT(*) 查询，返回整数
async fn count_sql(st: &AppState, t: &str, sql: &str) -> Result<i64, ApiError> {
    let r = st
        .db
        .query_one_statement(sea_orm::Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Sqlite,
            sql,
            vec![SqlValue::String(Some(t.to_string()))],
        ))
        .await
        .map_err(|e| ApiError::bad(format!("统计失败：{e}")))?;
    Ok(r.map(|r| i(&r, "n")).unwrap_or(0))
}
