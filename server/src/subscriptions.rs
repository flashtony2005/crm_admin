//! 付费订阅（Subscriptions）：Stripe Checkout 接入 + Webhook 落库 + 内容门槛。
//!
//! 配置：`STRIPE_SECRET_KEY` / `STRIPE_WEBHOOK_SECRET`。
//! **未配置时不再"进入测试模式直接发会员"**（那是 P0-2 资损路径），
//! 而是明确拒绝；演示必须显式 `STRIPE_DEMO_MODE=1` 且 `CMS_ENV != production`。

use axum::{extract::State, Json};
use hmac::{Hmac, Mac};
use sea_orm::Value as SqlValue;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::Sha256;
use uuid::Uuid;

use crate::{
    db::now_iso,
    members::MemberAuth,
    error::{ok, ApiError, ApiResult},
    state::AppState,
};
use crate::sub_state::{unix_to_iso, SubSnapshot, UpsertOutcome};

fn sval(s: String) -> SqlValue { SqlValue::String(Some(s)) }

type HmacSha256 = Hmac<Sha256>;

/// Stripe 签名时间容差（秒）。没有容差时，抓到任意一个历史合法请求即可无限重放。
const STRIPE_SIG_TOLERANCE_SECS: i64 = 300;

/// GET /api/public/tiers —— 公开套餐列表
pub async fn tiers(State(st): State<AppState>) -> ApiResult {
    let rows = st
        .db
        .query_all(
            "SELECT id, name, slug, description, price_monthly, price_yearly, features, active, \
                    stripe_price_id, stripe_price_yearly_id \
             FROM tiers WHERE tenant_id = ? AND active = 1 ORDER BY price_monthly ASC",
            vec![sval(st.tenant.clone())],
        )
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    let mut items: Vec<Value> = Vec::new();
    for r in &rows {
        items.push(json!({
            "id": r.try_get::<String>("", "id").map_err(internal)?,
            "name": r.try_get::<String>("", "name").unwrap_or_default(),
            "slug": r.try_get::<String>("", "slug").unwrap_or_default(),
            "description": r.try_get::<String>("", "description").unwrap_or_default(),
            "priceMonthly": r.try_get::<f64>("", "price_monthly").unwrap_or(0.0),
            "priceYearly": r.try_get::<f64>("", "price_yearly").unwrap_or(0.0),
            "features": r.try_get::<String>("", "features").unwrap_or_else(|_| "[]".into()),
            "active": r.try_get::<i64>("", "active").unwrap_or(1),
            // 只下发「该周期能否在线支付」这一个布尔量，**不下发 Price ID 本身**：
            // 前端需要它来决定按钮是否可用（否则用户点下去只能拿到 400），
            // 但 Price ID 属于运营配置，没有出现在公开响应里的理由。
            "onlineMonthly": !r.try_get::<String>("", "stripe_price_id").unwrap_or_default().trim().is_empty(),
            "onlineYearly": !r.try_get::<String>("", "stripe_price_yearly_id").unwrap_or_default().trim().is_empty(),
        }));
    }
    ok(json!(items))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
// ── 键名契约：camelCase + snake 别名 ────────────────────────────────
// 全站 JSON 契约是 camelCase（通用网关 / 响应体 / 前端 TS 接口都是）。
// 这些手写 DTO 早期按 snake_case 定义，而调用方一律传 camelCase ；
// serde 对**未知字段静默忽略**，于是形成两类后果：
//   · 必填字段 → Json 提取失败 → 422（调用方直接不可用，看得到）
//   · 可选字段 → 静默变 None（**看不到，但业务已经错了**）
// rename_all 把接受名对齐契约；alias 保留 snake 接受名，让
// 老客户端（浏览器里缓存的旧 JS）与既有测试不必同步改。
pub struct CheckoutReq {
    #[serde(alias = "tier_id")]
    pub tier_id: String,
    #[serde(default)]
    pub interval: Option<String>,
}

/// POST /api/public/checkout —— 会员创建 Stripe Checkout 会话
pub async fn checkout(State(st): State<AppState>, auth: MemberAuth, Json(req): Json<CheckoutReq>) -> ApiResult {
    let row = st
        .db
        .query_one(
            "SELECT id, slug, stripe_price_id, stripe_price_yearly_id FROM tiers \
             WHERE id = ? AND tenant_id = ? AND active = 1 LIMIT 1",
            vec![sval(req.tier_id.clone()), sval(st.tenant.clone())],
        )
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    let Some(r) = row else { return Err(ApiError::not_found("套餐不存在")) };
    let tier_id: String = r.try_get("", "id").map_err(internal)?;
    let tier_slug: String = r.try_get("", "slug").unwrap_or_default();
    let monthly_id: String = r.try_get("", "stripe_price_id").unwrap_or_default();
    let yearly_id: String = r.try_get("", "stripe_price_yearly_id").unwrap_or_default();

    // 周期 → 价格 → 天数，三者必须同源（同一次 interval 解析）。
    // 此前这里硬挡年付：tiers 只有一列 stripe_price_id，月/年会拿着**同一个**
    // Stripe 价格结算 —— 页面写「¥180/年」而实际按月扣款。硬挡比静默错收好，
    // 但年付只能走人工。现在年付有独立 Price 列（迁移 0016），在线年付可以真做：
    // 取价严格按周期，用户选年付就必须用年付价格，**缺价报错、绝不回落月价**。
    let iv = Interval::parse(req.interval.as_deref());
    let price_id = price_id_for(iv, &monthly_id, &yearly_id);

    let sk = std::env::var("STRIPE_SECRET_KEY").unwrap_or_default();
    let demo_flag = env_flag("STRIPE_DEMO_MODE");
    let is_prod = std::env::var("CMS_ENV").as_deref() == Ok("production");
    match decide_checkout(&sk, &price_id, demo_flag, is_prod, iv) {
        CheckoutMode::Live => {
            let client = reqwest::Client::new();
            let success = format!("{}/membership?thank=1", public_base(&st));
            let cancel = format!("{}/membership", public_base(&st));
            // P0-3 配套：把 member_id 写进**订阅对象的 metadata**。
            // 否则后续 `customer.subscription.*` / `invoice.*` 事件只带 customer 与
            // subscription id，没有 member 就定位不到人（阶段 1 的订阅状态机要靠它）。
            let params = vec![
                ("mode", "subscription".to_string()),
                ("client_reference_id", auth.0.sub.clone()),
                ("success_url", success),
                ("cancel_url", cancel),
                ("line_items[0][price]", price_id),
                ("line_items[0][quantity]", "1".to_string()),
                ("subscription_data[metadata][member_id]", auth.0.sub.clone()),
                // 周期与套餐写进 metadata：Webhook 只拿得到一个 session 对象，
                // 没有这两个键就不知道该给哪个 plan、给多少天 —— 那意味着
                // **钱收了但会员不发**（比发错更糟，用户直接投诉到支付渠道）。
                // session 级与 subscription 级**都写**：前者给
                // checkout.session.completed 用，后者给阶段 1 的订阅状态机用
                // （customer.subscription.* 事件只带订阅对象，读不到 session metadata）。
                ("metadata[interval]", iv.key().to_string()),
                ("metadata[tier_slug]", tier_slug.clone()),
                ("metadata[tier_id]", tier_id.clone()),
                ("subscription_data[metadata][interval]", iv.key().to_string()),
                ("subscription_data[metadata][tier_slug]", tier_slug.clone()),
            ];
            let resp = client
                .post("https://api.stripe.com/v1/checkout/sessions")
                .basic_auth(&sk, None::<&str>)
                .form(&params)
                .send()
                .await
                .map_err(|e| ApiError::bad(format!("Stripe 请求失败：{e}")))?;
            if !resp.status().is_success() {
                let txt = resp.text().await.unwrap_or_default();
                return Err(ApiError::bad(format!("Stripe 错误：{txt}")));
            }
            let j: Value = resp.json().await.map_err(|e| ApiError::bad(format!("解析失败：{e}")))?;
            let url = j.get("url").and_then(|u| u.as_str()).unwrap_or("").to_string();
            crate::webhooks_out::emit(&st, "subscription.checkout", json!({ "tierId": tier_id, "memberId": auth.0.sub, "interval": iv.key(), "days": iv.days() }));
            ok(json!({ "url": url, "testMode": false }))
        }
        // 演示模式：不真实扣费，直接置为已订阅。**必须显式开关 + 非生产**。
        CheckoutMode::Demo => {
            eprintln!("[warn] STRIPE_DEMO_MODE 生效：直接置为已订阅（无真实扣费，仅演示）");
            // 必须同时写 plan_expires_at：只改 plan 会让 plan_expires_at 留空，
            // 而订阅有效性判据是「plan != free 且（到期为空 或 未到期）」——
            // 空到期 = **永久有效**。演示一次就得到一个永不过期的会员，
            // 且因为该会员没有订单，在归因/对账里也看不出异常。
            // 天数按周期给：原先写死 30，年付在演示模式下只得到 30 天 ——
            // 演示路径与真实路径的差异，恰恰是最难在验收时发现的那类问题。
            crate::points::extend_plan(&st, &auth.0.sub, iv.days()).await?;
            st.db
                .execute(
                    "UPDATE members SET plan = ?, status = 1, updated_at = ? WHERE id = ? AND tenant_id = ?",
                    vec![sval(tier_slug.clone()), sval(now_iso()), sval(auth.0.sub.clone()), sval(st.tenant.clone())],
                )
                .await
                .map_err(|e| ApiError::bad(format!("更新失败：{e}")))?;
            crate::webhooks_out::emit(&st, "subscription.activated", json!({ "tier": tier_slug, "memberId": auth.0.sub, "interval": iv.key(), "days": iv.days(), "demo": true }));
            ok(json!({ "url": format!("{}/membership?thank=1&demo=1", public_base(&st)), "testMode": true }))
        }
        CheckoutMode::Denied(why) => Err(ApiError::bad(format!(
            "在线支付未开通（{why}），请联系管理员"
        ))),
    }
}

/// 环境变量开关：1 / true / on 视为开
fn env_flag(key: &str) -> bool {
    matches!(
        std::env::var(key).unwrap_or_default().as_str(),
        "1" | "true" | "on"
    )
}

/// 归一化后的计费周期。
///
/// **只认 `yearly`，其余（含缺省、未知值）一律月付** —— 与 `points::create_order`
/// 的判定同源。两条下单路径对同一入参必须给出同一个周期：一个是订单表里的
/// `plan_days`，一个是 Stripe Price 的计费周期，两边一旦不一致，
/// 就会出现「订单按年计价、订阅按月扣款」这类只有用户查账单才发现的问题。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Interval {
    Monthly,
    Yearly,
}

