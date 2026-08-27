//! 定时触发器（B3）：让 `schedule.*` 事件真正按时间发生。
//!
//! 支持（本地时区）：
//! - `schedule.minutely` 每分钟（测试/演示用）
//! - `schedule.hourly`   每小时整点
//! - `schedule.daily`    每天 08:00
//! - `schedule.weekly`   每周一 08:00
//!
//! 去重：以「当前窗口起点」为准 —— 仅当 last_run_at < 窗口起点才触发，
//! 重启不重复、同一窗口不重发。主循环每 60s tick 一次。

use chrono::{DateTime, Datelike, Duration, Local, TimeZone, Timelike, Utc};
use sea_orm::{Statement, Value as SqlValue};

use crate::{automation, db, state::AppState};

const TICK_SECS: u64 = 60;

/// 启动后台调度循环
pub fn spawn(st: AppState) {
    tokio::spawn(async move {
        loop {
            tick_once(&st).await;
            tokio::time::sleep(std::time::Duration::from_secs(TICK_SECS)).await;
        }
    });
}

/// 定时发布：将到点的 `scheduled` 文章提升为 `published`。
///
/// 触发条件：`status='scheduled'` 且 `scheduled_at` 非空且 `<= 当前 UTC 时间`。
/// 提升时若 `published_at` 为空则补当前时间，保证公开列表/详情的日期正确。
pub async fn publish_scheduled(st: &AppState) -> usize {
    let now = db::now_iso();
    let rows = st
        .db
        .query_all(
            "SELECT id, published_at FROM articles \
             WHERE tenant_id = ? AND status = 'scheduled' \
               AND scheduled_at IS NOT NULL AND scheduled_at <> '' AND scheduled_at <= ?",
            vec![
                SqlValue::String(Some(st.tenant.clone())),
                SqlValue::String(Some(now.clone())),
            ],
        )
        .await
        .unwrap_or_default();
    let mut n = 0usize;
    for r in rows {
        let id = r.try_get::<String>("", "id").unwrap_or_default();
        if id.is_empty() {
            continue;
        }
        let published_at = r
            .try_get::<Option<String>>("", "published_at")
            .ok()
            .flatten()
            .unwrap_or_default();
        let pa = if published_at.is_empty() {
            now.clone()
        } else {
            published_at
        };
        let _ = st
            .db
            .execute(
                &format!(
                    "UPDATE articles SET status='published', published_at='{}', updated_at='{}' \
                     WHERE id='{}' AND tenant_id='{}'",
                    pa.replace('\'', "''"),
                    now.replace('\'', "''"),
                    id.replace('\'', "''"),
                    st.tenant.replace('\'', "''"),
                ),
                vec![],
            )
            .await;
        n += 1;
    }
    n
}

/// 计算事件在「now」所属的当前窗口起点（UTC）。None = 非定时事件。
pub fn window_start(event: &str, now: DateTime<Local>) -> Option<DateTime<Utc>> {
    let today8 = |d: DateTime<Local>| {
        d.date_naive()
            .and_hms_opt(8, 0, 0)
            .and_then(|t| Local.from_local_datetime(&t).single())
            .map(|x| x.with_timezone(&Utc))
    };
    match event {
        "schedule.minutely" => Some(
            now.date_naive()
                .and_hms_opt(now.hour(), now.minute(), 0)
                .and_then(|t| Local.from_local_datetime(&t).single())
                .map(|x| x.with_timezone(&Utc))?,
        ),
        "schedule.hourly" => Some(
            now.date_naive()
                .and_hms_opt(now.hour(), 0, 0)
                .and_then(|t| Local.from_local_datetime(&t).single())
                .map(|x| x.with_timezone(&Utc))?,
        ),
        "schedule.daily" => {
            // 今天未到 08:00 则属昨日窗口
            let today = today8(now);
            match today {
                Some(t) if now.with_timezone(&Utc) >= t => Some(t),
                _ => today8(now - Duration::days(1)),
            }
        }
        // 本周一 08:00；若尚未到达则回退上一周（chrono: Sat/Sun 偏移见 num_days_from_monday）
        "schedule.weekly" => {
            let monday = now.date_naive() - Duration::days((now.weekday().num_days_from_monday()) as i64);
            let this_week = monday
                .and_hms_opt(8, 0, 0)
                .and_then(|t| Local.from_local_datetime(&t).single())
                .map(|x| x.with_timezone(&Utc));
            match this_week {
                Some(t) if now.with_timezone(&Utc) >= t => Some(t),
                _ => (monday - Duration::days(7))
                    .and_hms_opt(8, 0, 0)
                    .and_then(|t| Local.from_local_datetime(&t).single())
                    .map(|x| x.with_timezone(&Utc)),
            }
        }
        _ => None,
    }
}

