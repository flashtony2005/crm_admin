//! 创作者社区 P1：积分钱包 / 每日签到 / 兑码充值 / 积分买断 / 订单（人工确认收款）。
//!
//! 设计要点：
//! - 积分唯一事实源是 points_ledger 流水（余额 = SUM(delta)）；每次写入
//!   带 balance_after 快照便于对账。
//! - 邀请奖励延迟到账：好友**首次付费开通**（订单确认收款）后才给邀请人发
//!   points_invite_reward 积分，注册即发会被脚本刷穿。
//! - 订单确认收款走 `UPDATE ... WHERE status='pending'` 原子判定，
//!   双击/并发确认只会成功一次；发货紧随其后。
//! - 订阅有效期：members.plan != 'free' 且（plan_expires_at 为空或未到期）。

use axum::{extract::{Path, State}, Json};
use chrono::Duration;
use sea_orm::Value as SqlValue;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    auth::{ensure, Auth},
    db::now_iso,
    error::{ok, ApiError, ApiResult},
    members::MemberAuth,
    state::AppState,
};

fn sval(s: String) -> SqlValue {
    SqlValue::String(Some(s))
}
fn ival(n: i64) -> SqlValue {
    SqlValue::BigInt(Some(n))
}

fn internal(e: impl std::fmt::Display) -> ApiError {
    ApiError::bad(format!("行解析失败：{e}"))
}

/// 读整数型站点设置（site_settings KV），缺省/解析失败用默认值
async fn setting_i64(st: &AppState, key: &str, default: i64) -> i64 {
    st.db
        .query_one(
            "SELECT value FROM site_settings WHERE tenant_id = ? AND key = ? LIMIT 1",
            vec![sval(st.tenant.clone()), sval(key.to_string())],
        )
        .await
        .ok()
        .flatten()
        .and_then(|r| r.try_get::<String>("", "value").ok())
        .and_then(|v| v.trim().parse::<i64>().ok())
        .unwrap_or(default)
}

/// 当前余额 = SUM(delta)
async fn balance(st: &AppState, member_id: &str) -> Result<i64, ApiError> {
    let r = st
        .db
        .query_one(
            "SELECT COALESCE(SUM(delta), 0) AS bal FROM points_ledger WHERE tenant_id = ? AND member_id = ?",
            vec![sval(st.tenant.clone()), sval(member_id.to_string())],
        )
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    Ok(r.and_then(|x| x.try_get::<i64>("", "bal").ok()).unwrap_or(0))
}

/// 记一笔积分流水（balance_after = 现余额 + delta），返回新余额。
/// 供本模块与订单确认共用；balance_after 由服务端计算，前端传值无效。
/// 记一笔积分流水，返回记账后的余额。
///
/// F2 修复：原实现是 `balance()`（SELECT SUM）→ `INSERT` 的 read-then-write，
/// 并发记账时两个请求会读到同一个旧余额，算出并写入相同的 balance_after，
/// 导致流水快照与 SUM(delta) 永久不一致。
///
/// 改为**单条 `INSERT...SELECT`**：余额在库内由子查询算出，单条语句天然原子
/// （SQLite 单条写语句持有写锁；Turso 单语句亦原子）。插入后用已知 id 回读快照。
pub async fn add_ledger(
    st: &AppState,
    member_id: &str,
    delta: i64,
    reason: &str,
    ref_id: &str,
    note: &str,
) -> Result<i64, ApiError> {
    let id = Uuid::new_v4().to_string();
    let now = now_iso();
    // 参数顺序：id, tenant, member, delta, [子查询]tenant, member, delta, reason, ref_id, note, created_at
    let n = st
        .db
        .execute(
            "INSERT INTO points_ledger \
             (id, tenant_id, member_id, delta, balance_after, reason, ref_id, note, created_at) \
             SELECT ?, ?, ?, ?, \
                    (SELECT COALESCE(SUM(delta), 0) FROM points_ledger \
                      WHERE tenant_id = ? AND member_id = ?) + ?, \
                    ?, ?, ?, ?",
            vec![
                sval(id.clone()),
                sval(st.tenant.clone()),
                sval(member_id.to_string()),
                ival(delta),
                sval(st.tenant.clone()),
                sval(member_id.to_string()),
                ival(delta),
                sval(reason.to_string()),
                sval(ref_id.to_string()),
                sval(note.to_string()),
                sval(now),
            ],
        )
        .await
        .map_err(|e| ApiError::bad(format!("记账失败：{e}")))?;
    if n == 0 {
        return Err(ApiError::bad("记账失败：未写入流水"));
    }
    // 回读本条快照（id 由本地生成，唯一，不受并发影响）
    let row = st
        .db
        .query_one(
            "SELECT balance_after FROM points_ledger WHERE id = ? LIMIT 1",
            vec![sval(id)],
        )
        .await
        .map_err(|e| ApiError::bad(format!("读取余额失败：{e}")))?;
    let bal = match row {
        Some(r) => r.try_get::<i64>("", "balance_after").unwrap_or(delta),
        None => delta,
    };
    Ok(bal)
}