impl Interval {
    fn parse(raw: Option<&str>) -> Self {
        if raw == Some("yearly") {
            Self::Yearly
        } else {
            Self::Monthly
        }
    }

    /// 写进 Stripe metadata 的值，也是 Webhook 读回来判定天数的键
    fn key(self) -> &'static str {
        match self {
            Self::Monthly => "monthly",
            Self::Yearly => "yearly",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Monthly => "月付",
            Self::Yearly => "年付",
        }
    }

    /// 该周期对应的 Price 列名：报错信息要能直接指到后台那一个字段
    fn price_col(self) -> &'static str {
        match self {
            Self::Monthly => "stripe_price_id",
            Self::Yearly => "stripe_price_yearly_id",
        }
    }

    /// 发货天数，必须与所选 Price 的实际计费周期一致
    fn days(self) -> i64 {
        match self {
            Self::Monthly => 30,
            Self::Yearly => 365,
        }
    }
}

/// 按周期取 Stripe Price ID。
///
/// **年付缺价时返回空串，绝不回落到月价**：回落等于页面承诺 ¥180/年、
/// 实际按月扣款 —— 用户按年付费却每月被扣一次，是最坏的结果。
/// 空串会一路走到 `decide_checkout` 变成 400，并在信息里点名缺的是哪一列。
fn price_id_for(iv: Interval, monthly: &str, yearly: &str) -> String {
    match iv {
        Interval::Monthly => monthly.trim().to_string(),
        Interval::Yearly => yearly.trim().to_string(),
    }
}

