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
    let now = crate::db::now_iso();
    let id = uuid::Uuid::new_v4().to_string();

    st.db
        .execute_statement(sea_orm::Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Sqlite,
            "INSERT INTO events (id, tenant_id, type, ref_id, ref_key, payload, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
            vec![
                SqlValue::String(Some(id)),
                SqlValue::String(Some(st.tenant.clone())),
                SqlValue::String(Some(etype.clone())),
                SqlValue::String(Some(ref_id.clone())),
                SqlValue::String(Some(ref_key.clone())),
                SqlValue::String(Some(payload)),
                SqlValue::String(Some(now)),
            ],
        ))
        .await
        .map_err(|e| ApiError::bad(format!("写入事件失败：{e}")))?;

    // 文章阅读：同步累加 articles.views 缓存计数（便于按文章取总量，无需每次聚合 events）
    if etype == "article_view" && !ref_id.is_empty() {
        let _ = st
            .db
            .execute_statement(sea_orm::Statement::from_sql_and_values(
                sea_orm::DatabaseBackend::Sqlite,
                "UPDATE articles SET views = views + 1 WHERE id = ? AND tenant_id = ?",
                vec![
                    SqlValue::String(Some(ref_id)),
                    SqlValue::String(Some(st.tenant.clone())),
                ],
            ))
            .await;
    }

    Ok((StatusCode::OK, Json(json!({ "ok": true }))).into_response())
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
                SqlValue::String(Some(since)),
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

    ok(json!({
        "totalViews": total_views,
        "totalArticles": total_articles,
        "totalMembers": total_members,
        "totalComments": total_comments,
        "days": days,
        "viewsSeries": views_series,
        "topArticles": top_articles,
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
