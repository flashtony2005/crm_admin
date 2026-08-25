//! AI 执行器 —— Capability → Policy → Action → Audit 全链路（Phase 2）。
//!
//! 设计（PRODUCT_VISION G4「AI 是执行者，不是超级管理员」）：
//! - 唯一入口 `POST /api/ai/invoke`，能力白名单注册表驱动；
//! - Policy：执行者权限 = 登录用户角色矩阵。有码 → 直接执行；
//!   无码且能力可升级（Editor 场景）→ 生成 pending 审批，人裁决后执行；
//!   无码不可升级（Viewer）→ 403；
//! - 每次尝试（执行/升级/拒绝）都写审计。
//!
//! Phase 2 的"生成"是确定性模板（无外部 LLM）；接真模型只替换 execute_*，
//! 链路与策略不变。

use axum::{extract::{State}, Json};
use sea_orm::{Statement, Value as SqlValue};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    auth::{ensure, Auth},
    db::now_iso,
    error::{ok, ok_list, ApiError, ApiResult},
    state::AppState,
};

pub struct CapDef {
    pub name: &'static str,
    pub required_perm: &'static str,
    /// 缺码时是否允许降级为「待审批」请求（Editor 发布场景）
    pub escalatable: bool,
}

pub static CAPS: &[CapDef] = &[
    CapDef { name: "content.articles.draft", required_perm: "content.articles.create", escalatable: false },
    CapDef { name: "content.articles.publish", required_perm: "content.articles.publish", escalatable: true },
    CapDef { name: "content.seo.optimize", required_perm: "content.articles.update", escalatable: false },
    CapDef { name: "content.translate", required_perm: "content.articles.update", escalatable: false },
];

fn find_cap(name: &str) -> Option<&'static CapDef> {
    CAPS.iter().find(|c| c.name == name)
}


/// 写审计（永不因审计失败阻断主流程——但记录到 stderr）
fn audit(st: &AppState, actor: &str, role: &str, capability: &str, decision: &'static str, target: String, detail: String) {
    let sql = "INSERT INTO ai_audit_log (id, tenant_id, actor, actor_role, capability, decision, target_id, detail, created_at) \
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)";
    let vals = vec![
        sval(Uuid::new_v4().to_string()),
        sval(st.tenant.clone()),
        sval(actor.to_string()),
        sval(role.to_string()),
        sval(capability.to_string()),
        sval(decision.to_string()),
        sval(target),
        sval(detail),
        sval(now_iso()),
    ];
    let db = st.db.clone();
    let sql = sql.to_string();
    // 写审计异步化：不阻塞主流程；失败仅记日志
    tokio::spawn(async move {
        if let Err(e) = db.execute_statement(Statement::from_sql_and_values(sea_orm::DatabaseBackend::Sqlite, sql, vals)).await {
            eprintln!("[audit] 写入失败：{e}");
        }
    });
}

fn sval(s: String) -> SqlValue {
    SqlValue::String(Some(s))
}

#[derive(serde::Deserialize)]
pub struct InvokeReq {
    pub capability: String,
    #[serde(default)]
    pub input: Value,
}

/// POST /api/ai/invoke
pub async fn invoke(State(st): State<AppState>, auth: Auth, Json(req): Json<InvokeReq>) -> ApiResult {
    let Some(cap) = find_cap(&req.capability) else {
        return Err(ApiError::bad(format!("未知能力：{}", req.capability)));
    };
    let role = auth.0.role.clone();
    let actor = auth.0.username.clone();

    // ── Policy ──
    let perm_result = ensure(&auth, cap.required_perm);
    match perm_result {
        Err(denied) => {
            let editor_may_escalate = cap.escalatable && role == "editor";
            if !editor_may_escalate {
                audit(&st, &actor, &role, cap.name, "denied", "-".into(), "权限不足且不可升级".into());
                return Err(denied);
            }
            // ── 升级为审批（发布场景）──
            let article_id = req.input.get("article_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if article_id.is_empty() {
                return Err(ApiError::bad("input.article_id 必填"));
            }
            let title = article_title(&st, &article_id).await?.unwrap_or_else(|| article_id.clone());
            let approval_id = Uuid::new_v4().to_string();
            let now = now_iso();
            let payload = json!({
                "capability": cap.name,
                "input": { "article_id": article_id },
            })
            .to_string();
            st.db
                .execute_statement(Statement::from_sql_and_values(
                    sea_orm::DatabaseBackend::Sqlite,
                    "INSERT INTO approvals (id, tenant_id, action, target, requested_by, risk, status, summary, payload, created_at, updated_at) \
                     VALUES (?, ?, 'publish', ?, ?, 'mid', 'pending', ?, ?, ?, ?)",
                    vec![
                        sval(approval_id.clone()),
                        sval(st.tenant.clone()),
                        sval(format!("article:{title}")),
                        sval(format!("{actor}（AI 代发起）")),
                        sval(format!("AI 能力 {} 需要你的裁决", cap.name)),
                        sval(payload),
                        sval(now.clone()),
                        sval(now),
                    ],
                ))
                .await
                .map_err(|e| ApiError::bad(format!("创建审批失败：{e}")))?;
            audit(&st, &actor, &role, cap.name, "escalated", approval_id.clone(), "缺发布权 → 转人工审批".into());
            // B1 推送：通知 Owner 有待裁决请求
            crate::notify::approval_created(&st, &title);
            return ok(json!({
                "decision": "needs_approval",
                "approvalId": approval_id,
                "message": format!("「{title}」的发布请求已提交，等待 Owner 批准。"),
            }));
        }
        Ok(()) => {}
    }

    // ── Action（直接执行）──
    let result = execute(&st, cap.name, &req.input).await?;
    audit(
        &st,
        &actor,
        &role,
        cap.name,
        "executed",
        result.get("targetId").and_then(|v| v.as_str()).unwrap_or("-").to_string(),
        "直接执行".into(),
    );
    ok(json!({ "decision": "executed", "result": result }))
}

