//! Automation 事件引擎 + Plugin 能力清单（Phase 5）。
//!
//! - `POST /api/automation/trigger`：事件入口。匹配 enabled 且 event 相符的
//!   工作流，逐个执行（MVP 动作 = 生成 AI 任务 + 审计留痕），更新 last_run_at；
//!   真实动作执行器（发消息/调外部 API）按 step 类型在 Phase 5+ 扩展，
//!   触发与记录链路不变。
//! - `GET /api/plugins`：对外能力清单 —— 外部集成方（Plugin）与内部页面
//!   走同一条 Capability→Policy 链路（G4：外部无特权）。

use axum::{extract::State, Json};
use sea_orm::{Statement, Value as SqlValue};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    auth::{ensure, Auth},
    db::now_iso,
    error::{ok, ok_list, ApiError, ApiResult},
    state::AppState,
};

fn sval(s: String) -> SqlValue {
    SqlValue::String(Some(s))
}

#[derive(serde::Deserialize)]
pub struct TriggerReq {
    pub event: String,
    #[serde(default)]
    pub context: Value,
}

/// POST /api/automation/trigger
pub async fn trigger(State(st): State<AppState>, auth: Auth, Json(req): Json<TriggerReq>) -> ApiResult {
    ensure(&auth, "automation.workflows.toggle")?;
    if req.event.trim().is_empty() {
        return Err(ApiError::bad("event 必填"));
    }
    let now = now_iso();
    let rows = st
        .db
        .query_all_statement(Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT id, name, step_count, steps FROM workflows \
             WHERE tenant_id = ? AND enabled = 1 AND event = ?",
            vec![sval(st.tenant.clone()), sval(req.event.clone())],
        ))
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;

    let mut runs: Vec<Value> = Vec::new();
    for r in rows {
        let id: String = r.try_get("", "id").map_err(internal)?;
        let name: String = r.try_get("", "name").map_err(internal)?;
        let steps: i64 = r.try_get::<i64>("", "step_count").unwrap_or(1);
        let steps_json: String = r.try_get::<String>("", "steps").unwrap_or_default();
        execute_workflow(&st, &id, &name, steps, steps_json, &req.event, &auth.0.username, &req.context).await?;
        runs.push(json!({ "workflowId": id, "name": name }));
    }

    ok(json!({ "event": req.event, "matched": runs.len(), "runs": runs }))
}