/// Checkout 可用性决策（纯函数：不读环境，便于单测）
#[derive(Debug, PartialEq)]
enum CheckoutMode {
    /// 走真实 Stripe Checkout
    Live,
    /// 本地演示：直接置为已订阅（需显式开关且非生产）
    Demo,
    /// 拒绝，附原因
    Denied(String),
}

/// 只有"密钥齐全"才允许真实收款；否则要么是显式演示模式，要么**明确拒绝**。
///
/// 判定顺序有讲究：**先判 Live** —— 生产环境若残留一个 `STRIPE_DEMO_MODE=1`，
/// 必须走真实链路，不能被演示模式短路成"免费发会员"。
fn decide_checkout(
    secret_key: &str,
    price_id: &str,
    demo_flag: bool,
    is_production: bool,
    iv: Interval,
) -> CheckoutMode {
    if !secret_key.is_empty() && !price_id.is_empty() {
        return CheckoutMode::Live;
    }
    if demo_flag {
        if is_production {
            return CheckoutMode::Denied(
                "演示模式（STRIPE_DEMO_MODE）在 CMS_ENV=production 下被禁用".into(),
            );
        }
        return CheckoutMode::Demo;
    }
    CheckoutMode::Denied(if secret_key.is_empty() {
        "STRIPE_SECRET_KEY 未配置".into()
    } else {
        // 必须点名**该周期缺的那一列**：站长配了月价、漏了年价时，
        // 若只报「未配置 stripe_price_id」，他会去改一个本来就填好的字段。
        format!(
            "该套餐未配置{}的 Stripe 价格（后台「付费订阅」的 {} 字段）",
            iv.label(),
            iv.price_col()
        )
    })
}