/// 查询会员行（plan / plan_expires_at / invited_by）
async fn member_row(
    st: &AppState,
    member_id: &str,
) -> Result<Option<(String, String, String)>, ApiError> {
    let r = st
        .db
        .query_one(
            "SELECT plan, plan_expires_at, invited_by FROM members WHERE id = ? AND tenant_id = ? LIMIT 1",
            vec![sval(member_id.to_string()), sval(st.tenant.clone())],
        )
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    Ok(r.map(|x| {
        (
            x.try_get("", "plan").unwrap_or_else(|_| "free".into()),
            x.try_get("", "plan_expires_at").unwrap_or_default(),
            x.try_get("", "invited_by").unwrap_or_default(),
        )
    }))
}

/// GET /api/public/members/wallet —— 余额 + 最近流水 + 订阅状态
pub async fn wallet(State(st): State<AppState>, auth: MemberAuth) -> ApiResult {
    let mid = auth.0.sub.clone();
    let bal = balance(&st, &mid).await?;
    let m = member_row(&st, &mid).await?;
    let (plan, plan_expires_at, invited_by) = m.unwrap_or_else(|| ("free".into(), String::new(), String::new()));
    let rows = st
        .db
        .query_all(
            "SELECT delta, balance_after, reason, ref_id, note, created_at FROM points_ledger \
             WHERE tenant_id = ? AND member_id = ? ORDER BY created_at DESC LIMIT 50",
            vec![sval(st.tenant.clone()), sval(mid.clone())],
        )
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    let ledger: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "delta": r.try_get::<i64>("", "delta").unwrap_or(0),
                "balanceAfter": r.try_get::<i64>("", "balance_after").unwrap_or(0),
                "reason": r.try_get::<String>("", "reason").unwrap_or_default(),
                "refId": r.try_get::<String>("", "ref_id").unwrap_or_default(),
                "note": r.try_get::<String>("", "note").unwrap_or_default(),
                "createdAt": r.try_get::<String>("", "created_at").unwrap_or_default(),
            })
        })
        .collect();
    // 今日是否已签到（ref_id = 当天日期）
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let signed_today = st
        .db
        .query_one(
            "SELECT id FROM points_ledger WHERE tenant_id = ? AND member_id = ? AND reason = 'signin' AND ref_id = ? LIMIT 1",
            vec![sval(st.tenant.clone()), sval(mid.clone()), sval(today.clone())],
        )
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?
        .is_some();
    // 到期提醒：付费计划剩余天数（-1 = 免费/无期限/解析失败）
    // planExpiring：7 天内到期；planExpired：到期时间已过（subscribed() 已视为无效）
    let days_left: i64 = if plan == "free" || plan_expires_at.is_empty() {
        -1
    } else {
        chrono::DateTime::parse_from_rfc3339(&plan_expires_at)
            .map(|d| (d.with_timezone(&chrono::Utc) - chrono::Utc::now()).num_days())
            .unwrap_or(-1)
    };
    ok(json!({
        "balance": bal,
        "plan": plan,
        "planExpiresAt": plan_expires_at,
        "planDaysLeft": days_left,
        "planExpiring": plan != "free" && days_left >= 0 && days_left <= 7,
        "planExpired": plan != "free" && !plan_expires_at.is_empty() && days_left < 0,
        "invited": !invited_by.is_empty(),
        "signedToday": signed_today,
        "signinPoints": setting_i64(&st, "points_signin", 10).await,
        "inviteRewardPoints": setting_i64(&st, "points_invite_reward", 100).await,
        "ledger": ledger,
    }))
}

/// POST /api/public/members/signin —— 每日签到（每天一次，积分可在后台配置）
pub async fn signin(State(st): State<AppState>, auth: MemberAuth) -> ApiResult {
    let mid = auth.0.sub.clone();
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let dup = st
        .db
        .query_one(
            "SELECT id FROM points_ledger WHERE tenant_id = ? AND member_id = ? AND reason = 'signin' AND ref_id = ? LIMIT 1",
            vec![sval(st.tenant.clone()), sval(mid.clone()), sval(today.clone())],
        )
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    if dup.is_some() {
        return Err(ApiError::bad("今天已签到，明天再来"));
    }
    let pts = setting_i64(&st, "points_signin", 10).await;
    let bal = add_ledger(&st, &mid, pts, "signin", &today, "每日签到").await?;
    ok(json!({ "ok": true, "delta": pts, "balance": bal }))
}

#[derive(Deserialize)]
pub struct RedeemReq {
    pub code: String,
}

