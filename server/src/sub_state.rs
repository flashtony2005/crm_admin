//! 订阅状态机（阶段 1）：把 Stripe 的订阅事件收敛成 `members` 的有效期。
//!
//! ## 为什么需要它
//!
//! 阶段 0 时四类订阅事件（`customer.subscription.*` / `invoice.payment_*`）
//! 一律落库标 `pending_apply` —— 事件收得到，但**没有任何东西能把它们变成
//! 会员状态**。两个后果都直接是钱：
//!
//! 1. **续费不发货**：年付会员第二年正常扣款，`members.plan_expires_at`
//!    却停在第一年末 —— 用户付了钱、服务在到期日断掉。
//! 2. **到期不降级**：`current_period_end` 过了，会员仍然有效。因为
//!    `members` 表自身是自洽的（plan ≠ free 且到期时间在未来），
//!    对账、归因、后台列表**都看不出异常**，等于在悄悄送会员。
//!
//! ## 三条设计不变量
//!
//! **① 判据是时间，不是事件类型。**
//! Stripe 的「期末取消」与「立即取消」发的**都是**
//! `customer.subscription.deleted`，区别只在于 `current_period_end`
//! 是否已经过去；「续费成功」与「重新订阅」发的**都是**
//! `invoice.payment_succeeded`。按事件类型写状态机，这两种情况各自对一半，
//! 而错的那一半不是多发就是少发。按 `current_period_end` 判，两种情况自然分开。
//!
//! **② 订阅是覆盖语义，人工开通是累加语义。**
//! 订阅的到期时间由 Stripe 权威决定，只能**覆盖**；`points::extend_plan`
//! 那种「在现有到期日上叠加天数」的做法在这里是错的 —— Stripe 会把周期改短
//! （试用转正、按比例调整），累加会算出比真实到期更晚的时间，而这个偏差
//! 只在 Stripe 停止扣款之后才会暴露。
//!
//! **③ webhook 可以丢，到期必须到。**
//! `expire_due()` 由调度器每分钟兜底扫描 `current_period_end <= now` 的订阅
//! 主动降级。这是「会员不会永久有效」的最后一道闸 —— 也是当年那个
//! 「空到期 = 永久会员」缺陷的根治：现在不依赖任何一次事件是否送达。

use chrono::{DateTime, TimeZone, Utc};
use sea_orm::Value as SqlValue;
use serde_json::json;
use uuid::Uuid;

use crate::{db::now_iso, state::AppState};

fn sval(s: String) -> SqlValue {
    SqlValue::String(Some(s))
}

// ═══════════════════════════════════════════════════════════════════════
// 一、纯函数层：判定「此刻该不该给会员资格」
// ═══════════════════════════════════════════════════════════════════════

/// Stripe 订阅状态（归一化后）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SubStatus {
    /// 正常计费中
    Active,
    /// 试用期（同样享有服务）
    Trialing,
    /// 扣款失败，Stripe 仍在按退避策略重试（宽限期）
    PastDue,
    /// 已取消。**期末取消时周期未过，仍然有效**
    Canceled,
    /// 重试用尽仍未收到钱
    Unpaid,
    /// 首次支付未完成 / 已过期（含 Stripe 的 paused）
    Incomplete,
    /// 不认识的字符串
    Unknown,
}

impl SubStatus {
    /// 只认 Stripe 的原始串。**未知一律归 `Unknown` 并按「不给服务」处理**：
    /// Stripe 新增一个状态时，我们宁可少发会员，也不能凭一个看不懂的串白送。
    pub(crate) fn parse(raw: &str) -> Self {
        match raw.trim() {
            "active" => Self::Active,
            "trialing" => Self::Trialing,
            "past_due" => Self::PastDue,
            "canceled" => Self::Canceled,
            "unpaid" => Self::Unpaid,
            "incomplete" | "incomplete_expired" | "paused" => Self::Incomplete,
            _ => Self::Unknown,
        }
    }

    /// 是否属于「订阅还活着」的状态（仅用于展示与统计，不参与有效性判定）
    pub(crate) fn is_live(self) -> bool {
        matches!(
            self,
            Self::Active | Self::Trialing | Self::PastDue | Self::Canceled
        )
    }
}

/// 订阅在当前时刻应当产生的会员状态效果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlanEffect {
    /// 有效
    Grant,
    /// 无效：应降回 free（除非该会员另有更晚的有效期，见 `apply_row`）
    Revoke,
}