async fn execute(st: &AppState, cap: &str, input: &Value) -> Result<Value, ApiError> {
    match cap {
        "content.articles.draft" => {
            let topic = input.get("topic").and_then(|v| v.as_str()).unwrap_or("新品");
            let title = input
                .get("title")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("AI 草稿：关于{topic}的介绍"));
            let id = Uuid::new_v4().to_string();
            let now = now_iso();
            // B5：配置 LLM_API_KEY 时走真实大模型生成，否则模板兜底
            let (content, gen) = match llm_draft(&topic, &title).await {
                Some(text) => (text, "llm"),
                None => (
                    format!(
                        "{title}\n\n由 AI 助手根据指令生成初稿，发布前请人工复核事实与价格信息。\n话题：{topic}"
                    ),
                    "template",
                ),
            };
            st.db
                .execute_statement(Statement::from_sql_and_values(
                    sea_orm::DatabaseBackend::Sqlite,
                    "INSERT INTO articles (id, tenant_id, title, summary, content, status, author, tags, created_at, updated_at) \
                     VALUES (?, ?, ?, ?, ?, 'draft', 'AI 助手', '[]', ?, ?)",
                    vec![
                        sval(id.clone()),
                        sval(st.tenant.clone()),
                        sval(title.clone()),
                        sval(format!("{topic}相关介绍（AI 初稿）")),
                        sval(content),
                        sval(now.clone()),
                        sval(now),
                    ],
                ))
                .await
                .map_err(|e| ApiError::bad(format!("写库失败：{e}")))?;
            Ok(json!({ "action": "draft_created", "targetId": id, "title": title, "generator": gen }))
        }
        "content.articles.publish" => {
            let id = require_article_id(input)?;
            touch_article(st, &id, "published").await?;
            Ok(json!({ "action": "published", "targetId": id }))
        }
        "content.seo.optimize" => {
            let id = require_article_id(input)?;
            append_article(st, &id, "\n\n【SEO】关键词建议：桂花栗子、欧包、社区烘焙坊、当季限定").await?;
            Ok(json!({ "action": "seo_appended", "targetId": id }))
        }
        "content.translate" => {
            let id = require_article_id(input)?;
            append_article(st, &id, "\n\n【EN】Osmanthus chestnut bread — autumn limited, baked fresh daily.").await?;
            Ok(json!({ "action": "translated", "targetId": id }))
        }
        _ => Err(ApiError::bad("能力未实现")),
    }
}

fn require_article_id(input: &Value) -> Result<String, ApiError> {
    input
        .get("article_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::bad("input.article_id 必填"))
}

async fn article_title(st: &AppState, id: &str) -> Result<Option<String>, ApiError> {
    let r = st
        .db
        .query_one_statement(Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT title FROM articles WHERE id = ? AND tenant_id = ? LIMIT 1",
            vec![sval(id.into()), sval(st.tenant.clone())],
        ))
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    Ok(r.and_then(|row| row.try_get::<String>("", "title").ok()))
}

