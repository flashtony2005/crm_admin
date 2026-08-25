//! 推送渠道（B1）：企业微信群机器人 Webhook。
//!
//! 选型说明：企微/钉钉/飞书的「群机器人」都是纯入站 Webhook——无需 OAuth，
//! 粘贴一个 URL 即可收消息，是国内小商家最低摩擦的推送通道。
//! 配置方式：在 Integrations 页连接 `wecom-bot`，API Key 填完整 Webhook 地址。
//! 消息为即发即忘（tokio::spawn），失败只记日志，绝不阻塞业务主链路。

use sea_orm::{Statement, Value as SqlValue};
use serde_json::json;

use crate::state::AppState;
use std::sync::OnceLock;

fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .expect("reqwest client")
    })
}

fn base_url() -> String {
    std::env::var("PUBLIC_BASE_URL").unwrap_or_else(|_| "http://localhost:5188".into())
}

/// 读取 wecom-bot 集成的 Webhook 地址（连接且已填 key 才有）
async fn webhook_url(st: &AppState) -> Option<String> {
    let sql = format!(
        "SELECT api_key FROM integrations WHERE tenant_id = '{}' AND key = 'wecom-bot' \
         AND connected = 1 AND api_key IS NOT NULL AND api_key != '' LIMIT 1",
        st.tenant
    );
    let row = st
        .db
        .query_one_statement(Statement::from_string(sea_orm::DatabaseBackend::Sqlite, sql))
        .await
        .ok()
        .flatten()?;
    row.try_get::<String>("", "api_key").ok()
}

/// 发送文本到企微群机器人；返回是否成功（errcode==0）
async fn send_text(webhook: &str, content: &str) -> bool {
    let body = json!({ "msgtype": "text", "text": { "content": content } });
    match client().post(webhook).json(&body).send().await {
        Ok(resp) => match resp.json::<serde_json::Value>().await {
            Ok(v) => v.get("errcode").and_then(|c| c.as_i64()) == Some(0),
            Err(_) => false,
        },
        Err(e) => {
            eprintln!("[notify] webhook 发送失败：{e}");
            false
        }
    }
}

fn sval(s: String) -> SqlValue {
    SqlValue::String(Some(s))
}

/// 通用文本推送（工作流 notify 步骤用）；未配置渠道则静默跳过
pub fn send_raw(st: &AppState, message: &str) {
    let st = st.clone();
    let msg = message.to_string();
    tokio::spawn(async move {
        if let Some(url) = webhook_url(&st).await {
            let _ = send_text(&url, &msg).await;
        }
    });
}

/// 审批产生 → 通知裁决者（Owner）一键直达 /m
pub fn approval_created(st: &AppState, title: &str) {
    let st = st.clone();
    let msg = format!(
        "【AI 工作台】有待审批请求\n「{title}」等待你批准。\n👉 打开 {}/m 一键批准/驳回",
        base_url()
    );
    tokio::spawn(async move {
        if let Some(url) = webhook_url(&st).await {
            let _ = send_text(&url, &msg).await;
        }
    });
}

/// 裁决完成 → 通知发起人结果
pub fn approval_decided(st: &AppState, status: &str, target: &str) {
    let st = st.clone();
    let verdict = if status == "approved" { "✅ 已批准" } else { "🚫 已驳回" };
    let msg = format!("【AI 工作台】审批结果\n{verdict}：「{target}」");
    tokio::spawn(async move {
        if let Some(url) = webhook_url(&st).await {
            let _ = send_text(&url, &msg).await;
        }
    });
}