/// POST /api/public/members/redeem —— 兑码充值（kind: points=积分 / plan_days=会员天数）
pub async fn redeem(State(st): State<AppState>, auth: MemberAuth, Json(req): Json<RedeemReq>) -> ApiResult {
    let mid = auth.0.sub.clone();
    let code = req.code.trim().to_string();
    if code.is_empty() {
        return Err(ApiError::bad("请填写兑换码"));
    }
    let row = st
        .db
        .query_one(
            "SELECT id, kind, value, status FROM redeem_codes WHERE tenant_id = ? AND code = ? LIMIT 1",
            vec![sval(st.tenant.clone()), sval(code.clone())],
        )
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    let Some(r) = row else { return Err(ApiError::bad("兑换码无效")) };
    let id: String = r.try_get("", "id").map_err(internal)?;
    let kind: String = r.try_get("", "kind").unwrap_or_else(|_| "points".into());
    let value: i64 = r.try_get::<i64>("", "value").unwrap_or(0);
    let status: String = r.try_get("", "status").unwrap_or_else(|_| "unused".into());
    if status != "unused" {
        return Err(ApiError::bad("该兑换码已被使用"));
    }
    if value <= 0 {
        return Err(ApiError::bad("兑换码面值无效"));
    }
    // 原子核销：并发使用同一码只有一次成功
    let now = now_iso();
    let n = st
        .db
        .execute(
            "UPDATE redeem_codes SET status = 'used', used_by = ?, used_at = ?, updated_at = ? \
             WHERE id = ? AND status = 'unused'",
            vec![sval(mid.clone()), sval(now.clone()), sval(now.clone()), sval(id.clone())],
        )
        .await
        .map_err(|e| ApiError::bad(format!("核销失败：{e}")))?;
    if n == 0 {
        return Err(ApiError::bad("该兑换码已被使用"));
    }

    match kind.as_str() {
        "points" => {
            let bal = add_ledger(&st, &mid, value, "redeem", &id, &format!("兑换码 {code}")).await?;
            ok(json!({ "ok": true, "kind": "points", "delta": value, "balance": bal }))
        }
        "plan_days" => {
            extend_plan(&st, &mid, value).await?;
            ok(json!({ "ok": true, "kind": "plan_days", "days": value }))
        }
        _ => Err(ApiError::bad("兑换码类型无效")),
    }
}