async fn touch_article(st: &AppState, id: &str, status: &str) -> Result<(), ApiError> {
    st.db
        .execute_statement(Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Sqlite,
            "UPDATE articles SET status = ?, updated_at = ? WHERE id = ? AND tenant_id = ?",
            vec![sval(status.into()), sval(now_iso()), sval(id.into()), sval(st.tenant.clone())],
        ))
        .await
        .map_err(|e| ApiError::bad(format!("更新失败：{e}")))?;
    Ok(())
}

async fn append_article(st: &AppState, id: &str, tail: &str) -> Result<(), ApiError> {
    let cur = match article_content(st, id).await? {
        Some(c) => c,
        None => return Err(ApiError::not_found("文章不存在")),
    };
    st.db
        .execute_statement(Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Sqlite,
            "UPDATE articles SET content = ?, updated_at = ? WHERE id = ? AND tenant_id = ?",
            vec![
                sval(format!("{cur}{tail}")),
                sval(now_iso()),
                sval(id.into()),
                sval(st.tenant.clone()),
            ],
        ))
        .await
        .map_err(|e| ApiError::bad(format!("更新失败：{e}")))?;
    Ok(())
}

async fn article_content(st: &AppState, id: &str) -> Result<Option<String>, ApiError> {
    let r = st
        .db
        .query_one_statement(Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT content FROM articles WHERE id = ? AND tenant_id = ? LIMIT 1",
            vec![sval(id.into()), sval(st.tenant.clone())],
        ))
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    Ok(r.and_then(|row| row.try_get::<String>("", "content").ok()))
}

/// GET /api/ai/audit —— 审计流水（AI 操作可回溯）
pub async fn audit_list(
    State(st): State<AppState>,
    auth: Auth,
    axum::extract::Query(filters): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> ApiResult {
    ensure(&auth, "ai.tasks.view")?;
    // 可选等值过滤（白名单列），供页签与测试精确定位
    let mut where_clause = format!("tenant_id = '{}'", st.tenant);
    for (col, key) in [("decision", "decision"), ("capability", "capability")] {
        if let Some(v) = filters.get(key) {
            where_clause += &format!(" AND {} = '{}'", col, v.replace('\'', "''"));
        }
    }
    let rows = st
        .db
        .query_all_statement(Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT id, actor, actor_role, capability, decision, target_id, detail, created_at \
                 FROM ai_audit_log WHERE {where_clause} ORDER BY created_at DESC LIMIT 200"
            ),
        ))
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    let items: Vec<Value> = rows
        .iter()
        .filter_map(|r| {
            Some(json!({
                "id": r.try_get::<String>("", "id").ok()?,
                "actor": r.try_get::<String>("", "actor").ok()?,
                "actorRole": r.try_get::<String>("", "actor_role").ok()?,
                "capability": r.try_get::<String>("", "capability").ok()?,
                "decision": r.try_get::<String>("", "decision").ok()?,
                "targetId": r.try_get::<String>("", "target_id").ok()?,
                "detail": r.try_get::<String>("", "detail").ok()?,
                "createdAt": r.try_get::<String>("", "created_at").ok()?,
            }))
        })
        .collect();
    let total = items.len();
    ok_list(items, total)
}

/// B5：OpenAI 兼容 Chat Completions（默认 DeepSeek；LLM_BASE_URL/LLM_MODEL 可换）。
/// 未配置 LLM_API_KEY 或调用失败 → None（上层回退模板）。
async fn llm_draft(topic: &str, title: &str) -> Option<String> {
    let api_key = std::env::var("LLM_API_KEY").ok()?;
    let base = std::env::var("LLM_BASE_URL").unwrap_or_else(|_| "https://api.deepseek.com/v1".into());
    let model = std::env::var("LLM_MODEL").unwrap_or_else(|_| "deepseek-chat".into());

    let prompt = format!(
        "你是社区烘焙店的运营助手。请为话题「{topic}」写一篇 300 字以内的微信公众号短文，\n         标题用「{title}」，正文面向周边居民，语气亲切，结尾加一句到店/预订提示。只输出正文。"
    );
    let body = serde_json::json!({
        "model": model,
        "messages": [
            { "role": "system", "content": "你是社区烘焙店的运营助手。" },
            { "role": "user", "content": prompt },
        ],
        "temperature": 0.8,
        "max_tokens": 800,
    });

    let resp = reqwest::Client::new()
        .post(format!("{}/chat/completions", base.trim_end_matches('/')))
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .ok()?;
    let v: serde_json::Value = resp.json().await.ok()?;
    let text = v
        .pointer("/choices/0/message/content")
        .and_then(|c| c.as_str())
        .map(|s| s.trim().to_string())?;
    if text.is_empty() {
        return None;
    }
    Some(format!("{title}\n\n{text}"))
}