/// POST /api/stripe/webhook —— Stripe 事件回调
///
/// P0-3 修复（此前：事件处理完即丢、无 event_id 去重、DB 写失败 `.ok()` 吞错后
/// 仍返 `{"received":true}` → Stripe 认为投递成功不再重投 → **事件永久丢失**）：
///   1. 验签加 **5 分钟时间容差**，拒绝重放旧包；
///   2. 事件先落 `webhook_events` 做 **event_id 去重**：INSERT 冲突即已处理，直接返 2xx；
///   3. **入库成功才返 2xx，处理失败返 5xx** —— 让 Stripe 按自己的退避策略重投；
///   4. 事件类型从只有 `checkout.session.completed` 扩到订阅变更 / 发票成功失败，
///      并全部交由 `sub_state` 状态机应用（阶段 1 已落地，见该模块头注释）。
pub async fn stripe_webhook(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    body: String,
) -> ApiResult {
    let secret = std::env::var("STRIPE_WEBHOOK_SECRET").unwrap_or_default();
    if !secret.is_empty() {
        let sig = headers
            .get("stripe-signature")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if !verify_stripe_sig(&secret, sig, &body, crate::auth::now_secs() as i64) {
            return Err(ApiError::unauthorized("签名校验失败或时间戳超出容差"));
        }
    }
    let evt: Value =
        serde_json::from_str(&body).map_err(|e| ApiError::bad(format!("JSON 解析失败：{e}")))?;
    let event_id = evt.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let etype = evt.get("type").and_then(|v| v.as_str()).unwrap_or("").to_string();
    if event_id.is_empty() {
        // 没有 id 就无法去重。**宁可让 Stripe 重投，也不静默丢弃**。
        return Err(ApiError::bad("Stripe 事件缺少 id，无法去重"));
    }

    // ── 幂等：先入库；唯一键冲突 = 这个事件已经收过了 ──
    let row_id = Uuid::new_v4().to_string();
    let now = now_iso();
    let inserted = st
        .db
        .execute(
            "INSERT INTO webhook_events \
             (id, tenant_id, provider, event_id, type, payload, status, attempts, last_error, applies_at, created_at, updated_at) \
             VALUES (?, ?, 'stripe', ?, ?, ?, 'received', 0, '', '', ?, ?)",
            vec![
                sval(row_id.clone()),
                sval(st.tenant.clone()),
                sval(event_id.clone()),
                sval(etype.clone()),
                sval(body.clone()),
                sval(now.clone()),
                sval(now.clone()),
            ],
        )
        .await;
    // ⚠️ **不能用 is_err() 判定“没插进去”**：
    // SQLite 后端唯一键冲突返回 Err，但 Turso 后端把语句错误吞成 0
    // （cmsdb.rs 现状：单 execute 错误被吞），冲突表现为 Ok(0)。
    // 两类后端统一的冲突信号是 **rows_affected == 0**
    // —— 与 CmsDb::exec_tx 的既有约定一致（“影响行数=0 即冲突”）。
    // 实测踩过：只判 is_err 时，Turso 模式下重复事件会被当成首次处理，
    // 出站 Webhook 与状态变更会重复执行一遍。
    if !insert_ok(&inserted) {
        // 若已存在同一 event_id，是重复投递，确认即可；否则是真故障 ——
        // 返 5xx 让 Stripe 重投，绝不能返 2xx 把事件丢掉。
        let dup = st
            .db
            .query_one(
                "SELECT id FROM webhook_events WHERE tenant_id = ? AND provider = 'stripe' AND event_id = ? LIMIT 1",
                vec![sval(st.tenant.clone()), sval(event_id.clone())],
            )
            .await
            .ok()
            .flatten();
        if dup.is_some() {
            return ok(json!({ "received": true, "duplicate": true, "eventId": event_id }));
        }
        return Err(ApiError::server("事件入库失败，请重试"));
    }

    // ── 应用事件 ──
    match apply_stripe_event(&st, &etype, &evt).await {
        Ok(applied) => {
            let (status, applies_at, note) = match applied {
                Applied::Done(n) => ("processed", now_iso(), n),
                Applied::PendingApply => (
                    "pending_apply",
                    String::new(),
                    "无法定位归属，待后续事件补齐或人工介入".to_string(),
                ),
                Applied::Ignored => ("ignored", String::new(), "未处理的事件类型".to_string()),
            };
            if let Err(e) = st
                .db
                .execute(
                    "UPDATE webhook_events SET status = ?, attempts = attempts + 1, last_error = '', \
                     applies_at = ?, updated_at = ? WHERE id = ? AND tenant_id = ?",
                    vec![
                        sval(status.to_string()),
                        sval(applies_at),
                        sval(now_iso()),
                        sval(row_id.clone()),
                        sval(st.tenant.clone()),
                    ],
                )
                .await
            {
                // 事件本身已应用，但状态没落准 → 不能声称"完全成功"。
                // 返 5xx 让 Stripe 重投；重投会命中 event_id 去重，不会重复发货。
                return Err(ApiError::server(format!("事件状态回写失败：{e}")));
            }
            ok(json!({
                "received": true,
                "eventId": event_id,
                "type": etype,
                "status": status,
                "note": note,
            }))
        }
        Err(e) => {
            // 落 failed + 返 5xx。此前这里吞掉错误仍返 2xx，事件就永久丢了。
            let _ = st
                .db
                .execute(
                    "UPDATE webhook_events SET status = 'failed', attempts = attempts + 1, \
                     last_error = ?, updated_at = ? WHERE id = ? AND tenant_id = ?",
                    vec![
                        sval(e.clone()),
                        sval(now_iso()),
                        sval(row_id.clone()),
                        sval(st.tenant.clone()),
                    ],
                )
                .await;
            Err(ApiError::server(format!("事件处理失败：{e}")))
        }
    }
}

/// 事件入库是否真的写进去了。
///
/// SQLite 后端冲突返回 Err，**Turso 后端把语句错误吞成 Ok(0)** ——
/// 两种都必须被判成「没插进去」。只判 is_err() 会让去重在 Turso 模式下静默失效。
fn insert_ok(written: &Result<u64, crate::cmsdb::DbErr>) -> bool {
    matches!(written, Ok(n) if *n > 0)
}

/// 事件应用结果
enum Applied {
    /// 已生效（说明文案含周期/天数，因此是 String 而不是 &'static str）
    Done(String),
    /// 已落库，但**无法定位归属**（事件体没带 member_id，本地也还没有该订阅行）。
    /// 不是失败：后续带 member 的事件会补齐，或留待人工排查。
    PendingApply,
    /// 不认识的事件类型
    Ignored,
}