/// 会员有效期延长 value 天：base = max(now, 现有到期时间)；plan 若为 free 则置 'paid'。
async fn extend_plan(st: &AppState, member_id: &str, days: i64) -> Result<(), ApiError> {
    let (plan, exp, _) = member_row(st, member_id)
        .await?
        .ok_or_else(|| ApiError::not_found("会员不存在"))?;
    let now = chrono::Utc::now();
    let base = if exp.is_empty() {
        now
    } else {
        chrono::DateTime::parse_from_rfc3339(&exp)
            .map(|d| d.with_timezone(&chrono::Utc))
            .unwrap_or(now)
            .max(now)
    };
    let new_exp = (base + Duration::days(days))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let new_plan = if plan == "free" { "paid".to_string() } else { plan };
    st.db
        .execute(
            "UPDATE members SET plan = ?, plan_expires_at = ?, updated_at = ? WHERE id = ? AND tenant_id = ?",
            vec![
                sval(new_plan),
                sval(new_exp),
                sval(now_iso()),
                sval(member_id.to_string()),
                sval(st.tenant.clone()),
            ],
        )
        .await
        .map_err(|e| ApiError::bad(format!("更新失败：{e}")))?;
    Ok(())
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
pub struct PurchaseReq {
    #[serde(alias = "article_id")]
    pub article_id: String,
}

/// POST /api/public/members/purchase-article —— 积分买断单篇文章（原子扣减）
pub async fn purchase_article(
    State(st): State<AppState>,
    auth: MemberAuth,
    Json(req): Json<PurchaseReq>,
) -> ApiResult {
    let mid = auth.0.sub.clone();
    let aid = req.article_id.trim().to_string();
    let row = st
        .db
        .query_one(
            "SELECT id, paid_level, price_points FROM articles \
             WHERE tenant_id = ? AND status = 'published' AND (id = ? OR slug = ?) LIMIT 1",
            vec![
                sval(st.tenant.clone()),
                sval(aid.clone()),
                sval(aid.clone()),
            ],
        )
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    let Some(r) = row else { return Err(ApiError::not_found("文章不存在或未发布")) };
    let id: String = r.try_get("", "id").map_err(internal)?;
    let paid_level: i64 = r.try_get::<i64>("", "paid_level").unwrap_or(0);
    let price: i64 = r.try_get::<i64>("", "price_points").unwrap_or(0);
    if paid_level != 2 || price <= 0 {
        return Err(ApiError::bad("该文章不支持积分解锁"));
    }
    // 幂等：已买断直接返回成功
    let owned = st
        .db
        .query_one(
            "SELECT id FROM points_ledger WHERE tenant_id = ? AND member_id = ? AND reason = 'purchase' AND ref_id = ? LIMIT 1",
            vec![sval(st.tenant.clone()), sval(mid.clone()), sval(id.clone())],
        )
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    if owned.is_some() {
        return ok(json!({ "ok": true, "alreadyOwned": true }));
    }
    // F2 修复：原实现「先查余额 → 判断 → 再扣减」是典型 TOCTOU，并发下两个请求
    // 都读到足够余额、都通过校验、都扣减，最终余额可为负。
    // 改为单条 INSERT...SELECT：余额门槛与扣减在同一条语句内完成，
    // 影响行数 = 0 即代表「已购买」或「余额不足」，再回查细分错误原因。
    let lid = Uuid::new_v4().to_string();
    let now = now_iso();
    let n = st
        .db
        .execute(
            "INSERT INTO points_ledger \
             (id, tenant_id, member_id, delta, balance_after, reason, ref_id, note, created_at) \
             SELECT ?, ?, ?, -?, \
                    (SELECT COALESCE(SUM(delta), 0) FROM points_ledger \
                      WHERE tenant_id = ? AND member_id = ?) - ?, \
                    'purchase', ?, ?, ? \
             WHERE (SELECT COALESCE(SUM(delta), 0) FROM points_ledger \
                     WHERE tenant_id = ? AND member_id = ?) >= ? \
               AND NOT EXISTS (SELECT 1 FROM points_ledger \
                     WHERE tenant_id = ? AND member_id = ? AND reason = 'purchase' AND ref_id = ?)",
            vec![
                sval(lid.clone()),
                sval(st.tenant.clone()),
                sval(mid.clone()),
                ival(price),          // delta = -price
                sval(st.tenant.clone()),
                sval(mid.clone()),
                ival(price),          // balance_after
                sval(id.clone()),     // ref_id
                sval("积分解锁文章".to_string()),
                sval(now),
                sval(st.tenant.clone()),
                sval(mid.clone()),
                ival(price),          // 余额门槛
                sval(st.tenant.clone()),
                sval(mid.clone()),
                sval(id.clone()),
            ],
        )
        .await
        .map_err(|e| ApiError::bad(format!("购买失败：{e}")))?;

    if n == 0 {
        // 未插入：区分「已买断」与「余额不足」给出准确提示
        let owned_now = st
            .db
            .query_one(
                "SELECT id FROM points_ledger WHERE tenant_id = ? AND member_id = ? AND reason = 'purchase' AND ref_id = ? LIMIT 1",
                vec![sval(st.tenant.clone()), sval(mid.clone()), sval(id.clone())],
            )
            .await
            .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
        if owned_now.is_some() {
            return ok(json!({ "ok": true, "alreadyOwned": true }));
        }
        let bal = balance(&st, &mid).await?;
        return Err(ApiError::bad(format!("积分不足：需要 {price}，当前 {bal}")));
    }

    let new_bal = st
        .db
        .query_one(
            "SELECT balance_after FROM points_ledger WHERE id = ? LIMIT 1",
            vec![sval(lid)],
        )
        .await
        .map_err(|e| ApiError::bad(format!("读取余额失败：{e}")))?
        .and_then(|r| r.try_get::<i64>("", "balance_after").ok())
        .unwrap_or(0);
    ok(json!({ "ok": true, "spent": price, "balance": new_bal }))
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
pub struct CreateOrderReq {
    /// points_recharge 充积分 | plan 开通订阅（按 tier 计价）
    #[serde(alias = "biz_type")]
    pub biz_type: String,
    #[serde(default)]
    pub points: Option<i64>,
    #[serde(default, alias = "tier_id")]
    pub tier_id: Option<String>,
    /// 支付渠道：manual（默认，人工确认收款）| wechat（微信 Native 扫码，P2）
    #[serde(default)]
    pub channel: Option<String>,
}

/// POST /api/public/orders —— 创建人工确认收款订单
/// （channel=manual；P2 接在线支付后加 channel=wechat 分支，业务表不变）
pub async fn create_order(
    State(st): State<AppState>,
    auth: MemberAuth,
    Json(req): Json<CreateOrderReq>,
) -> ApiResult {
    let mid = auth.0.sub.clone();
    // 渠道：wechat = 微信 Native 扫码（P2 在线支付）；其余一律 manual（人工确认收款）
    let channel = if req.channel.as_deref() == Some("wechat") { "wechat" } else { "manual" };
    let mut wechat_cfg: Option<crate::wechat_pay::PayConfig> = None;
    if channel == "wechat" {
        let c = crate::wechat_pay::load_config(&st).await?;
        let miss = crate::wechat_pay::missing_fields(&c);
        if !miss.is_empty() {
            return Err(ApiError::bad(format!(
                "微信支付未配置，缺少：{}（请站长在后台「订单 → 微信支付配置」填写）",
                miss.join("、")
            )));
        }
        wechat_cfg = Some(c);
    }
    let (points, plan_days, amount_cents, tier_id) = match req.biz_type.as_str() {
        "points_recharge" => {
            let p = req.points.unwrap_or(0);
            if p <= 0 || p > 1_000_000 {
                return Err(ApiError::bad("充值积分数无效（1-1000000）"));
            }
            // 价目：1 积分 = 1 分（P1 简化定价；P2 价目表化）
            (p, 0, p, String::new())
        }
        "plan" => {
            let tid = req.tier_id.clone().unwrap_or_default();
            let row = st
                .db
                .query_one(
                    "SELECT slug, price_monthly FROM tiers WHERE id = ? AND tenant_id = ? AND active = 1 LIMIT 1",
                    vec![sval(tid.clone()), sval(st.tenant.clone())],
                )
                .await
                .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
            let Some(t) = row else { return Err(ApiError::not_found("套餐不存在")) };
            let slug: String = t.try_get("", "slug").unwrap_or_default();
            let price: f64 = t.try_get::<f64>("", "price_monthly").unwrap_or(0.0);
            (0, 30, (price * 100.0).round() as i64, slug)
        }
        _ => return Err(ApiError::bad("biz_type 无效")),
    };

    let id = Uuid::new_v4().to_string();
    let order_no = format!("PO{}", Uuid::new_v4().simple());
    let now = now_iso();
    st.db
        .execute(
            "INSERT INTO orders (id, tenant_id, order_no, member_id, biz_type, tier_id, points, plan_days, \
             amount_cents, channel, status, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending', ?, ?)",
            vec![
                sval(id.clone()),
                sval(st.tenant.clone()),
                sval(order_no.clone()),
                sval(mid.clone()),
                sval(req.biz_type.clone()),
                sval(tier_id.clone()),
                ival(points),
                ival(plan_days),
                ival(amount_cents),
                sval(channel.to_string()),
                sval(now.clone()),
                sval(now.clone()),
            ],
        )
        .await
        .map_err(|e| ApiError::bad(format!("创建订单失败：{e}")))?;

    // 微信 Native 扫码：调 API 拿 code_url + 二维码；失败则关闭订单防悬挂
    if let Some(cfg) = wechat_cfg {
        let desc = match req.biz_type.as_str() {
            "points_recharge" => format!("积分充值 {} 点", points),
            _ => format!("会员订阅 {} 天", plan_days.max(30)),
        };
        match crate::wechat_pay::create_native_order(&st, &cfg, &order_no, amount_cents, &desc).await {
            Ok(code_url) => {
                let qr_svg = crate::wechat_pay::qr_svg(&code_url)?;
                crate::webhooks_out::emit(&st, "order.created", json!({ "orderNo": order_no, "memberId": mid, "bizType": req.biz_type, "channel": "wechat" }));
                return ok(json!({
                    "orderNo": order_no,
                    "amountCents": amount_cents,
                    "channel": "wechat",
                    "codeUrl": code_url,
                    "qrSvg": qr_svg,
                    "hint": "请使用微信扫码完成支付，支付成功后自动到账。",
                }));
            }
            Err(e) => {
                let _ = st
                    .db
                    .execute(
                        "UPDATE orders SET status = 'closed', updated_at = ? WHERE id = ? AND tenant_id = ? AND status = 'pending'",
                        vec![sval(now_iso()), sval(id.clone()), sval(st.tenant.clone())],
                    )
                    .await;
                return Err(e);
            }
        }
    }

    crate::webhooks_out::emit(&st, "order.created", json!({ "orderNo": order_no, "memberId": mid, "bizType": req.biz_type, "channel": "manual" }));
    ok(json!({
        "orderNo": order_no,
        "amountCents": amount_cents,
        "channel": "manual",
        "hint": "请按收款码完成转账，并在转账备注填写该订单号；站长确认收款后自动到账。",
    }))
}

/// POST /api/admin/orders/{id}/confirm —— 人工确认收款（原子防重复），确认后发货：
/// - points_recharge：给买家 +points 积分
/// - plan：开通/延长订阅 30 天（plan 置套餐 slug；free 则成为付费会员）
/// 首次付费发货后，若买家是邀请注册且邀请人未领过奖励 → 补发邀请积分。
/// 发货逻辑统一在 fulfill_order（微信回调走同一条链路）。
pub async fn confirm_order(
    State(st): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
) -> ApiResult {
    ensure(&auth, "content.members.update")?;
    let now = now_iso();
    // 原子确认：只有 pending → paid 成功一次
    let n = st
        .db
        .execute(
            "UPDATE orders SET status = 'paid', paid_at = ?, updated_at = ? \
             WHERE id = ? AND tenant_id = ? AND status = 'pending'",
            vec![sval(now.clone()), sval(now.clone()), sval(id.clone()), sval(st.tenant.clone())],
        )
        .await
        .map_err(|e| ApiError::bad(format!("确认失败：{e}")))?;
    if n == 0 {
        return Err(ApiError::bad("订单不存在或已处理"));
    }
    let (order_no, invite_reward) = fulfill_order(&st, &id).await?;
    ok(json!({
        "ok": true,
        "orderNo": order_no,
        "inviteReward": invite_reward,
    }))
}

// ── 发货 outbox（F2：杜绝「已扣款未发货」）─────────────────────────
// fulfill_order 的每个动作先占位再执行：占位用 UNIQUE(tenant_id, order_id, stage_key)
// 保证同一单同一动作只会执行一次；执行成功标 done，失败留 failed + 错误信息。
// 中途崩溃/异常不会丢失已置为 paid 的订单，由 retry_fulfill 重放补齐。

/// 占位：返回 true 表示该阶段此前已完成（调用方应跳过，幂等）
async fn stage_claimed(
    st: &AppState,
    order_id: &str,
    order_no: &str,
    stage_key: &str,
    stage: &str,
) -> Result<bool, ApiError> {
    let now = now_iso();
    let n = st
        .db
        .execute(
            "INSERT OR IGNORE INTO order_fulfill_log \
             (id, tenant_id, order_id, order_no, stage_key, stage, status, attempts, last_error, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, 'pending', 0, '', ?, ?)",
            vec![
                sval(Uuid::new_v4().to_string()),
                sval(st.tenant.clone()),
                sval(order_id.to_string()),
                sval(order_no.to_string()),
                sval(stage_key.to_string()),
                sval(stage.to_string()),
                sval(now.clone()),
                sval(now),
            ],
        )
        .await
        .map_err(|e| ApiError::bad(format!("发货占位失败：{e}")))?;
    if n > 0 {
        return Ok(false); // 新占位，需要执行
    }
    let row = st
        .db
        .query_one(
            "SELECT status FROM order_fulfill_log WHERE tenant_id = ? AND order_id = ? AND stage_key = ? LIMIT 1",
            vec![
                sval(st.tenant.clone()),
                sval(order_id.to_string()),
                sval(stage_key.to_string()),
            ],
        )
        .await
        .map_err(|e| ApiError::bad(format!("查询发货状态失败：{e}")))?;
    Ok(row
        .and_then(|r| r.try_get::<String>("", "status").ok())
        .map(|x| x == "done")
        .unwrap_or(false))
}

async fn stage_done(st: &AppState, order_id: &str, stage_key: &str) -> Result<(), ApiError> {
    st.db
        .execute(
            "UPDATE order_fulfill_log SET status = 'done', attempts = attempts + 1, \
             last_error = '', updated_at = ? \
             WHERE tenant_id = ? AND order_id = ? AND stage_key = ?",
            vec![
                sval(now_iso()),
                sval(st.tenant.clone()),
                sval(order_id.to_string()),
                sval(stage_key.to_string()),
            ],
        )
        .await
        .map_err(|e| ApiError::bad(format!("更新发货状态失败：{e}")))?;
    Ok(())
}

async fn stage_failed(
    st: &AppState,
    order_id: &str,
    stage_key: &str,
    err: &str,
) -> Result<(), ApiError> {
    st.db
        .execute(
            "UPDATE order_fulfill_log SET status = 'failed', attempts = attempts + 1, \
             last_error = ?, updated_at = ? \
             WHERE tenant_id = ? AND order_id = ? AND stage_key = ?",
            vec![
                sval(err.to_string()),
                sval(now_iso()),
                sval(st.tenant.clone()),
                sval(order_id.to_string()),
                sval(stage_key.to_string()),
            ],
        )
        .await
        .map_err(|e| ApiError::bad(format!("更新发货状态失败：{e}")))?;
    Ok(())
}

/// 订单发货（幂等由调用方的「pending → paid」原子转换保证，每单只会执行一次）：
/// - points_recharge：买家加 points 积分
/// - plan：extend_plan 延长有效期 + plan 置套餐 slug
/// - 尾随：好友首次付费 → 邀请人补发奖励（每好友一次，按 ledger 去重）
/// 返回 (order_no, 邀请奖励信息)。
pub async fn fulfill_order(
    st: &AppState,
    order_id: &str,
) -> Result<(String, Option<Value>), ApiError> {
    let row = st
        .db
        .query_one(
            "SELECT order_no, member_id, biz_type, tier_id, points, plan_days FROM orders \
             WHERE id = ? AND tenant_id = ? LIMIT 1",
            vec![sval(order_id.to_string()), sval(st.tenant.clone())],
        )
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    let Some(r) = row else { return Err(ApiError::not_found("订单不存在")) };
    let order_no: String = r.try_get("", "order_no").map_err(internal)?;
    let member_id: String = r.try_get("", "member_id").map_err(internal)?;
    let biz_type: String = r.try_get("", "biz_type").unwrap_or_default();
    let tier_id: String = r.try_get("", "tier_id").unwrap_or_default();
    let points: i64 = r.try_get::<i64>("", "points").unwrap_or(0);
    let plan_days: i64 = r.try_get::<i64>("", "plan_days").unwrap_or(30);
    let now = now_iso();

    // F2 修复：原实现「先 UPDATE orders SET paid，再发货」，一旦发货中途失败
    // （数据库抖动、订阅表异常等），用户已扣款却拿不到积分/会员，且系统无任何
    // 记录可供补偿——既无重试入口也无告警线索。
    //
    // 改为 outbox：每个发货动作先占位（UNIQUE 幂等）再执行，失败留痕。
    // 失败时订单保持 paid，outbox 留 failed 记录，由 retry_fulfill 重放补齐。
    let mut stage_errors: Vec<String> = Vec::new();

    match biz_type.as_str() {
        "points_recharge" => {
            let key = format!("points:{order_no}");
            let claimed = stage_claimed(st, order_id, &order_no, &key, "points_recharge").await?;
            if !claimed {
                match add_ledger(st, &member_id, points, "recharge", &order_no, "充值到账").await {
                    Ok(_) => stage_done(st, order_id, &key).await?,
                    Err(e) => {
                        let msg = e.message.clone();
                        let _ = stage_failed(st, order_id, &key, &msg).await;
                        stage_errors.push(format!("积分发货：{msg}"));
                    }
                }
            }
        }
        "plan" => {
            let key = format!("plan:{order_no}");
            let claimed = stage_claimed(st, order_id, &order_no, &key, "plan_extend").await?;
            if !claimed {
                // plan 字段存 tier slug（tier_id 列在创建时已写入 slug）；无 slug 用 'paid'
                let plan_name = if tier_id.is_empty() { "paid".to_string() } else { tier_id.clone() };
                let r = async {
                    extend_plan(st, &member_id, plan_days.max(1)).await?;
                    st.db
                        .execute(
                            "UPDATE members SET plan = ?, updated_at = ? WHERE id = ? AND tenant_id = ?",
                            vec![
                                sval(plan_name),
                                sval(now.clone()),
                                sval(member_id.clone()),
                                sval(st.tenant.clone()),
                            ],
                        )
                        .await
                        .map_err(|e| ApiError::bad(format!("更新失败：{e}")))?;
                    Ok::<(), ApiError>(())
                }
                .await;
                match r {
                    Ok(()) => stage_done(st, order_id, &key).await?,
                    Err(e) => {
                        let msg = e.message.clone();
                        let _ = stage_failed(st, order_id, &key, &msg).await;
                        stage_errors.push(format!("订阅发货：{msg}"));
                    }
                }
            }
        }
        _ => {}
    }

    // 邀请奖励延迟到账：好友首次付费 → 邀请人补发（每个好友只发一次）
    let mut reward_note: Option<(String, i64, i64)> = None;
    if let Some((_, _, invited_by)) = member_row(st, &member_id).await? {
        if !invited_by.is_empty() {
            let key = format!("invite:{member_id}");
            let claimed = stage_claimed(st, order_id, &order_no, &key, "invite_reward").await?;
            if !claimed {
                let pts = setting_i64(st, "points_invite_reward", 100).await;
                if pts > 0 {
                    match add_ledger(
                        st,
                        &invited_by,
                        pts,
                        "invite_reward",
                        &member_id,
                        &format!("好友首次付费（订单 {order_no}）"),
                    )
                    .await
                    {
                        Ok(bal) => {
                            stage_done(st, order_id, &key).await?;
                            reward_note = Some((invited_by, pts, bal));
                        }
                        Err(e) => {
                            let msg = e.message.clone();
                            let _ = stage_failed(st, order_id, &key, &msg).await;
                            stage_errors.push(format!("邀请奖励：{msg}"));
                        }
                    }
                } else {
                    // 奖励配置为 0：视为无需发货，直接标记完成避免每次重试
                    stage_done(st, order_id, &key).await?;
                }
            }
        }
    }

    // 任一阶段失败：订单已 paid，错误随响应返回，outbox 留痕待 retry_fulfill 重放
    if !stage_errors.is_empty() {
        return Err(ApiError::bad(format!(
            "订单 {} 已收款但发货未全部完成（可在订单页点「重试发货」补齐）：{}",
            order_no,
            stage_errors.join("；")
        )));
    }

    let invite_reward = reward_note.map(|(inviter, pts, bal)| json!({
        "inviterId": inviter, "points": pts, "balance": bal
    }));
    Ok((order_no, invite_reward))
}

/// GET /api/public/members/orders —— 我的订单（会员本人视角，最近 50 条）
pub async fn my_orders(State(st): State<AppState>, auth: MemberAuth) -> ApiResult {
    let mid = auth.0.sub.clone();
    let rows = st
        .db
        .query_all(
            "SELECT order_no, biz_type, points, plan_days, amount_cents, channel, status, created_at, paid_at \
             FROM orders WHERE tenant_id = ? AND member_id = ? ORDER BY created_at DESC LIMIT 50",
            vec![sval(st.tenant.clone()), sval(mid.clone())],
        )
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "orderNo": r.try_get::<String>("", "order_no").unwrap_or_default(),
                "bizType": r.try_get::<String>("", "biz_type").unwrap_or_default(),
                "points": r.try_get::<i64>("", "points").unwrap_or(0),
                "planDays": r.try_get::<i64>("", "plan_days").unwrap_or(0),
                "amountCents": r.try_get::<i64>("", "amount_cents").unwrap_or(0),
                "channel": r.try_get::<String>("", "channel").unwrap_or_default(),
                "status": r.try_get::<String>("", "status").unwrap_or_default(),
                "createdAt": r.try_get::<String>("", "created_at").unwrap_or_default(),
                "paidAt": r.try_get::<String>("", "paid_at").unwrap_or_default(),
            })
        })
        .collect();
    ok(json!({ "items": items }))
}