/// 执行核（HTTP 触发与定时调度共用）：
/// 按 steps JSON 执行真实动作，登记 AI 任务 + 审计留痕 + 更新 last_run_at。
/// step 类型（B2 动作执行器）：
/// - notify: 经 notify.rs 发群机器人消息（message 支持 {字段} 模板替换）
/// - task:   创建跟进任务（title 支持模板）
/// - log:    仅在审计里记录（默认动作）
/// 无 steps 时保持旧行为（仅登记一条自动任务）。
pub(crate) async fn execute_workflow(
    st: &AppState,
    wf_id: &str,
    name: &str,
    step_count: i64,
    steps_json: String,
    event: &str,
    actor: &str,
    context: &Value,
) -> Result<(), ApiError> {
    let now = now_iso();
    let task_id = Uuid::new_v4().to_string();

    // 动作执行（B2）：模板替换 {字段} ← context
    let steps: Vec<Value> = serde_json::from_str(&steps_json).unwrap_or_default();
    let mut executed: Vec<String> = Vec::new();
    let mut task_titles: Vec<String> = Vec::new();
    if steps.is_empty() {
        task_titles.push(format!("自动流程：{name}"));
    }
    for step in &steps {
        let ty = step.get("type").and_then(|v| v.as_str()).unwrap_or("log");
        match ty {
            "notify" => {
                let tmpl = step.get("message").and_then(|v| v.as_str()).unwrap_or("自动化通知").to_string();
                let msg = render_template(&tmpl, context);
                crate::notify::send_raw(st, &msg);
                executed.push(format!("notify:{msg}"));
            }
            "task" => {
                let tmpl = step.get("title").and_then(|v| v.as_str()).unwrap_or("自动化跟进").to_string();
                task_titles.push(render_template(&tmpl, context));
            }
            _ => executed.push("log".into()),
        }
    }

    // 登记自动任务（含每个 task 步骤）
    if task_titles.is_empty() {
        task_titles.push(format!("自动流程：{name}"));
    }
    let first = task_titles.remove(0);
    st.db
        .execute_statement(Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Sqlite,
            "INSERT INTO ai_tasks (id, tenant_id, title, capability, status, result, created_at, updated_at) \
             VALUES (?, ?, ?, 'automation.workflow.run', 'done', ?, ?, ?)",
            vec![
                sval(task_id.clone()),
                sval(st.tenant.clone()),
                sval(first),
                sval(format!("由事件 {} 触发，{} 个步骤执行完成", event, executed.join(" | "))),
                sval(now.clone()),
                sval(now.clone()),
            ],
        ))
        .await
        .map_err(|e| ApiError::bad(format!("任务写入失败：{e}")))?;
    for t in task_titles {
        st.db
            .execute_statement(Statement::from_sql_and_values(
                sea_orm::DatabaseBackend::Sqlite,
                "INSERT INTO ai_tasks (id, tenant_id, title, capability, status, result, created_at, updated_at) \
                 VALUES (?, ?, ?, 'automation.workflow.run', 'done', '由工作流「{}」生成', ?, ?)",
                vec![
                    sval(Uuid::new_v4().to_string()),
                    sval(st.tenant.clone()),
                    sval(t),
                    sval(name.to_string()),
                    sval(now.clone()),
                    sval(now.clone()),
                ],
            ))
            .await
            .map_err(|e| ApiError::bad(format!("任务写入失败：{e}")))?;
    }

    audit_run(st, actor, event, wf_id, name);
    st.db
        .execute_statement(Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Sqlite,
            "UPDATE workflows SET last_run_at = ?, updated_at = ? WHERE id = ?",
            vec![sval(now.clone()), sval(now.clone()), sval(wf_id.to_string())],
        ))
        .await
        .ok();
    Ok(())
}

/// {字段} → context[字段] 模板替换（找不到保留原样）
fn render_template(tmpl: &str, context: &Value) -> String {
    let mut out = tmpl.to_string();
    if let Some(obj) = context.as_object() {
        for (k, v) in obj {
            let ph = format!("{{{}}}", k);
            let val = v.as_str().unwrap_or(&v.to_string()).to_string();
            out = out.replace(&ph, &val);
        }
    }
    out
}

/// GET /api/plugins —— 能力清单（登录即可见；调用仍受各自权限约束）
pub async fn plugins(auth: Auth) -> ApiResult {
    let _ = auth; // 仅要求有效登录
    let caps = vec![
        json!({ "capability": "content.articles.draft",   "requiredPerm": "content.articles.create", "escalatable": false }),
        json!({ "capability": "content.articles.publish", "requiredPerm": "content.articles.publish","escalatable": true }),
        json!({ "capability": "content.seo.optimize",     "requiredPerm": "content.articles.update", "escalatable": false }),
        json!({ "capability": "content.translate",        "requiredPerm": "content.articles.update", "escalatable": false }),
    ];
    let total = caps.len();
    ok_list(caps, total)
}

/// 工作流运行审计（复用 ai_audit_log，capability 固定）
fn audit_run(st: &AppState, actor: &str, event: &str, workflow_id: &str, name: &str) {
    let sql = "INSERT INTO ai_audit_log (id, tenant_id, actor, actor_role, capability, decision, target_id, detail, created_at) \
               VALUES (?, ?, ?, ?, 'automation.workflow.run', 'executed', ?, ?, ?)";
    let vals = vec![
        sval(Uuid::new_v4().to_string()),
        sval(st.tenant.clone()),
        sval(actor.to_string()),
        sval("system".into()),
        sval(workflow_id.to_string()),
        sval(format!("工作流「{name}」由事件 {event} 触发")),
        sval(now_iso()),
    ];
    let db = st.db.clone();
    tokio::spawn(async move {
        if let Err(e) = db.execute_statement(Statement::from_sql_and_values(sea_orm::DatabaseBackend::Sqlite, sql, vals)).await {
            eprintln!("[audit] 工作流运行记录失败：{e}");
        }
    });
}

fn internal(e: impl std::fmt::Display) -> ApiError {
    ApiError::bad(format!("行解析失败：{e}"))
}