/// **阶段 1 的核心不变量**：会员是否有有效，只由 `current_period_end` 与
/// `now` 的关系决定，**不由收到的是哪个事件决定**。
pub(crate) fn derive_plan_effect(
    status_raw: &str,
    period_end: &str,
    now: DateTime<Utc>,
) -> PlanEffect {
    // 没有周期终点 = 无从判断有效期。**绝不能当成无限期** ——
    // 那正是「空到期 = 永久会员」的老缺陷。一律视为无效。
    let Some(end) = parse_ts(period_end) else {
        return PlanEffect::Revoke;
    };
    // 周期已过：不论 Stripe 说它是什么状态，都不该继续享有服务。
    // 这一条单独成判，是因为「已取消但周期未过」必须**仍然有效** ——
    // 用户已经付到期末，提前停服等于吞掉他付过的那一段。
    if end <= now {
        return PlanEffect::Revoke;
    }
    match SubStatus::parse(status_raw) {
        SubStatus::Active | SubStatus::Trialing => PlanEffect::Grant,
        // past_due：Stripe 正在重试扣款（卡过期是最常见的场景）。
        // 周期内继续给服务 —— 立刻断供会误伤「换了新卡、几分钟后就补上」的用户；
        // 而 Stripe 重试用尽后会自己转 canceled/unpaid，那时周期也已过去。
        SubStatus::PastDue => PlanEffect::Grant,
        // canceled 且周期未过 = **期末取消**：用户已付过这一段，服务给到期末。
        SubStatus::Canceled => PlanEffect::Grant,
        // 钱没到位：不收钱就不发服务。
        SubStatus::Unpaid | SubStatus::Incomplete => PlanEffect::Revoke,
        // 看不懂的状态：默认不发。
        SubStatus::Unknown => PlanEffect::Revoke,
    }
}

/// 乱序防护：Stripe **不保证事件顺序**（重投与并发投递都会乱序）。
/// 旧事件后到会把新周期覆盖回去，表现为「刚续费完又变回过期」。
///
/// `stored = 0`（本地还没时间戳）时任何事件都算新；
/// `event_at = 0`（事件体没带 `created`）时**不做判定** ——
/// 不能因为一个可选字段缺失就丢掉事件。
pub(crate) fn is_stale_event(event_at: i64, stored: i64) -> bool {
    event_at != 0 && stored != 0 && event_at < stored
}

/// RFC3339 → UTC。空串/非法一律 `None`（调用方据此判定"无从判断"）。
pub(crate) fn parse_ts(raw: &str) -> Option<DateTime<Utc>> {
    if raw.trim().is_empty() {
        return None;
    }
    DateTime::parse_from_rfc3339(raw)
        .map(|d| d.with_timezone(&Utc))
        .ok()
}