/// 扫描一轮：返回本次触发的流程数
pub async fn tick_once(st: &AppState) -> usize {
    // 先处理定时发布（与 schedule.* 工作流解耦，独立每分钟扫描）
    let _ = publish_scheduled(st).await;

    let rows = st
        .db
        .query_all_statement(Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT id, name, step_count, event, steps, last_run_at FROM workflows \
                 WHERE tenant_id = '{}' AND enabled = 1 AND event LIKE 'schedule.%'",
                st.tenant
            ),
        ))
        .await
        .unwrap_or_default();

    let now_local = Local::now();
    let mut fired = 0usize;
    for r in rows {
        let id = r.try_get::<String>("", "id").unwrap_or_default();
        let name = r.try_get::<String>("", "name").unwrap_or_default();
        let event = r.try_get::<String>("", "event").unwrap_or_default();
        if id.is_empty() || event.is_empty() { continue; }
        let Some(ws) = window_start(&event, now_local) else { continue };
        let last = r
            .try_get::<Option<String>>("", "last_run_at")
            .ok()
            .flatten()
            .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
            .map(|t| t.with_timezone(&Utc));
        let due = match last {
            Some(l) => l < ws,
            None => true,
        };
        if !due || Utc::now() < ws {
            continue;
        }
        let step_count = r.try_get::<i64>("", "step_count").unwrap_or(1);
        let steps_json = r.try_get::<String>("", "steps").unwrap_or_default();
        if automation::execute_workflow(st, &id, &name, step_count, steps_json, &event, "system", &serde_json::json!({}))
            .await
            .is_ok()
        {
            fired += 1;
        }
    }
    fired
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn local(y: i32, m: u32, d: u32, h: u32, min: u32) -> DateTime<Local> {
        Local
            .from_local_datetime(&NaiveDate::from_ymd_opt(y, m, d).unwrap().and_hms_opt(h, min, 0).unwrap())
            .single()
            .unwrap()
    }

    #[test]
    fn weekly_window_is_this_monday_8am() {
        // 2025-08-23 是周六 → 窗口起点应为本周一 08:00
        let sat = local(2025, 8, 23, 15, 30);
        let ws = window_start("schedule.weekly", sat).unwrap();
        assert_eq!(ws, local(2025, 8, 18, 8, 0).with_timezone(&Utc));

        // 周一当天上午 7 点仍属上一周窗口；9 点属于当周窗口
        let mon_morning = local(2025, 8, 25, 7, 0);
        assert_eq!(window_start("schedule.weekly", mon_morning).unwrap(), local(2025, 8, 18, 8, 0).with_timezone(&Utc));
        let mon_after = local(2025, 8, 25, 9, 0);
        assert_eq!(window_start("schedule.weekly", mon_after).unwrap(), local(2025, 8, 25, 8, 0).with_timezone(&Utc));
    }

    #[test]
    fn daily_and_hourly_windows() {
        let t = local(2025, 8, 23, 15, 30);
        assert_eq!(window_start("schedule.daily", t).unwrap(), local(2025, 8, 23, 8, 0).with_timezone(&Utc));
        assert_eq!(window_start("schedule.hourly", t).unwrap(), local(2025, 8, 23, 15, 0).with_timezone(&Utc));
        assert_eq!(window_start("schedule.minutely", t).unwrap(), local(2025, 8, 23, 15, 30).with_timezone(&Utc));
        assert!(window_start("customer.created", t).is_none());
    }

    /// 集成：临时库上建一条 minutely 工作流，tick 触发一次且同窗口不重复
    #[tokio::test]
    async fn tick_fires_once_per_window() {
        let db_path = std::env::temp_dir().join(format!("sched_test_{}.db", uuid::Uuid::new_v4()));
        let url = format!("sqlite://{}?mode=rwc", db_path.to_string_lossy());
        let raw = sea_orm::Database::connect(&url).await.unwrap();
        let db = crate::cmsdb::CmsDb::local(raw.clone());
        crate::db::bootstrap(&db).await;

        let now = crate::db::now_iso();
        for sql in [
            "INSERT INTO workflows (id, tenant_id, name, trigger_expr, event, step_count, enabled, created_at, updated_at) \
             VALUES ('wf_sched', 't_demo', '每分钟摘要', '每分钟', 'schedule.minutely', 1, 1, ?, ?)",
        ] {
            db.execute(&sql.replace("?", &format!("'{now}'")), vec![]).await.unwrap();
        }

        let st = AppState { db: db.clone(), tenant: "t_demo".into() };
        let _ = &raw;
        tick_once(&st).await; // 种子的 schedule.weekly 也会触发，故不限定总数

        // 仅统计本测试工作流的任务（种子的 weekly 流程另行触发，不计入）
        async fn n_tasks(db: &crate::cmsdb::CmsDb) -> i64 {
            let r = db.query_one(
                "SELECT COUNT(*) AS n FROM ai_tasks WHERE title = '自动流程：每分钟摘要'",
                vec![],
            ).await.unwrap().unwrap();
            r.try_get::<i64>("", "n").unwrap()
        }
        assert_eq!(n_tasks(&db).await, 1, "应生成一条自动任务");

        tick_once(&st).await;
        assert_eq!(n_tasks(&db).await, 1, "同窗口内不得重复触发");

        // 把 last_run_at 回拨到上一窗口 → 再次触发
        let old = (Utc::now() - Duration::minutes(2)).to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        db.execute(&format!("UPDATE workflows SET last_run_at = '{old}' WHERE id = 'wf_sched'"), vec![]).await.unwrap();
        tick_once(&st).await;
        assert_eq!(n_tasks(&db).await, 2, "跨窗口后应再次触发");

        let _ = std::fs::remove_file(&db_path);
    }
}

