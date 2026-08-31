//! AI 大脑 —— 自然语言 → LLM function calling → Capability 执行链。
//!
//! 职责边界：大脑只负责「听懂 + 选工具 + 汇报」；真正的执行一律走
//! `ai::run_capability`（Policy → Approval → Audit 全链路原样复用），
//! 保证 G4：AI 是执行者，不是超级管理员——无权限自动转审批/拒绝。
//!
//! 配置（优先环境变量，其次 site_settings 键，后台改配置免重启）：
//! - LLM_API_KEY / llm_api_key   （必填，缺省返回明确提示）
//! - LLM_BASE_URL / llm_base_url （默认 https://api.deepseek.com/v1，OpenAI 兼容）
//! - LLM_MODEL  / llm_model      （默认 deepseek-chat）

use axum::{extract::State, Json};
use sea_orm::Statement;
use serde_json::{json, Value};

use crate::{
    ai,
    auth::{ensure, Auth},
    error::{ok, ApiError, ApiResult},
    state::AppState,
};

#[derive(serde::Deserialize)]
pub struct ChatMsg {
    pub role: String,
    pub content: String,
}

#[derive(serde::Deserialize)]
pub struct ChatReq {
    pub messages: Vec<ChatMsg>,
}

struct LlmCfg {
    base: String,
    key: String,
    model: String,
}

/// 配置读取：env 优先 → site_settings（llm_* 键）兜底
async fn llm_cfg(st: &AppState) -> Result<LlmCfg, ApiError> {
    let kv = site_kv(st).await;
    let pick = |env_key: &str, kv_key: &str| -> Option<String> {
        std::env::var(env_key)
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| kv.get(kv_key).cloned())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    };
    let key = pick("LLM_API_KEY", "llm_api_key").ok_or_else(|| {
        ApiError::bad("LLM 未配置：请设置环境变量 LLM_API_KEY，或在 设置 → 站点外观 配置 llm_api_key")
    })?;
    let base = pick("LLM_BASE_URL", "llm_base_url").unwrap_or_else(|| "https://api.deepseek.com/v1".into());
    let model = pick("LLM_MODEL", "llm_model").unwrap_or_else(|| "deepseek-chat".into());
    Ok(LlmCfg { base, key, model })
}

/// 读本租户 site_settings 的 llm_* 键
async fn site_kv(st: &AppState) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    let rows = st
        .db
        .query_all_statement(Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT key, value FROM site_settings WHERE tenant_id = '{}' AND key LIKE 'llm\\_%' ESCAPE '\\'",
                st.tenant.replace('\'', "''")
            ),
        ))
        .await
        .unwrap_or_default();
    for r in rows.iter() {
        if let (Ok(k), Ok(v)) = (r.try_get::<String>("", "key"), r.try_get::<String>("", "value")) {
            map.insert(k, v);
        }
    }
    map
}

/// tools：能力注册表 → OpenAI function calling schema
fn tools() -> Value {
    json!([
        {
            "type": "function",
            "function": {
                "name": "content.articles.draft",
                "description": "写一篇文章草稿（只存草稿，不发布）。用户想写文章、介绍、文案、公告时使用。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "topic": { "type": "string", "description": "文章主题，例如：桂花栗子欧包上市" },
                        "title": { "type": "string", "description": "可选，文章标题" }
                    },
                    "required": ["topic"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "content.articles.publish",
                "description": "发布一篇文章。当前用户无发布权限时会自动转成待审批请求，属正常流程。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "article_id": { "type": "string", "description": "要发布的文章 id，从系统提示里的文章清单中选取" }
                    },
                    "required": ["article_id"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "content.seo.optimize",
                "description": "为指定文章追加 SEO 关键词建议。",
                "parameters": {
                    "type": "object",
                    "properties": { "article_id": { "type": "string" } },
                    "required": ["article_id"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "content.translate",
                "description": "为指定文章追加英文翻译。",
                "parameters": {
                    "type": "object",
                    "properties": { "article_id": { "type": "string" } },
                    "required": ["article_id"]
                }
            }
        }
    ])
}

/// system prompt：身份 + 最近文章清单（供 LLM 选取 article_id）+ 行为规则
async fn system_prompt(st: &AppState) -> String {
    let rows = st
        .db
        .query_all_statement(Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT id, title, status FROM articles WHERE tenant_id = '{}' ORDER BY updated_at DESC LIMIT 20",
                st.tenant.replace('\'', "''")
            ),
        ))
        .await
        .unwrap_or_default();
    let mut list = String::new();
    for r in rows.iter() {
        let id = r.try_get::<String>("", "id").unwrap_or_default();
        let title = r.try_get::<String>("", "title").unwrap_or_default();
        let status = r.try_get::<String>("", "status").unwrap_or_default();
        list.push_str(&format!("- {id} | {status} | {title}\n"));
    }
    format!(
        "你是本站点的经营助手（AI-Native CMS）。当前登录者通过你管理网站内容。\n\
         可用工具见 tools；系统已注入最近 20 篇文章清单（id | 状态 | 标题）：\n{list}\n\
         规则：\n\
         1. 用户指令能用工具完成就调用工具；一次可调用多个工具。\n\
         2. 执行结果会以 tool 消息返回，请如实向用户汇报，不要编造结果。\n\
         3. 发布类操作若返回 needs_approval，告诉用户已提交审批、在 AI → Approvals 查看。\n\
         4. 闲聊或无法映射到工具的请求，直接自然语言回答，不调用工具。\n\
         5. 全程使用简体中文。"
    )
}