/// GET /api/public/orders/{order_no} —— 会员查询自己的订单状态（扫码支付轮询用）
pub async fn order_status(
    State(st): State<AppState>,
    auth: MemberAuth,
    Path(no): Path<String>,
) -> ApiResult {
    let row = st
        .db
        .query_one(
            "SELECT status, amount_cents, paid_at, member_id FROM orders \
             WHERE tenant_id = ? AND order_no = ? LIMIT 1",
            vec![sval(st.tenant.clone()), sval(no.clone())],
        )
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    let Some(r) = row else { return Err(ApiError::not_found("订单不存在")) };
    let owner: String = r.try_get("", "member_id").unwrap_or_default();
    if owner != auth.0.sub {
        return Err(ApiError::not_found("订单不存在"));
    }
    ok(json!({
        "orderNo": no,
        "status": r.try_get::<String>("", "status").unwrap_or_default(),
        "amountCents": r.try_get::<i64>("", "amount_cents").unwrap_or(0),
        "paidAt": r.try_get::<String>("", "paid_at").unwrap_or_default(),
    }))
}

/// GET /api/admin/orders/recon?days=30 —— 订单对账：窗口内按 日期×渠道×状态 分组
/// （日期取 paid_at，未支付订单回退 created_at）
pub async fn orders_recon(
    State(st): State<AppState>,
    auth: Auth,
    axum::extract::Query(q): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> ApiResult {
    ensure(&auth, "content.members.view")?;
    let days = q
        .get("days")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(30)
        .clamp(1, 365);
    let since = (chrono::Utc::now() - Duration::days(days))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let rows = st
        .db
        .query_all(
            "SELECT date(COALESCE(NULLIF(paid_at, ''), created_at)) AS d, channel, status, \
             COUNT(*) AS n, COALESCE(SUM(amount_cents), 0) AS amt \
             FROM orders WHERE tenant_id = ? AND created_at >= ? \
             GROUP BY d, channel, status ORDER BY d DESC",
            vec![sval(st.tenant.clone()), sval(since.clone())],
        )
        .await
        .map_err(|e| ApiError::bad(format!("对账查询失败：{e}")))?;
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "date": r.try_get::<String>("", "d").unwrap_or_default(),
                "channel": r.try_get::<String>("", "channel").unwrap_or_default(),
                "status": r.try_get::<String>("", "status").unwrap_or_default(),
                "count": r.try_get::<i64>("", "n").unwrap_or(0),
                "amountCents": r.try_get::<i64>("", "amt").unwrap_or(0),
            })
        })
        .collect();
    // 窗口内按状态汇总（合计行用）
    let srows = st
        .db
        .query_all(
            "SELECT status, COUNT(*) AS n, COALESCE(SUM(amount_cents), 0) AS amt \
             FROM orders WHERE tenant_id = ? AND created_at >= ? GROUP BY status",
            vec![sval(st.tenant.clone()), sval(since.clone())],
        )
        .await
        .map_err(|e| ApiError::bad(format!("对账汇总失败：{e}")))?;
    let mut summary = std::collections::BTreeMap::new();
    for r in &srows {
        let k: String = r.try_get("", "status").unwrap_or_default();
        summary.insert(k, json!({
            "count": r.try_get::<i64>("", "n").unwrap_or(0),
            "amountCents": r.try_get::<i64>("", "amt").unwrap_or(0),
        }));
    }
    ok(json!({ "days": days, "rows": items, "summary": summary }))
}