/// 应用一个 Stripe 事件。`Err` 表示"没处理成功"，应当让 Stripe 重投。
///
/// 阶段 1（2026-09-28）之后，涉及周期的事件不再「只落库」：
///   · `checkout.session.completed` —— 回写会员状态 + 按 metadata 发货
///     （首期。此时还不知道 Stripe 的权威周期终点，所以按套餐周期估算）；
///   · `customer.subscription.*` —— 写订阅快照，再由状态机生效；
///   · `invoice.payment_succeeded` —— **续费发货就发生在这里**
///     （新的周期终点只在发票对象上）；
///   · `invoice.payment_failed` —— 只告警，状态等订阅事件（那才是权威来源）。
///
/// 所有分支的判据统一在 `crate::sub_state`：**看 `current_period_end`，
/// 不看收到了哪个事件**。这是「期末取消」与「立即取消」唯一分得开的办法 ——
/// 两者发的本来就是同一个事件类型。
async fn apply_stripe_event(st: &AppState, etype: &str, evt: &Value) -> Result<Applied, String> {
    match etype {
        "checkout.session.completed" => {
            let obj = evt.pointer("/data/object");
            let member_id = obj
                .and_then(|o| o.get("client_reference_id"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if member_id.is_empty() {
                // 没有 member 指认：事件已落库可人工排查，不算处理失败。
                return Ok(Applied::Done("缺少 client_reference_id，仅落库".into()));
            }
            let customer = obj
                .and_then(|o| o.get("customer"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            // 周期与套餐从会话 metadata 读回（checkout 建会话时写入）。
            let tier_slug = obj
                .and_then(|o| o.pointer("/metadata/tier_slug"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let iv = Interval::parse(
                obj.and_then(|o| o.pointer("/metadata/interval"))
                    .and_then(|v| v.as_str()),
            );
            st.db
                .execute(
                    "UPDATE members SET status = 1, \
                     stripe_customer_id = CASE WHEN ? = '' THEN stripe_customer_id ELSE ? END, \
                     updated_at = ? WHERE id = ? AND tenant_id = ?",
                    vec![
                        sval(customer.clone()),
                        sval(customer.clone()),
                        sval(now_iso()),
                        sval(member_id.clone()),
                        sval(st.tenant.clone()),
                    ],
                )
                .await
                .map_err(|e| format!("更新会员失败：{e}"))?;

            // ── 发货 ──
            // 这里必须真开通会员：Stripe Checkout 走的是真实扣款，只把 status 置 1
            // 而 plan 不动，等于**钱收了、会员没发**。原先只写 status/customer，
            // 把「周期落到 plan 上」推给还没落地的订阅状态机（阶段 1）——
            // 在真实收款链路上，等一个未来模块就是**持续的资损**。
            if tier_slug.is_empty() {
                // 没有套餐标识就不敢发货：不知道落到哪个 plan、也不知道给多少天。
                // 早期会话（本次改动前创建）没有 metadata，只能激活账号并留痕待人工。
                crate::webhooks_out::emit(st, "subscription.activated", json!({ "memberId": member_id }));
                return Ok(Applied::Done("缺少 metadata.tier_slug，仅激活账号（未发货）".into()));
            }
            // 会员可能已被删除：这不是"处理失败"，返 Err 会让 Stripe 一直重投这个事件。
            // 注意：本函数的 Err 通道是 String（要给 webhook_events.last_error 落库），
            // ApiError 没有 Display，必须显式取 message 转换。
            let exists = crate::points::member_row(st, &member_id)
                .await
                .map_err(|e| format!("查询会员失败：{}", e.message))?
                .is_some();
            if !exists {
                return Ok(Applied::Done("会员不存在，仅落库".into()));
            }
            let granted = crate::points::extend_plan(st, &member_id, iv.days())
                .await
                .map_err(|e| format!("延长会员失败：{}", e.message))?;
            // extend_plan 只保证"延长"，套餐名要落到具体 slug（free → 付费）
            st.db
                .execute(
                    "UPDATE members SET plan = ?, updated_at = ? WHERE id = ? AND tenant_id = ?",
                    vec![
                        sval(tier_slug.clone()),
                        sval(now_iso()),
                        sval(member_id.clone()),
                        sval(st.tenant.clone()),
                    ],
                )
                .await
                .map_err(|e| format!("更新套餐失败：{e}"))?;
            // ── 同时建一条本地订阅记录 ──
            // ⚠️ 这里**刻意不落 orders**：订单的幂等键是「订阅尾号 + 周期末」，
            // 而 checkout 拿到的周期末是按套餐天数估算的（session 对象不带周期终点），
            // 与 `invoice.payment_succeeded` 里的真实周期末往往差几秒到几天 ——
            // 两处都写就会落成**两条单，收入翻倍**。所以口径只留一个：
            // 记账一律由发票事件完成（那是钱到账的直接凭证，且带真实周期末）。
            // 这一处只负责**发货**与**建行**。
            //
            // 这是为了堵住「永久会员」的最后一条入口：若随后的
            // `customer.subscription.created` 丢包，而本地又**没有任何订阅行**，
            // 调度器的到期兜底就扫不到这个人 —— 他会一直有效下去，
            // 而 members 表自身完全自洽，对账时看不出异常。
            //
            // 这里的 period_end 是按套餐周期**估算**的（session 对象不带周期终点），
            // 所以用 `insert_if_absent`：**只在缺行时创建，绝不覆盖**。
            // 真实周期终点由 `customer.subscription.created` 覆盖进来。
            let sub_id = obj
                .and_then(|o| o.get("subscription"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if !sub_id.is_empty() {
                let now_dt = chrono::Utc::now();
                let iso = |d: chrono::DateTime<chrono::Utc>| {
                    d.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
                };
                let snap = SubSnapshot {
                    // clone：sub_id 后面日志里还要用（建行失败要报得出是哪个订阅）
                    stripe_subscription_id: sub_id.clone(),
                    stripe_customer_id: customer.clone(),
                    member_id: member_id.clone(),
                    tier_slug: tier_slug.clone(),
                    price_id: String::new(),
                    interval: iv.key().to_string(),
                    status: "active".to_string(),
                    current_period_end: iso(now_dt + chrono::Duration::days(iv.days())),
                    // None：session 对象不带这个字段，不能凭「没带」就写 false
                    cancel_at_period_end: None,
                    canceled_at: String::new(),
                    started_at: iso(now_dt),
                    event_at: evt.get("created").and_then(|v| v.as_i64()).unwrap_or(0),
                };
                match crate::sub_state::insert_if_absent(st, &snap).await {
                    Ok(true) => {
                        // 这次真的新建了行：把会员**实际拿到**的那份有效期记为
                        // 本订阅的授予（`granted`，不是 snap 里按天数估算的值 ——
                        // 若会员原本已有人工有效期，extend_plan 是叠加而非覆盖，
                        // 两者会不同）。漏记这一步，若随后的
                        // customer.subscription.created 丢包，到期兜底就会把
                        // members 上那份有效期当成"另有来源"而不敢降级 ——
                        // 又变回永久会员（见迁移 0018 的注释）。
                        if let Err(e) =
                            crate::sub_state::mark_granted(st, &sub_id, &granted).await
                        {
                            eprintln!("[warn] 记录订阅授予失败（{sub_id}）：{e}");
                        }
                    }
                    Ok(false) => {}
                    Err(e) => {
                        // 建行失败不能让整件事失败：会员已经发货了，
                        // 返 Err 只会让 Stripe 把整个 checkout 事件重投一遍。
                        eprintln!("[warn] 建订阅记录失败（{sub_id}）：{e}");
                    }
                }
            }
            crate::webhooks_out::emit(
                st,
                "subscription.activated",
                json!({
                    "memberId": member_id,
                    "tier": tier_slug,
                    "interval": iv.key(),
                    "days": iv.days(),
                }),
            );
            Ok(Applied::Done(format!(
                "会员已激活：{} / {} 天",
                tier_slug,
                iv.days()
            )))
        }
        // ── 订阅对象变更：写快照 → 由状态机生效 ──────────────────────
        // `deleted` 与 `updated` 走同一支是有意的：「期末取消」与「立即取消」
        // 发的都是 deleted，事件体长得一模一样，**只有 current_period_end 能区分**
        // （已过去 = 真结束；在未来 = 服务给到期末）。若按事件类型分成两支，
        // 必然有一半是错的。
        "customer.subscription.created"
        | "customer.subscription.updated"
        | "customer.subscription.deleted" => {
            let Some(obj) = evt.pointer("/data/object") else {
                return Ok(Applied::Done("事件缺少 subscription 对象，仅落库".into()));
            };
            let snap = subscription_snapshot(obj, evt);
            let sub_id = snap.stripe_subscription_id.clone();
            if sub_id.is_empty() {
                return Ok(Applied::Done("订阅对象无 id，仅落库".into()));
            }
            match crate::sub_state::upsert_snapshot(st, &snap).await? {
                // 旧事件后到：**必须忽略**，否则会把刚续上的周期覆盖回去，
                // 表现为「用户刚付了年费，页面又显示已过期」。
                UpsertOutcome::Stale => Ok(Applied::Done("旧事件（乱序到达），已忽略".into())),
                // 事件体没带 member_id 且本地也没有该订阅行 —— 定位不到人。
                // 不返 Err（会让 Stripe 无限重投），标 pending_apply 等补齐。
                UpsertOutcome::Orphan => Ok(Applied::PendingApply),
                UpsertOutcome::Applied => {
                    let a = crate::sub_state::settle(st, &sub_id, 0).await?;
                    Ok(Applied::Done(a.note))
                }
            }
        }
        // ── 发票：**续费发货就发生在这里** ────────────────────────────────
        // 年付会员第二年 Stripe 扣款成功，新的周期终点只在发票对象上。
        // 订阅对象上也会有（另一个事件），但发票是**钱到账的直接凭证**：
        // 先信它，订阅事件万一丢了，续费也不至于不发。
        "invoice.payment_succeeded" | "invoice.payment_failed" => {
            let Some(obj) = evt.pointer("/data/object") else {
                return Ok(Applied::Done("事件缺少 invoice 对象，仅落库".into()));
            };
            let sub_id = obj
                .get("subscription")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if sub_id.is_empty() {
                // 一次性付款（mode=payment）的发票没有 subscription，与订阅无关。
                return Ok(Applied::Done("发票无 subscription，与订阅无关".into()));
            }
            if etype == "invoice.payment_failed" {
                // **不动状态**：Stripe 紧接着会发带 status=past_due 的
                // customer.subscription.updated，那才是权威来源。这里只告警，
                // 让运营知道有人的卡刷不过去了（宽限期判定见 sub_state）。
                crate::webhooks_out::emit(
                    st,
                    "subscription.payment_failed",
                    json!({
                        "subscriptionId": sub_id,
                        "attemptCount": obj.get("attempt_count"),
                        "nextAttempt": obj.get("next_payment_attempt"),
                    }),
                );
                return Ok(Applied::Done("扣款失败，已告警（状态以订阅事件为准）".into()));
            }
            // 新周期终点：`lines.data[0].period.end`（Unix 秒）
            let period_end = unix_to_iso(obj.pointer("/lines/data/0/period/end"));
            let amount = obj.get("amount_paid").and_then(|v| v.as_i64()).unwrap_or(0);
            let event_at = evt.get("created").and_then(|v| v.as_i64()).unwrap_or(0);
            match crate::sub_state::touch_period(st, &sub_id, &period_end, Some("active"), event_at)
                .await?
            {
                UpsertOutcome::Stale => {
                    return Ok(Applied::Done("旧发票事件（乱序到达），已忽略".into()))
                }
                UpsertOutcome::Orphan => return Ok(Applied::PendingApply),
                UpsertOutcome::Applied => {}
            }
            let a = crate::sub_state::settle(st, &sub_id, amount).await?;
            Ok(Applied::Done(format!("{}；本次收款 {} 分", a.note, amount)))
        }
        // 其余不认识的类型留痕即可（不返错，避免 Stripe 无意义重投）。
        _ => Ok(Applied::Ignored),
    }
}

/// 从 `customer.subscription` 对象解析本地快照。
///
/// 几个字段的来源都有讲究：
///   · `member_id` **只在 metadata 上**（checkout 建会话时写入）——
///     拿不到它就无法把订阅归属到人；而 `invoice.*` 事件连这个都没有，
///     只能靠本地订阅行反查（这正是 0017 建表的原因之一）；
///   · `customer` 可能是字符串，也可能是 expand 之后的 `{id: ...}` 对象；
///   · 时间字段全是 Unix 秒，必须转成 RFC3339 —— 要与
///     `members.plan_expires_at` 用同一个比较口径。混用两种格式是最容易
///     埋进对账里的错：一边是 "2026-10-01T..."、一边是 "1800000000"，
///     字符串比较永远不按预期工作，而且**不会报任何错**。
fn subscription_snapshot(o: &Value, evt: &Value) -> SubSnapshot {
    let s = |p: &str| {
        o.pointer(p)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string()
    };
    let customer = {
        let direct = s("/customer");
        if direct.is_empty() {
            s("/customer/id")
        } else {
            direct
        }
    };
    SubSnapshot {
        stripe_subscription_id: s("/id"),
        stripe_customer_id: customer,
        member_id: s("/metadata/member_id"),
        tier_slug: s("/metadata/tier_slug"),
        price_id: s("/items/data/0/price/id"),
        interval: s("/metadata/interval"),
        // 原始状态串原样存 —— 后台看到的应该是 Stripe 的原话；
        // 归一化只发生在判定的那一刻（sub_state::SubStatus::parse）。
        status: s("/status"),
        current_period_end: unix_to_iso(o.get("current_period_end")),
        // 只有 subscription.updated 稳定带这个字段；没带 = None，
        // 让本地已有的「已安排期末取消」标记活下来。
        cancel_at_period_end: o.get("cancel_at_period_end").and_then(|v| v.as_bool()),
        canceled_at: unix_to_iso(o.get("canceled_at")),
        started_at: unix_to_iso(o.get("start_date")),
        event_at: evt.get("created").and_then(|v| v.as_i64()).unwrap_or(0),
    }
}

/// Stripe 签名校验：HMAC-SHA256("{t}.{body}", secret) == v1，且 t 在容差窗口内。
///
/// P0-3：此前**没有时间容差** —— 抓到一个历史合法请求就能无限重放。
fn verify_stripe_sig(secret: &str, sig_header: &str, body: &str, now: i64) -> bool {
    let t_val = match sig_header.strip_prefix("t=") {
        Some(rest) => rest.split(',').next().unwrap_or("").to_string(),
        None => return false,
    };
    let v1_val = match sig_header.split(',').find_map(|p| p.strip_prefix("v1=")) {
        Some(v) => v.to_string(),
        None => return false,
    };
    // 时间容差：Stripe 的 t 是秒级 Unix 时间戳
    let Ok(t) = t_val.parse::<i64>() else { return false };
    if (now - t).abs() > STRIPE_SIG_TOLERANCE_SECS {
        return false;
    }
    let mut mac = match HmacSha256::new_from_slice(secret.as_bytes()) {
        Ok(m) => m,
        Err(_) => return false,
    };
    mac.update(format!("{t_val}.{body}").as_bytes());
    let expected = hex::encode(mac.finalize().into_bytes());
    constant_time_eq(&expected, &v1_val)
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() { return false; }
    let mut diff = 0u8;
    for (x, y) in a.bytes().zip(b.bytes()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn public_base(_st: &AppState) -> String {
    std::env::var("PUBLIC_BASE_URL").unwrap_or_else(|_| "http://localhost:5188".into())
}

fn internal(e: impl std::fmt::Display) -> ApiError {
    ApiError::bad(format!("行解析失败：{e}"))
}


// ─────────────────────── 阶段 0 / 4 条 P0 的单测 ───────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// P0-2：只有密钥齐全才允许真实收款；演示模式必须显式开关 + 非生产
    #[test]
    fn checkout_mode_matrix() {
        // 密钥齐全 → 真实链路。**即使环境里残留演示开关也不能被短路**
        // （否则生产上一个历史遗留的 STRIPE_DEMO_MODE=1 就能免费发会员）
        let m = Interval::Monthly;
        assert_eq!(decide_checkout("sk_test", "price_1", false, false, m), CheckoutMode::Live);
        assert_eq!(decide_checkout("sk_test", "price_1", true, false, m), CheckoutMode::Live);
        assert_eq!(decide_checkout("sk_test", "price_1", true, true, m), CheckoutMode::Live);
        // 未配置 + 无演示开关 → 拒绝（此前是"静默标记为已订阅"）
        assert!(matches!(decide_checkout("", "price_1", false, false, m), CheckoutMode::Denied(_)));
        assert!(matches!(decide_checkout("sk_test", "", false, false, m), CheckoutMode::Denied(_)));
        // 演示开关 → 仅非生产放行
        assert_eq!(decide_checkout("", "", true, false, m), CheckoutMode::Demo);
        assert!(matches!(decide_checkout("", "", true, true, m), CheckoutMode::Denied(_)));
    }

    /// 拒绝时必须说清缺什么（运维才能定位，而不是一句"支付不可用"）
    #[test]
    fn denied_reason_is_actionable() {
        match decide_checkout("", "price_1", false, false, Interval::Monthly) {
            CheckoutMode::Denied(w) => assert!(w.contains("STRIPE_SECRET_KEY"), "实际：{w}"),
            other => panic!("应为拒绝，实际 {other:?}"),
        }
        match decide_checkout("sk_test", "", false, false, Interval::Monthly) {
            CheckoutMode::Denied(w) => assert!(w.contains("stripe_price_id"), "实际：{w}"),
            other => panic!("应为拒绝，实际 {other:?}"),
        }
        // 年付缺价必须点名**年付那一列**：只报 stripe_price_id 会让站长
        // 去改一个本来就填好的字段（月付列），年付永远是坏的。
        match decide_checkout("sk_test", "", false, false, Interval::Yearly) {
            CheckoutMode::Denied(w) => {
                assert!(w.contains("stripe_price_yearly_id"), "未点名字段：{w}");
                assert!(w.contains("年付"), "未指明周期：{w}");
            }
            other => panic!("应为拒绝，实际 {other:?}"),
        }
    }

    /// 年付在线支付的**核心不变量**：取价与天数只由 interval 决定，
    /// 且年付缺价时绝不回落月价。
    ///
    /// 回落是最坏的失败形态：页面写着「¥180/年」，实际用月价建了订阅会话，
    /// 用户以为买了一年、其实每月被扣一次 —— 且订单/会话里数字都自洽，
    /// 不查 Stripe 账单看不出来。
    #[test]
    fn yearly_never_falls_back_to_monthly_price() {
        assert_eq!(price_id_for(Interval::Monthly, "price_m", "price_y"), "price_m");
        assert_eq!(price_id_for(Interval::Yearly, "price_m", "price_y"), "price_y");
        assert_eq!(
            price_id_for(Interval::Yearly, "price_m", ""),
            "",
            "年付缺价必须为空串（→ 400），不得回落月价"
        );
        // 只配年价时月付同样不该拿到年价
        assert_eq!(price_id_for(Interval::Monthly, "", "price_y"), "");
        // 周期 → 天数：与 Stripe Price 的计费周期一致
        assert_eq!(Interval::Monthly.days(), 30);
        assert_eq!(Interval::Yearly.days(), 365);
    }

    /// 周期解析只认 yearly；未知值/缺省/大小写不一致一律按月付 ——
    /// 与 points::create_order 同源，避免两条下单路对同一入参给出不同周期。
    #[test]
    fn interval_parse_is_strict_but_safe() {
        assert_eq!(Interval::parse(Some("yearly")), Interval::Yearly);
        assert_eq!(Interval::parse(Some("monthly")), Interval::Monthly);
        assert_eq!(Interval::parse(None), Interval::Monthly);
        assert_eq!(Interval::parse(Some("")), Interval::Monthly);
        assert_eq!(Interval::parse(Some("YEARLY")), Interval::Monthly);
        assert_eq!(Interval::parse(Some("annual")), Interval::Monthly);
        // metadata 里写出去的键必须与 days/price_col 属于同一个周期
        assert_eq!(Interval::Yearly.key(), "yearly");
        assert_eq!(Interval::Yearly.price_col(), "stripe_price_yearly_id");
        assert_eq!(Interval::Monthly.key(), "monthly");
    }

    /// P0-3：签名必须有时间容差，否则抓到一个历史包就能无限重放
    #[test]
    fn stripe_sig_requires_fresh_timestamp() {
        let secret = "whsec_test";
        let body = r#"{"id":"evt_1"}"#;
        let now = 1_800_000_000i64;
        let sig = sign(secret, body, now);
        assert!(verify_stripe_sig(secret, &sig, body, now), "同一时刻应通过");
        assert!(verify_stripe_sig(secret, &sig, body, now + 60), "1 分钟内应通过");
        assert!(!verify_stripe_sig(secret, &sig, body, now + 3600), "超出容差应拒绝");
        assert!(!verify_stripe_sig(secret, &sig, body, now - 3600), "未来时间同样拒绝");
    }

    /// 改一个字节的 body、换密钥、头部格式不对，都必须失败
    #[test]
    fn stripe_sig_detects_tampering() {
        let now = 1_800_000_000i64;
        let sig = sign("whsec_test", "body-a", now);
        assert!(!verify_stripe_sig("whsec_test", &sig, "body-b", now), "改 body 应失败");
        assert!(!verify_stripe_sig("whsec_other", &sig, "body-a", now), "换密钥应失败");
        assert!(!verify_stripe_sig("whsec_test", "garbage", "body-a", now), "缺 t= 应失败");
        assert!(!verify_stripe_sig("whsec_test", "t=abc,v1=xx", "body-a", now), "t 非数字应失败");
        assert!(!verify_stripe_sig("whsec_test", &format!("t={now}"), "body-a", now), "缺 v1 应失败");
    }

    /// P0-3 回归：冲突信号必须同时覆盖 SQLite 的 Err 与 Turso 的 Ok(0)。
    /// （实测在 Turso 模式下只判 is_err 会让去重失效，重复事件被处理两次。）
    #[test]
    fn insert_ok_treats_zero_rows_as_conflict() {
        assert!(insert_ok(&Ok(1)));
        assert!(!insert_ok(&Ok(0)), "Turso 冲突表现为 Ok(0)，必须判成没插进去");
        assert!(!insert_ok(&Err(crate::cmsdb::DbErr("unique 冲突".into()))));
    }

    fn sign(secret: &str, body: &str, t: i64) -> String {
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(format!("{t}.{body}").as_bytes());
        format!("t={t},v1={}", hex::encode(mac.finalize().into_bytes()))
    }
}