/// POST /api/ai/chat —— 对话主循环（最多 3 轮工具调用）
pub async fn chat(State(st): State<AppState>, auth: Auth, Json(req): Json<ChatReq>) -> ApiResult {
    ensure(&auth, "ai.tasks.view")?;
    let cfg = llm_cfg(&st).await?;

    let mut msgs: Vec<Value> = vec![json!({ "role": "system", "content": system_prompt(&st).await })];
    for m in req.messages.iter().rev().take(20).rev() {
        if m.role == "user" || m.role == "assistant" {
            msgs.push(json!({ "role": m.role, "content": m.content }));
        }
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| ApiError::bad(format!("HTTP client 初始化失败：{e}")))?;
    let url = format!("{}/chat/completions", cfg.base.trim_end_matches('/'));
    let mut steps: Vec<Value> = Vec::new();
    let mut reply = String::new();

    for round in 0..3 {
        let resp = client
            .post(&url)
            .bearer_auth(&cfg.key)
            .json(&json!({
                "model": cfg.model,
                "messages": msgs,
                "tools": tools(),
                "temperature": 0.4,
            }))
            .send()
            .await
            .map_err(|e| ApiError::bad(format!("LLM 请求失败：{e}")))?;
        let status = resp.status();
        let v: Value = resp
            .json()
            .await
            .map_err(|e| ApiError::bad(format!("LLM 响应解析失败（HTTP {status}）：{e}")))?;
        let msg = v.pointer("/choices/0/message").cloned().unwrap_or(Value::Null);
        let tool_calls = msg.get("tool_calls").and_then(|t| t.as_array()).cloned();

        match tool_calls {
            None => {
                reply = msg
                    .get("content")
                    .and_then(|c| c.as_str())
                    .unwrap_or("（模型未返回内容）")
                    .to_string();
                break;
            }
            Some(calls) => {
                if calls.is_empty() {
                    reply = msg
                        .get("content")
                        .and_then(|c| c.as_str())
                        .unwrap_or("（模型未返回内容）")
                        .to_string();
                    break;
                }
                // assistant 消息（含 tool_calls）必须原样回传，tool 消息才能对上 id
                msgs.push(msg.clone());
                for call in calls {
                    let fn_name = call.pointer("/function/name").and_then(|s| s.as_str()).unwrap_or("").to_string();
                    let args_raw = call.pointer("/function/arguments").and_then(|s| s.as_str()).unwrap_or("{}");
                    let input: Value = serde_json::from_str(args_raw).unwrap_or(json!({}));
                    let tool_call_id = call.get("id").and_then(|s| s.as_str()).unwrap_or("").to_string();

                    let result = match ai::run_capability(&st, &auth, &fn_name, &input).await {
                        Ok(r) => r,
                        Err(e) => json!({ "decision": "error", "message": e.message }),
                    };
                    steps.push(json!({
                        "capability": fn_name,
                        "decision": result.get("decision").cloned().unwrap_or(json!("unknown")),
                        "targetId": result.pointer("/result/targetId").cloned().unwrap_or(json!(null)),
                    }));
                    msgs.push(json!({
                        "role": "tool",
                        "tool_call_id": tool_call_id,
                        "content": result.to_string(),
                    }));
                }
                if round == 2 {
                    reply = "已执行相关操作，详情见执行步骤。（已达单次对话工具调用上限）".into();
                    break;
                }
            }
        }
    }

    ok(json!({ "reply": reply, "steps": steps }))
}