/// POST /api/admin/orders/close-stale —— 关闭创建超过 24 小时仍未支付的订单
pub async fn orders_close_stale(State(st): State<AppState>, auth: Auth) -> ApiResult {
    ensure(&auth, "content.members.update")?;
    let cutoff = (chrono::Utc::now() - Duration::hours(24))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let n = st
        .db
        .execute(
            "UPDATE orders SET status = 'closed', updated_at = ? \
             WHERE tenant_id = ? AND status = 'pending' AND created_at < ?",
            vec![sval(now_iso()), sval(st.tenant.clone()), sval(cutoff)],
        )
        .await
        .map_err(|e| ApiError::bad(format!("关闭订单失败：{e}")))?;
    ok(json!({ "closed": n }))
}

/// GET /api/admin/orders/fulfill-pending —— 列出发货未完成的订单（对账/运维用）
pub async fn orders_fulfill_pending(State(st): State<AppState>, auth: Auth) -> ApiResult {
    ensure(&auth, "content.members.view")?;
    let rows = st
        .db
        .query_all(
            "SELECT order_id, order_no, stage_key, stage, status, attempts, last_error, updated_at \
             FROM order_fulfill_log \
             WHERE tenant_id = ? AND status <> 'done' \
             ORDER BY updated_at DESC LIMIT 100",
            vec![sval(st.tenant.clone())],
        )
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "orderId": r.try_get::<String>("", "order_id").unwrap_or_default(),
                "orderNo": r.try_get::<String>("", "order_no").unwrap_or_default(),
                "stageKey": r.try_get::<String>("", "stage_key").unwrap_or_default(),
                "stage": r.try_get::<String>("", "stage").unwrap_or_default(),
                "status": r.try_get::<String>("", "status").unwrap_or_default(),
                "attempts": r.try_get::<i64>("", "attempts").unwrap_or(0),
                "lastError": r.try_get::<String>("", "last_error").unwrap_or_default(),
                "updatedAt": r.try_get::<String>("", "updated_at").unwrap_or_default(),
            })
        })
        .collect();
    ok(json!({ "items": items }))
}

/// POST /api/admin/orders/retry-fulfill —— 重放未完成的发货动作
///
/// 用户已付款、但发货中途失败的订单（如数据库抖动、订阅表异常），
/// 由本接口按 outbox 记录重新执行未完成的阶段。每个阶段靠 UNIQUE 幂等，
/// 已 done 的不会重复发货，可安全反复调用。
pub async fn orders_retry_fulfill(State(st): State<AppState>, auth: Auth) -> ApiResult {
    ensure(&auth, "content.members.update")?;
    let rows = st
        .db
        .query_all(
            "SELECT DISTINCT order_id FROM order_fulfill_log \
             WHERE tenant_id = ? AND status <> 'done' LIMIT 50",
            vec![sval(st.tenant.clone())],
        )
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    let mut done = 0usize;
    let mut failed: Vec<String> = Vec::new();
    for r in &rows {
        let oid: String = r.try_get("", "order_id").unwrap_or_default();
        if oid.is_empty() {
            continue;
        }
        match fulfill_order(&st, &oid).await {
            Ok(_) => done += 1,
            Err(e) => failed.push(e.message.clone()),
        }
    }
    ok(json!({
        "retried": rows.len(),
        "recovered": done,
        "stillFailing": failed,
    }))
}