/// Stripe 的 Unix 秒（整数或字符串都见过）→ RFC3339
pub(crate) fn unix_to_iso(v: Option<&serde_json::Value>) -> String {
    let sec = match v {
        Some(serde_json::Value::Number(n)) => n.as_i64().unwrap_or(0),
        Some(serde_json::Value::String(s)) => s.trim().parse::<i64>().unwrap_or(0),
        _ => 0,
    };
    if sec <= 0 {
        return String::new();
    }
    Utc.timestamp_opt(sec, 0)
        .single()
        .map(|d| d.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
        .unwrap_or_default()
}

// ═══════════════════════════════════════════════════════════════════════
// 二、落库层：订阅行读写
// ═══════════════════════════════════════════════════════════════════════

/// 从 `customer.subscription.*` 事件解析出的订阅快照
#[derive(Debug, Clone, Default)]
pub(crate) struct SubSnapshot {
    pub stripe_subscription_id: String,
    pub stripe_customer_id: String,
    /// 从订阅 metadata 读回（checkout 时写入）。为空时沿用本地已有的 member_id。
    pub member_id: String,
    pub tier_slug: String,
    pub price_id: String,
    pub interval: String,
    /// **原始** Stripe 状态串（不做归一化，后台看到的就是 Stripe 原话）
    pub status: String,
    pub current_period_end: String,
    /// `None` = 事件里没带这个字段。**必须区分「没带」与「带了 false」**：
    /// 只有 `customer.subscription.updated` 稳定携带它，若把"没带"当成 false
    /// 覆盖下去，一个已安排期末取消的订阅会被抹掉取消标记 ——
    /// 用户以为退订成功了，下个月继续被扣款。
    pub cancel_at_period_end: Option<bool>,
    pub canceled_at: String,
    pub started_at: String,
    /// 事件 `created`（Unix 秒），用于乱序防护
    pub event_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum UpsertOutcome {
    /// 已写入（新建或更新）
    Applied,
    /// 事件比本地记录更旧，已忽略
    Stale,
    /// 认不出是哪个会员（无 member_id 且本地也没有该订阅行）
    Orphan,
}

/// 写入订阅快照（`customer.subscription.created/updated/deleted`）。
pub(crate) async fn upsert_snapshot(
    st: &AppState,
    snap: &SubSnapshot,
) -> Result<UpsertOutcome, String> {
    if snap.stripe_subscription_id.trim().is_empty() {
        return Ok(UpsertOutcome::Orphan);
    }
    let existing = st
        .db
        .query_one(
            "SELECT id, member_id, last_event_at FROM subscriptions \
             WHERE tenant_id = ? AND stripe_subscription_id = ? LIMIT 1",
            vec![
                sval(st.tenant.clone()),
                sval(snap.stripe_subscription_id.clone()),
            ],
        )
        .await
        .map_err(|e| format!("查询订阅失败：{e}"))?;

    let now = now_iso();
    match existing {
        Some(r) => {
            let row_id: String = r.try_get("", "id").map_err(|e| e.to_string())?;
            let stored_at: i64 = r.try_get("", "last_event_at").unwrap_or(0);
            if is_stale_event(snap.event_at, stored_at) {
                return Ok(UpsertOutcome::Stale);
            }
            // member_id：事件带了就用事件的，没带则**保留本地已有的** ——
            // invoice 系事件不带 member_id，若用空串覆盖就会把归属关系抹掉。
            let local_member: String = r.try_get("", "member_id").unwrap_or_default();
            let member_id = if snap.member_id.trim().is_empty() {
                local_member
            } else {
                snap.member_id.clone()
            };
            // 同理：tier_slug / customer 可能只在 checkout 那次带上，空值不覆盖。
            st.db
                .execute(
                    "UPDATE subscriptions SET member_id = ?, \
                     stripe_customer_id = CASE WHEN ? = '' THEN stripe_customer_id ELSE ? END, \
                     tier_slug = CASE WHEN ? = '' THEN tier_slug ELSE ? END, \
                     price_id = CASE WHEN ? = '' THEN price_id ELSE ? END, \
                     interval = CASE WHEN ? = '' THEN interval ELSE ? END, \
                     status = ?, current_period_end = ?, \
                     cancel_at_period_end = CASE WHEN ? = 1 THEN ? ELSE cancel_at_period_end END, \
                     canceled_at = CASE WHEN ? = '' THEN canceled_at ELSE ? END, \
                     started_at = CASE WHEN ? = '' THEN started_at ELSE ? END, \
                     last_event_at = MAX(last_event_at, ?), \
                     updated_at = ? \
                     WHERE id = ? AND tenant_id = ?",
                    vec![
                        sval(member_id),
                        sval(snap.stripe_customer_id.clone()),
                        sval(snap.stripe_customer_id.clone()),
                        sval(snap.tier_slug.clone()),
                        sval(snap.tier_slug.clone()),
                        sval(snap.price_id.clone()),
                        sval(snap.price_id.clone()),
                        sval(snap.interval.clone()),
                        sval(snap.interval.clone()),
                        sval(snap.status.clone()),
                        sval(snap.current_period_end.clone()),
                        // 「事件带了这个字段吗」与「值是多少」分开传：
                        // 没带时保持本地原值（见 SubSnapshot 的注释）。
                        SqlValue::Int(Some(if snap.cancel_at_period_end.is_some() { 1 } else { 0 })),
                        SqlValue::Int(Some(if snap.cancel_at_period_end.unwrap_or(false) { 1 } else { 0 })),
                        sval(snap.canceled_at.clone()),
                        sval(snap.canceled_at.clone()),
                        sval(snap.started_at.clone()),
                        sval(snap.started_at.clone()),
                        SqlValue::BigInt(Some(snap.event_at)),
                        sval(now),
                        sval(row_id),
                        sval(st.tenant.clone()),
                    ],
                )
                .await
                .map_err(|e| format!("更新订阅失败：{e}"))?;
            Ok(UpsertOutcome::Applied)
        }
        None => {
            // 新订阅必须知道属于谁：没有 member_id 就无法生效，
            // 落库也只是给人工排查留痕（下次带 member_id 的事件会把它补齐）。
            if snap.member_id.trim().is_empty() {
                return Ok(UpsertOutcome::Orphan);
            }
            insert_snapshot(st, snap).await?;
            Ok(UpsertOutcome::Applied)
        }
    }
}

/// 新建一行订阅快照。调用方负责确认「本地确实还没有这一行」。
async fn insert_snapshot(st: &AppState, snap: &SubSnapshot) -> Result<(), String> {
    let now = now_iso();
    st.db
        .execute(
            "INSERT INTO subscriptions \
             (id, tenant_id, member_id, stripe_subscription_id, stripe_customer_id, \
              tier_slug, price_id, interval, status, current_period_end, \
              cancel_at_period_end, canceled_at, started_at, last_event_at, closed_at, \
              created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, '', ?, ?)",
            vec![
                sval(Uuid::new_v4().to_string()),
                sval(st.tenant.clone()),
                sval(snap.member_id.clone()),
                sval(snap.stripe_subscription_id.clone()),
                sval(snap.stripe_customer_id.clone()),
                sval(if snap.tier_slug.trim().is_empty() {
                    "paid".to_string()
                } else {
                    snap.tier_slug.clone()
                }),
                sval(snap.price_id.clone()),
                sval(if snap.interval.trim().is_empty() {
                    "monthly".to_string()
                } else {
                    snap.interval.clone()
                }),
                sval(snap.status.clone()),
                sval(snap.current_period_end.clone()),
                // 新建行：事件没带就按 false（新订阅默认没安排取消）。
                // 与 UPDATE 分支不同 —— 那里必须区分「没带」以保住已有标记。
                SqlValue::Int(Some(if snap.cancel_at_period_end.unwrap_or(false) { 1 } else { 0 })),
                sval(snap.canceled_at.clone()),
                sval(snap.started_at.clone()),
                SqlValue::BigInt(Some(snap.event_at)),
                sval(now.clone()),
                sval(now),
            ],
        )
        .await
        .map_err(|e| format!("写入订阅失败：{e}"))?;
    Ok(())
}

/// 仅在本地没有该订阅行时创建它。
///
/// 与 `upsert_snapshot` 的关键区别：**绝不覆盖已有行**。Checkout 事件里的
/// `current_period_end` 是按套餐周期**估算**的（session 对象根本不带周期终点），
/// 若它后到并覆盖真实值，到期时间就错了 —— 要么让用户被提前断供，
/// 要么多送几个月，两种都只有去查某个会员的到期日才发现。
pub(crate) async fn insert_if_absent(st: &AppState, snap: &SubSnapshot) -> Result<bool, String> {
    if snap.stripe_subscription_id.trim().is_empty() || snap.member_id.trim().is_empty() {
        return Ok(false);
    }
    let exists = st
        .db
        .query_one(
            "SELECT id FROM subscriptions \
             WHERE tenant_id = ? AND stripe_subscription_id = ? LIMIT 1",
            vec![
                sval(st.tenant.clone()),
                sval(snap.stripe_subscription_id.clone()),
            ],
        )
        .await
        .map_err(|e| format!("查询订阅失败：{e}"))?
        .is_some();
    if exists {
        return Ok(false);
    }
    insert_snapshot(st, snap).await?;
    Ok(true)
}

/// 记下「本订阅授予了这份有效期」。
///
/// 只用于 Checkout 兜底发货那条路 —— 真实 `customer.subscription.created`
/// 由 `apply_row` 的 Grant 分支写入。**两条路都必须写**：少任何一条，
/// 该订阅的 `granted_expiry` 就会停在空串，到期兜底便不敢降级
/// （空串的语义是"本订阅从未给过东西"，见迁移 0018），
/// 那个会员就会一直有效下去。
pub(crate) async fn mark_granted(
    st: &AppState,
    stripe_subscription_id: &str,
    expiry: &str,
) -> Result<(), String> {
    if stripe_subscription_id.trim().is_empty() || expiry.trim().is_empty() {
        return Ok(());
    }
    st.db
        .execute(
            "UPDATE subscriptions SET granted_expiry = ?, updated_at = ? \
             WHERE tenant_id = ? AND stripe_subscription_id = ?",
            vec![
                sval(expiry.to_string()),
                sval(now_iso()),
                sval(st.tenant.clone()),
                sval(stripe_subscription_id.to_string()),
            ],
        )
        .await
        .map_err(|e| format!("记录订阅授予失败：{e}"))?;
    Ok(())
}

/// `invoice.payment_succeeded` 带来的新周期终点。
///
/// invoice 对象**只有** subscription id 与 customer，没有 member_id ——
/// 所以必须靠本地订阅行来定位人（这也正是 0017 建表的原因之一）。
pub(crate) async fn touch_period(
    st: &AppState,
    stripe_subscription_id: &str,
    period_end: &str,
    status_hint: Option<&str>,
    event_at: i64,
) -> Result<UpsertOutcome, String> {
    if stripe_subscription_id.trim().is_empty() {
        return Ok(UpsertOutcome::Orphan);
    }
    let row = st
        .db
        .query_one(
            "SELECT id, last_event_at FROM subscriptions \
             WHERE tenant_id = ? AND stripe_subscription_id = ? LIMIT 1",
            vec![
                sval(st.tenant.clone()),
                sval(stripe_subscription_id.to_string()),
            ],
        )
        .await
        .map_err(|e| format!("查询订阅失败：{e}"))?;
    let Some(r) = row else {
        return Ok(UpsertOutcome::Orphan);
    };
    let row_id: String = r.try_get("", "id").map_err(|e| e.to_string())?;
    let stored_at: i64 = r.try_get("", "last_event_at").unwrap_or(0);
    if is_stale_event(event_at, stored_at) {
        return Ok(UpsertOutcome::Stale);
    }
    // 周期终点为空时不覆盖（宁可用旧值，也不能把订阅变成"无到期"= 永久有效）。
    let set_period = !period_end.trim().is_empty();
    let hint = status_hint.unwrap_or("").to_string();
    st.db
        .execute(
            "UPDATE subscriptions SET \
             current_period_end = CASE WHEN ? = 1 THEN ? ELSE current_period_end END, \
             status = CASE WHEN ? = '' THEN status ELSE ? END, \
             last_event_at = MAX(last_event_at, ?), updated_at = ? \
             WHERE id = ? AND tenant_id = ?",
            vec![
                SqlValue::Int(Some(if set_period { 1 } else { 0 })),
                sval(period_end.to_string()),
                sval(hint.clone()),
                sval(hint),
                SqlValue::BigInt(Some(event_at)),
                sval(now_iso()),
                sval(row_id),
                sval(st.tenant.clone()),
            ],
        )
        .await
        .map_err(|e| format!("更新订阅周期失败：{e}"))?;
    Ok(UpsertOutcome::Applied)
}

// ═══════════════════════════════════════════════════════════════════════
// 三、生效层：把订阅状态落到 members
// ═══════════════════════════════════════════════════════════════════════

/// 一次生效动作的结果（用于 webhook 事件的说明文案与调度器统计）
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ApplyOutcome {
    /// 是否让会员**获得了**有效期（Grant）。false 不等于出错：可能是
    /// 会员另有更晚的有效期所以保持不动。
    pub granted: bool,
    pub note: String,
}

/// 按 Stripe 订阅 id 取出本地行（四类事件都只有这个 id，没有本地主键）。
async fn load_by_stripe_id(
    st: &AppState,
    stripe_subscription_id: &str,
) -> Result<Option<crate::cmsdb::Row>, String> {
    st.db
        .query_one(
            "SELECT id, member_id, tier_slug, status, current_period_end, closed_at, \
                    granted_expiry \
             FROM subscriptions WHERE tenant_id = ? AND stripe_subscription_id = ? LIMIT 1",
            vec![
                sval(st.tenant.clone()),
                sval(stripe_subscription_id.to_string()),
            ],
        )
        .await
        .map_err(|e| format!("查询订阅失败：{e}"))
}

/// 让某个 Stripe 订阅**立即生效**，并在有金额时记一条收款。
///
/// 这是四类事件唯一的出口：状态机（订阅对象）与发票（新周期）都收敛到这里，
/// 避免「两条路各写一遍 members」这种迟早会分叉的写法。
///
/// `amount_cents = 0` 表示这次调用没有带来金额信息（订阅状态变更事件），
/// 此时只生效不记账 —— 账目由发票事件落，那是唯一知道金额的地方。
pub(crate) async fn settle(
    st: &AppState,
    stripe_subscription_id: &str,
    amount_cents: i64,
) -> Result<ApplyOutcome, String> {
    let Some(r) = load_by_stripe_id(st, stripe_subscription_id).await? else {
        return Ok(ApplyOutcome {
            granted: false,
            note: "本地无该订阅，仅落库（等带 member 的事件补齐归属）".into(),
        });
    };
    // 先落账再改会员：账目是钱的凭证，任何一步失败都要留下痕迹。
    // 反过来的顺序一旦在中间出错，就会出现"会员已开通但没记账"——
    // 那是收入对不上的那种错，比"记了账但没生效"难查得多。
    if amount_cents > 0 {
        let member_id: String = r.try_get("", "member_id").unwrap_or_default();
        let tier_slug: String = r.try_get("", "tier_slug").unwrap_or_default();
        let period_end: String = r.try_get("", "current_period_end").unwrap_or_default();
        record_order(
            st,
            &member_id,
            stripe_subscription_id,
            &period_end,
            amount_cents,
            &tier_slug,
        )
        .await?;
    }
    apply_row(st, &r).await
}

/// 对一个已读出的订阅行生效。
pub(crate) async fn apply_row(
    st: &AppState,
    r: &crate::cmsdb::Row,
) -> Result<ApplyOutcome, String> {
    let row_id: String = r.try_get("", "id").map_err(|e| e.to_string())?;
    let member_id: String = r.try_get("", "member_id").unwrap_or_default();
    let tier_slug: String = r.try_get("", "tier_slug").unwrap_or_default();
    let status: String = r.try_get("", "status").unwrap_or_default();
    let period_end: String = r.try_get("", "current_period_end").unwrap_or_default();

    if member_id.trim().is_empty() {
        return Ok(ApplyOutcome {
            granted: false,
            note: "订阅无归属会员，仅落库".into(),
        });
    }

    let now_dt = Utc::now();
    let effect = derive_plan_effect(&status, &period_end, now_dt);

    let m = st
        .db
        .query_one(
            "SELECT plan, plan_expires_at FROM members WHERE id = ? AND tenant_id = ? LIMIT 1",
            vec![sval(member_id.clone()), sval(st.tenant.clone())],
        )
        .await
        .map_err(|e| format!("查询会员失败：{e}"))?;
    let Some(m) = m else {
        return Ok(ApplyOutcome {
            granted: false,
            note: "会员不存在，仅落库".into(),
        });
    };
    let cur_plan: String = m.try_get("", "plan").unwrap_or_else(|_| "free".into());
    let cur_exp: String = m.try_get("", "plan_expires_at").unwrap_or_default();

    match effect {
        PlanEffect::Grant => {
            // 套餐名：订阅行上的 tier_slug。为空时退 'paid' —— 沿用
            // extend_plan 的既有约定（免费 → 付费），不能写空串把 plan 抹掉。
            let new_plan = if tier_slug.trim().is_empty() {
                if cur_plan == "free" {
                    "paid".to_string()
                } else {
                    cur_plan.clone()
                }
            } else {
                tier_slug.clone()
            };
            // **覆盖**而非累加：Stripe 的 current_period_end 是权威到期时间。
            st.db
                .execute(
                    "UPDATE members SET plan = ?, plan_expires_at = ?, updated_at = ? \
                     WHERE id = ? AND tenant_id = ?",
                    vec![
                        sval(new_plan.clone()),
                        sval(period_end.clone()),
                        sval(now_iso()),
                        sval(member_id.clone()),
                        sval(st.tenant.clone()),
                    ],
                )
                .await
                .map_err(|e| format!("更新会员失败：{e}"))?;
            // 记下「这份有效期是本订阅给的」。订阅结束时据此判断能否降级：
            // 只有 members.plan_expires_at 仍等于这个值，才说明期间没有别的
            // 来源（人工开通 / 兑码 / 另一订阅）改写过它（见迁移 0018）。
            st.db
                .execute(
                    "UPDATE subscriptions SET granted_expiry = ?, updated_at = ? \
                     WHERE id = ? AND tenant_id = ?",
                    vec![
                        sval(period_end.clone()),
                        sval(now_iso()),
                        sval(row_id.clone()),
                        sval(st.tenant.clone()),
                    ],
                )
                .await
                .map_err(|e| format!("记录订阅授予失败：{e}"))?;
            // 订阅复活（此前已终结又续上）：清掉终结标记，否则调度器
            // 会以为它已经处理完，后续到期不再扫描。
            let was_closed: String = r.try_get("", "closed_at").unwrap_or_default();
            if !was_closed.is_empty() {
                st.db
                    .execute(
                        "UPDATE subscriptions SET closed_at = '', updated_at = ? WHERE id = ? AND tenant_id = ?",
                        vec![sval(now_iso()), sval(row_id), sval(st.tenant.clone())],
                    )
                    .await
                    .map_err(|e| format!("清理终结标记失败：{e}"))?;
            }
            crate::webhooks_out::emit(
                st,
                "subscription.renewed",
                json!({
                    "memberId": member_id,
                    "tier": new_plan,
                    "status": status,
                    "expiresAt": period_end,
                }),
            );
            Ok(ApplyOutcome {
                granted: true,
                note: format!("会员有效期至 {period_end}（{}）", status),
            })
        }
        PlanEffect::Revoke => {
            // ⚠️ 不能无条件降 free：会员的有效期可能**另有来源**（人工开通、
            // 积分兑换、兑码、另一个订阅）。直接清零会把那些已经收到的钱
            // 一并抹掉，而这类误伤在后台看起来只是"某会员突然变回免费"，
            // 查不出原因。
            //
            // 但"另有来源"**不能**用「plan_expires_at > now」判断 —— 那个值
            // 恰恰就是本订阅 Grant 时写进去的，等于同义反复：任何订阅结束后
            // 都会恒判为"另有来源"，于是永不降级。那正是要消灭的「永久会员」，
            // 而且比老缺陷隐蔽得多（members 表自身完全自洽，只有拿订阅行去
            // 对才看得出来）。
            //
            // 判据取**相等**：会员当前的有效期恰好等于本订阅最后授予的值
            // ⇔ 自那以后没有别的来源改写过它 ⇔ 降级是安全的。
            let granted: String = r.try_get("", "granted_expiry").unwrap_or_default();
            let from_this_sub = !granted.is_empty() && granted == cur_exp;
            let still_valid = parse_ts(&cur_exp).map(|e| e > now_dt).unwrap_or(false);
            if !from_this_sub && still_valid {
                // 订阅已结束，但会员仍有效期至 cur_exp —— 只标终结，不动 plan。
                st.db
                    .execute(
                        "UPDATE subscriptions SET closed_at = ?, updated_at = ? WHERE id = ? AND tenant_id = ?",
                        vec![
                            sval(now_iso()),
                            sval(now_iso()),
                            sval(row_id.clone()),
                            sval(st.tenant.clone()),
                        ],
                    )
                    .await
                    .map_err(|e| format!("标记订阅终结失败：{e}"))?;
                crate::webhooks_out::emit(
                    st,
                    "subscription.ended",
                    json!({ "memberId": member_id, "tier": tier_slug, "retainedUntil": cur_exp }),
                );
                return Ok(ApplyOutcome {
                    granted: false,
                    note: format!("订阅已结束；会员另有有效期至 {cur_exp}，未降级"),
                });
            }
            st.db
                .execute(
                    "UPDATE members SET plan = 'free', plan_expires_at = '', updated_at = ? \
                     WHERE id = ? AND tenant_id = ?",
                    vec![
                        sval(now_iso()),
                        sval(member_id.clone()),
                        sval(st.tenant.clone()),
                    ],
                )
                .await
                .map_err(|e| format!("降级会员失败：{e}"))?;
            st.db
                .execute(
                    "UPDATE subscriptions SET closed_at = ?, granted_expiry = '', \
                     updated_at = ? WHERE id = ? AND tenant_id = ?",
                    vec![
                        sval(now_iso()),
                        sval(now_iso()),
                        sval(row_id),
                        sval(st.tenant.clone()),
                    ],
                )
                .await
                .map_err(|e| format!("标记订阅终结失败：{e}"))?;
            crate::webhooks_out::emit(
                st,
                "subscription.ended",
                json!({ "memberId": member_id, "tier": tier_slug, "status": status }),
            );
            Ok(ApplyOutcome {
                granted: false,
                note: format!("会员已降回 free（订阅状态 {status}）"),
            })
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 四、账目：订阅收入落 orders
// ═══════════════════════════════════════════════════════════════════════

/// 记一条订阅收款。
///
/// **为什么必须记**：`orders` 是「钱进来过」的唯一凭证。订阅链路此前不落
/// orders（人工通道才落），于是归因收入、对账、订单列表三处全都看不见
/// Stripe 的钱 —— 只有去 Stripe 后台翻账单才知道收了多少。
///
/// 幂等靠 `order_no` 的唯一键：`SUB-{订阅尾号}-{周期末秒}`。
/// 同一个计费周期无论收到多少次事件，只会有一条单。
///
/// 返回 `true` 表示**这一次真的新增了**（用于统计重复投递）。
pub(crate) async fn record_order(
    st: &AppState,
    member_id: &str,
    stripe_subscription_id: &str,
    period_end: &str,
    amount_cents: i64,
    tier_slug: &str,
) -> Result<bool, String> {
    if stripe_subscription_id.trim().is_empty() || member_id.trim().is_empty() {
        return Ok(false);
    }
    let Some(end) = parse_ts(period_end) else {
        // 没有周期终点就构造不出稳定的幂等键 —— 宁可暂时不记账，
        // 也不能用"当前时间"当键（那样每次事件都会产生一条新单，收入翻倍）。
        return Ok(false);
    };
    let tail: String = stripe_subscription_id
        .chars()
        .rev()
        .take(12)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let order_no = format!("SUB-{tail}-{}", end.timestamp());
    let now = now_iso();
    let written = st
        .db
        .execute(
            "INSERT OR IGNORE INTO orders \
             (id, tenant_id, order_no, member_id, biz_type, tier_id, points, plan_days, \
              amount_cents, channel, status, ref_no, paid_at, created_at, updated_at) \
             VALUES (?, ?, ?, ?, 'plan', ?, 0, 0, ?, 'stripe', 'paid', ?, ?, ?, ?)",
            vec![
                sval(Uuid::new_v4().to_string()),
                sval(st.tenant.clone()),
                sval(order_no.clone()),
                sval(member_id.to_string()),
                // tier_id 列存 slug 而不是 tiers.id：订阅元数据里只有 slug 是稳定的，
                // tier 行可能被改名换 id；用 slug 至少还能人工对上。
                sval(tier_slug.to_string()),
                SqlValue::BigInt(Some(amount_cents)),
                sval(stripe_subscription_id.to_string()),
                sval(now.clone()),
                sval(now.clone()),
                sval(now),
            ],
        )
        .await
        .map_err(|e| format!("订单落账失败：{e}"))?;
    // 与事件去重同一约定：SQLite 冲突返 Err、Turso 冲突返 Ok(0)，
    // 统一的"真的插进去了"信号是 rows_affected > 0。
    let newly = matches!(written, n if n > 0);
    if newly {
        crate::webhooks_out::emit(
            st,
            "order.paid",
            json!({
                "orderNo": order_no,
                "memberId": member_id,
                "bizType": "plan",
                "channel": "stripe",
                "amountCents": amount_cents,
                "refNo": stripe_subscription_id,
            }),
        );
    }
    Ok(newly)
}

// ═══════════════════════════════════════════════════════════════════════
// 五、兜底：调度器每分钟扫描到期订阅
// ═══════════════════════════════════════════════════════════════════════

/// 扫描已到期的订阅并降级。返回本轮处理的条数。
///
/// **这是「会员不会永久有效」的最后一道闸**：不依赖任何一次 webhook 是否送达。
/// 判据是 `current_period_end <= now`，与 `derive_plan_effect` 同源 ——
/// 两处必须是同一个判据，否则会出现「兜底降了、事件又给升回来」的抖动。
///
/// 每轮限量，避免某次积压（例如长时间停机）把所有工作挤在一分钟里做完。
pub(crate) async fn expire_due(st: &AppState) -> usize {
    const BATCH: usize = 200;
    let rows = st
        .db
        .query_all(
            "SELECT id, member_id, tier_slug, status, current_period_end, closed_at, \
                    granted_expiry \
             FROM subscriptions \
             WHERE tenant_id = ? AND closed_at = '' AND current_period_end <> '' \
               AND current_period_end <= ? \
             ORDER BY current_period_end ASC LIMIT ?",
            vec![
                sval(st.tenant.clone()),
                sval(now_iso()),
                SqlValue::BigInt(Some(BATCH as i64)),
            ],
        )
        .await
        .unwrap_or_default();

    let mut n = 0usize;
    for r in &rows {
        match apply_row(st, r).await {
            Ok(_) => n += 1,
            Err(e) => eprintln!("[warn] 订阅到期处理失败：{e}"),
        }
    }
    n
}

/// 启动时自检：库里是否存在「周期已过但还没终结」的订阅。
///
/// 只读，不修数据 —— 用于把「兜底是否真的在跑」这件事显式化。
pub(crate) async fn due_count(st: &AppState) -> i64 {
    st.db
        .query_one(
            "SELECT COUNT(*) AS c FROM subscriptions \
             WHERE tenant_id = ? AND closed_at = '' AND current_period_end <> '' \
               AND current_period_end <= ?",
            vec![sval(st.tenant.clone()), sval(now_iso())],
        )
        .await
        .ok()
        .flatten()
        .and_then(|r| r.try_get::<i64>("", "c").ok())
        .unwrap_or(0)
}

// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    /// **核心不变量**：判据是 current_period_end，不是事件类型。
    ///
    /// 这一组用例是阶段 1 的地基 —— 它同时钉死两件曾经做不到的事：
    /// 「期末取消」必须继续有效到期末；「周期已过」必须失效。
    #[test]
    fn plan_effect_is_decided_by_period_end_not_event_type() {
        let now = t("2026-09-28T00:00:00Z");
        let future = "2026-10-28T00:00:00Z";
        let past = "2026-08-28T00:00:00Z";

        // 周期内：active / trialing / past_due / canceled 都有效
        for s in ["active", "trialing", "past_due", "canceled"] {
            assert_eq!(
                derive_plan_effect(s, future, now),
                PlanEffect::Grant,
                "状态 {s} 且周期未过应有效"
            );
        }
        // **期末取消**：canceled 但周期未过 → 仍然有效（用户已付到期末）
        assert_eq!(
            derive_plan_effect("canceled", future, now),
            PlanEffect::Grant,
            "期末取消在周期结束前必须继续有效"
        );
        // 周期已过：任何状态都失效 —— 包括 active（续费事件丢了也不会白送）
        for s in ["active", "trialing", "past_due", "canceled", "unpaid", "incomplete"] {
            assert_eq!(
                derive_plan_effect(s, past, now),
                PlanEffect::Revoke,
                "状态 {s} 但周期已过应失效"
            );
        }
        // 钱没到位：周期未过也不发
        for s in ["unpaid", "incomplete", "incomplete_expired", "paused"] {
            assert_eq!(
                derive_plan_effect(s, future, now),
                PlanEffect::Revoke,
                "状态 {s} 不应发服务"
            );
        }
    }

    /// 空到期 / 非法时间**绝不能**被当成"永久有效"。这是历史缺陷的回归防线。
    #[test]
    fn missing_period_end_is_never_permanent() {
        let now = t("2026-09-28T00:00:00Z");
        for pe in ["", "   ", "not-a-date", "2026-09-28"] {
            assert_eq!(
                derive_plan_effect("active", pe, now),
                PlanEffect::Revoke,
                "周期终点 `{pe}` 无法解析时必须失效，不得视为无限期"
            );
        }
    }

    /// 不认识的 Stripe 状态一律不发服务。
    /// 反过来（未知当有效）意味着 Stripe 新增一个状态就能白送会员。
    #[test]
    fn unknown_status_never_grants() {
        let now = t("2026-09-28T00:00:00Z");
        assert_eq!(
            derive_plan_effect("some_new_stripe_state", "2026-10-28T00:00:00Z", now),
            PlanEffect::Revoke
        );
        assert_eq!(
            derive_plan_effect("", "2026-10-28T00:00:00Z", now),
            PlanEffect::Revoke
        );
        assert_eq!(SubStatus::parse("ACTIVE"), SubStatus::Unknown, "大小写不匹配即未知");
    }

    /// 周期刚好等于当前时刻 = 已到期（用 <= 而不是 <）。
    /// 边界方向决定"到期那一刻是否还给服务"，取保守侧。
    #[test]
    fn period_end_boundary_revokes() {
        let now = t("2026-09-28T00:00:00Z");
        assert_eq!(
            derive_plan_effect("active", "2026-09-28T00:00:00Z", now),
            PlanEffect::Revoke,
            "周期终点 == now 视为已到期"
        );
        assert_eq!(
            derive_plan_effect("active", "2026-09-28T00:00:00.001Z", now),
            PlanEffect::Grant
        );
    }

    /// 乱序防护：旧事件不得覆盖新状态。
    #[test]
    fn stale_events_are_rejected() {
        assert!(is_stale_event(100, 200), "更旧的事件应被拒绝");
        assert!(!is_stale_event(300, 200), "更新的事件应接受");
        assert!(!is_stale_event(200, 200), "同一时刻允许覆盖（Stripe 秒级精度）");
        assert!(!is_stale_event(100, 0), "本地无时间戳时任何事件都算新");
        assert!(!is_stale_event(0, 200), "事件缺 created 字段时不做判定，不能丢事件");
    }

    /// Stripe 的 Unix 秒解析：数字、字符串都要认；0/缺失一律空串
    /// （空串会走 derive 的"无从判断"分支 → 失效，绝不能变成永久）。
    #[test]
    fn unix_seconds_parsing() {
        assert_eq!(unix_to_iso(Some(&json!(1800000000))), "2027-01-15T08:00:00.000Z");
        assert_eq!(unix_to_iso(Some(&json!("1800000000"))), "2027-01-15T08:00:00.000Z");
        assert_eq!(unix_to_iso(Some(&json!(0))), "");
        assert_eq!(unix_to_iso(Some(&json!(""))), "");
        assert_eq!(unix_to_iso(None), "");
    }
}
