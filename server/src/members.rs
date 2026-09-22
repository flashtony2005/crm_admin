//! 会员（Members）：公开注册 / 登录 / 个人资料。
//! 复用 auth 的 JWT 与 argon2，但签发 role="member" 的令牌，与管理员令牌隔离
//! （管理员 Auth 提取器只接受 owner/editor/viewer；会员令牌无法访问管理端点）。

use argon2::{password_hash::{PasswordHash, PasswordVerifier}, Argon2};
use axum::{extract::{FromRef, Path, State}, Json};
use sea_orm::{Statement, Value as SqlValue};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    auth::{ensure, hash_password, sign, verify, Auth},
    db::now_iso,
    error::{ok, ApiError, ApiResult},
    state::AppState,
};

fn sval(s: String) -> SqlValue { SqlValue::String(Some(s)) }

/// 会员令牌提取器：仅接受 role=="member" 的合法 JWT。
pub struct MemberAuth(pub crate::auth::Claims);

impl<S> axum::extract::FromRequestParts<S> for MemberAuth
where
    S: Send + Sync + Clone + 'static,
    AppState: axum::extract::FromRef<S>,
{
    type Rejection = ApiError;
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or_else(|| ApiError::unauthorized("未登录"))?;
        let claims = verify(token).ok_or_else(|| ApiError::unauthorized("登录已过期"))?;
        if claims.role != "member" {
            return Err(ApiError::unauthorized("需要会员身份"));
        }
        // P0-4 修复：此前会员令牌**只验签、不回查 members**，而令牌有效期 30 天，
        // 所以管理员把会员 status 置 0 之后，对方手上令牌在剩余有效期内
        // 依旧能解锁全部付费内容（越权）。管理员侧早就在 Auth 里逐请求回查
        // （auth.rs），会员侧漏了这一条。现照同一做法补齐：停用立即失效。
        let st = AppState::from_ref(state);
        let row = st
            .db
            .query_one(
                "SELECT status FROM members WHERE id = ? AND tenant_id = ? LIMIT 1",
                vec![sval(claims.sub.clone()), sval(st.tenant.clone())],
            )
            .await
            .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
        match row {
            Some(r) => {
                let status: i64 = r.try_get::<i64>("", "status").unwrap_or(1);
                if status != 1 {
                    return Err(ApiError::unauthorized("账号已被停用"));
                }
            }
            None => return Err(ApiError::unauthorized("账号不存在或已删除")),
        }
        Ok(MemberAuth(claims))
    }
}

fn member_claims(id: &str, email: &str, tenant: &str) -> crate::auth::Claims {
    crate::auth::Claims {
        sub: id.to_string(),
        username: email.to_string(),
        role: "member".to_string(),
        tenant: tenant.to_string(),
        // 会员 token 不参与 users.token_version 校验（那是后台账号体系），置 0
        tv: 0,
        exp: crate::auth::now_secs() + 30 * 86400,
    }
}

#[derive(Deserialize)]
// 键名契约：**camelCase**（与通用网关 / 全站 JSON 契约一致）。
//
// 这里曾经是纯 snake_case，而实际调用方（admin web 的 `memberAuth.register`
// 与公开站注册表单）传的都是 `inviteCode`/`visitorId` —— serde 对未知字段
// **静默忽略**，于是两个键双双丢失：
//   · 邀请制开启时，用户明明填了码却被拒「注册需要邀请码」；
//   · 邀请制关闭时，`invited_by` 恒为空 → **邀请关系永远建立不起来，
//     邀请奖励一发不出去**（资损，且无任何报错）。
// 修掉的同时用 `alias` 保留 snake_case 接受名：老调用方不该因为这次
// 契约对齐而被打断。
#[serde(rename_all = "camelCase")]
pub struct MemberRegister {
    pub email: String,
    pub name: Option<String>,
    pub password: String,
    /// 邀请码：member_invite_required=on 时必填；off 时可空
    #[serde(default, alias = "invite_code")]
    pub invite_code: Option<String>,
    /// 匿名访客标识（分析归因）：由公开站 localStorage 持久化并随注册提交。
    /// 有了它，「这个会员是被哪篇文章带进来的」才第一次可回答。
    /// 可选 —— 老版本前端、后台建号、隐私模式都可能没有。
    #[serde(default, alias = "visitor_id")]
    pub visitor_id: Option<String>,
}

/// 读注册门槛开关：site_settings.member_invite_required ∈ on/1/true 视为开启
async fn invite_required(st: &AppState) -> bool {
    let v = st
        .db
        .query_one(
            "SELECT value FROM site_settings WHERE tenant_id = ? AND key = 'member_invite_required' LIMIT 1",
            vec![sval(st.tenant.clone())],
        )
        .await
        .ok()
        .flatten()
        .and_then(|r| r.try_get::<String>("", "value").ok())
        .unwrap_or_else(|| "off".into());
    matches!(v.as_str(), "on" | "1" | "true")
}

/// 校验并原子核销一个邀请码：返回邀请人 member id。
/// 核销走 `UPDATE ... WHERE used < quota` 条件写 + rows_affected 判定，
/// 并发重复注册不会超额消费额度。
async fn consume_invite(st: &AppState, code: &str) -> Result<String, ApiError> {
    let now = now_iso();
    let row = st
        .db
        .query_one(
            "SELECT id, owner_member_id, quota, used, expires_at, enabled \
             FROM invite_codes WHERE tenant_id = ? AND code = ? LIMIT 1",
            vec![sval(st.tenant.clone()), sval(code.to_string())],
        )
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    let Some(r) = row else { return Err(ApiError::bad("邀请码无效")) };
    let id: String = r.try_get("", "id").map_err(internal)?;
    let owner: String = r.try_get("", "owner_member_id").unwrap_or_default();
    let quota: i64 = r.try_get::<i64>("", "quota").unwrap_or(1);
    let used: i64 = r.try_get::<i64>("", "used").unwrap_or(0);
    let expires_at: String = r.try_get("", "expires_at").unwrap_or_default();
    let enabled: i64 = r.try_get::<i64>("", "enabled").unwrap_or(1);
    if enabled != 1 {
        return Err(ApiError::bad("邀请码已停用"));
    }
    if used >= quota {
        return Err(ApiError::bad("邀请码次数已用完"));
    }
    if !expires_at.is_empty() && expires_at.as_str() <= now.as_str() {
        return Err(ApiError::bad("邀请码已过期"));
    }
    let n = st
        .db
        .execute(
            "UPDATE invite_codes SET used = used + 1, updated_at = ? \
             WHERE id = ? AND used < quota",
            vec![sval(now), sval(id)],
        )
        .await
        .map_err(|e| ApiError::bad(format!("核销失败：{e}")))?;
    if n == 0 {
        return Err(ApiError::bad("邀请码次数已用完"));
    }
    Ok(owner)
}

/// POST /api/public/members/register
pub async fn register(State(st): State<AppState>, Json(req): Json<MemberRegister>) -> ApiResult {
    let email = req.email.trim().to_lowercase();
    if !email.contains('@') {
        return Err(ApiError::bad("邮箱格式不正确"));
    }
    if req.password.len() < 8 {
        return Err(ApiError::bad("密码至少 8 位"));
    }
    // 邀请门槛：开启时必须凭有效邀请码注册；关闭时提供了码也要校验（显式意图）
    let required = invite_required(&st).await;
    let code = req
        .invite_code
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let invited_by = match code.as_deref() {
        Some(c) => consume_invite(&st, c).await?,
        None if required => return Err(ApiError::bad("本站已开启邀请制，注册需要邀请码")),
        None => String::new(),
    };
    // 重名检查
    let exist = st
        .db
        .query_one(
            "SELECT id FROM members WHERE tenant_id = ? AND email = ? LIMIT 1",
            vec![sval(st.tenant.clone()), sval(email.clone())],
        )
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    if exist.is_some() {
        return Err(ApiError::bad("该邮箱已注册"));
    }
    let id = Uuid::new_v4().to_string();
    let now = now_iso();
    let name = req.name.clone().unwrap_or_else(|| email.clone());
    // ── 归因：把匿名访客的「首次触达文章」固化到会员上 ──────────────────
    // 取**最早**一条 article_view。归因要回答的是「哪一篇把他带进来的」，
    // 取最后一次会被站内推荐位、热门榜、回访反复改写 —— 那反映的是
    // 「促成」而非「获客」，直接指导不了「接下来该多写什么」。
    // 两个口径都有用，但不能混成一个数字，所以这里只固化首次触达。
    //
    // 顺序上**先读后写、随 INSERT 一次落库**：注册是主流程，不该出现
    // 「会员已建、归因没写上」的中间态。查不到浏览记录（例如直接打开
    // 注册页）就只留 visitor_id —— 归因留空会被统计成「未归因」，
    // 而这正是应该被看见的缺口，不该悄悄算到某篇文章头上。
    let (vid_final, first_aid, first_at) = match req
        .visitor_id
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        Some(v) => {
            let v: String = v.chars().take(64).collect();
            let row = st
                .db
                .query_one(
                    "SELECT ref_id, created_at FROM events \
                     WHERE tenant_id = ? AND visitor_id = ? AND type = 'article_view' AND ref_id <> '' \
                     ORDER BY created_at ASC, rowid ASC LIMIT 1",
                    vec![sval(st.tenant.clone()), sval(v.clone())],
                )
                .await
                .unwrap_or(None);
            match row {
                Some(r) => (
                    v,
                    r.try_get::<String>("", "ref_id").unwrap_or_default(),
                    r.try_get::<String>("", "created_at").unwrap_or_default(),
                ),
                None => (v, String::new(), String::new()),
            }
        }
        None => (String::new(), String::new(), String::new()),
    };
    st.db
        .execute(
            "INSERT INTO members (id, tenant_id, email, name, password_hash, status, plan, invited_by, \
             visitor_id, first_touch_article_id, first_touch_at, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, 1, 'free', ?, ?, ?, ?, ?, ?)",
            vec![
                sval(id.clone()),
                sval(st.tenant.clone()),
                sval(email.clone()),
                sval(name.clone()),
                sval(hash_password(&req.password)),
                sval(invited_by.clone()),
                sval(vid_final),
                sval(first_aid),
                sval(first_at),
                sval(now.clone()),
                sval(now.clone()),
            ],
        )
        .await
        .map_err(|e| ApiError::bad(format!("注册失败：{e}")))?;
    let token = sign(&member_claims(&id, &email, &st.tenant))?;
    // 触发会员注册出站 Webhook
    crate::webhooks_out::emit(&st, "member.registered", json!({ "id": id, "email": email, "name": name, "invited": !invited_by.is_empty() }));
    ok(json!({ "token": token, "member": public_member(&id, &email, &name, "free") }))
}

#[derive(Deserialize)]
pub struct MemberLogin {
    pub email: String,
    pub password: String,
}

/// POST /api/public/members/login
pub async fn login(State(st): State<AppState>, Json(req): Json<MemberLogin>) -> ApiResult {
    let email = req.email.trim().to_lowercase();
    let row = st
        .db
        .query_one(
            "SELECT id, name, password_hash, plan, status FROM members WHERE tenant_id = ? AND email = ? LIMIT 1",
            vec![sval(st.tenant.clone()), sval(email.clone())],
        )
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    let Some(r) = row else { return Err(ApiError::unauthorized("邮箱或密码错误")) };
    let id: String = r.try_get("", "id").map_err(internal)?;
    let name: String = r.try_get("", "name").unwrap_or_default();
    let hash: String = r.try_get("", "password_hash").map_err(internal)?;
    let plan: String = r.try_get("", "plan").unwrap_or_else(|_| "free".into());
    let status: i64 = r.try_get::<i64>("", "status").unwrap_or(1);
    if status != 1 {
        return Err(ApiError::unauthorized("账号已被停用"));
    }
    let ph = PasswordHash::new(&hash).map_err(|_| ApiError::bad("密码哈希损坏"))?;
    if !Argon2::default().verify_password(req.password.as_bytes(), &ph).is_ok() {
        return Err(ApiError::unauthorized("邮箱或密码错误"));
    }
    let token = sign(&member_claims(&id, &email, &st.tenant))?;
    ok(json!({ "token": token, "member": public_member(&id, &email, &name, &plan) }))
}

/// GET /api/public/members/me
pub async fn me(State(st): State<AppState>, auth: MemberAuth) -> ApiResult {
    let row = st
        .db
        .query_one(
            "SELECT id, email, name, plan FROM members WHERE id = ? AND tenant_id = ? LIMIT 1",
            vec![sval(auth.0.sub.clone()), sval(st.tenant.clone())],
        )
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    let Some(r) = row else { return Err(ApiError::not_found("会员不存在")) };
    let id: String = r.try_get("", "id").map_err(internal)?;
    let email: String = r.try_get("", "email").unwrap_or_default();
    let name: String = r.try_get("", "name").unwrap_or_default();
    let plan: String = r.try_get("", "plan").unwrap_or_else(|_| "free".into());
    ok(json!({ "member": public_member(&id, &email, &name, &plan) }))
}

/// GET /api/public/members/plans —— 当前会员套餐（供前端展示）
pub async fn plans(_st: State<AppState>) -> ApiResult {
    ok(json!({ "plan": "free" }))
}

fn public_member(id: &str, email: &str, name: &str, plan: &str) -> Value {
    json!({ "id": id, "email": email, "name": name, "plan": plan })
}

#[derive(Deserialize)]
pub struct MemberUpdate {
    pub name: Option<String>,
    /// P0-1：**保留此字段只为显式拒绝**，它不是可写字段。
    ///
    /// 此前这里把客户端传来的 `plan` 直接写库，而付费墙只读该字段、且
    /// `plan_expires_at` 为空即视为不限期 —— 于是注册后一句
    /// `POST /api/public/members/me {"plan":"pro"}` 就是**永久解锁全部
    /// paid_level=1 内容**（注册默认开放）。
    ///
    /// 换成直接删字段会变成"静默忽略"，调用方以为改成功了；保留字段 + 显式
    /// 400 才能让前端/攻击者立刻知道这条路不通。
    /// 会员套餐的唯一合法写入口：订单发货 / 兑码核销 / 后台管理。
    pub plan: Option<String>,
}

/// POST /api/public/members/me —— 更新昵称（会员自助）
pub async fn update_me(State(st): State<AppState>, auth: MemberAuth, Json(req): Json<MemberUpdate>) -> ApiResult {
    // P0-1：套餐只能由钱换，不接受客户端声明。
    if req.plan.is_some() {
        return Err(ApiError::bad(
            "会员套餐不可自助修改，请通过购买订单或兑换码升级",
        ));
    }
    let mut sets = Vec::new();
    let mut args: Vec<SqlValue> = vec![];
    if let Some(n) = &req.name {
        sets.push("name = ?");
        args.push(sval(n.trim().to_string()));
    }
    if sets.is_empty() {
        return Err(ApiError::bad("无更新字段"));
    }
    sets.push("updated_at = ?");
    args.push(sval(now_iso()));
    args.push(sval(auth.0.sub.clone()));
    args.push(sval(st.tenant.clone()));
    st.db
        .execute(&format!("UPDATE members SET {} WHERE id = ? AND tenant_id = ?", sets.join(", ")), args)
        .await
        .map_err(|e| ApiError::bad(format!("更新失败：{e}")))?;
    ok(json!({ "updated": true }))
}

fn internal(e: impl std::fmt::Display) -> ApiError {
    ApiError::bad(format!("行解析失败：{e}"))
}

// 防止未使用告警
#[allow(dead_code)]
fn _use(_: Auth) {}


// ─────────────────────── 会员档案时间线（P0-3）───────────────────────

/// 订单类型 → 标题
fn order_title(biz: &str) -> &'static str {
    match biz {
        "plan" => "订阅购买",
        "points_recharge" => "积分充值",
        _ => "订单",
    }
}

fn order_status_label(s: &str) -> &'static str {
    match s {
        "paid" => "已支付",
        "pending" => "待支付",
        "closed" => "已关闭",
        "refunded" => "已退款",
        _ => "状态未知",
    }
}

fn comment_status_label(s: &str) -> &'static str {
    match s {
        "approved" => "已通过",
        "pending" => "待审核",
        "rejected" => "已驳回",
        _ => "",
    }
}

/// 按**字符**截断（不是字节）—— 中文按字节切会切出半个字。
fn clip(s: &str, n: usize) -> String {
    let t: String = s.chars().take(n).collect();
    if s.chars().count() > n {
        format!("{t}...")
    } else {
        t
    }
}

/// 从一行里安全取字符串：列缺失 / NULL 都回落空串。
/// 时间线是「尽力而为」的聚合视图 —— 少一个字段不该让整页失败。
fn gets(r: &crate::cmsdb::Row, col: &str) -> String {
    r.try_get::<String>("", col).unwrap_or_default()
}

fn geti(r: &crate::cmsdb::Row, col: &str) -> i64 {
    r.try_get::<i64>("", col).unwrap_or(0)
}

/// 取多行；查询失败返回空集而不是让整个时间线报错。
async fn rows(db: &crate::cmsdb::CmsDb, sql: &str, args: Vec<SqlValue>) -> Vec<crate::cmsdb::Row> {
    db.query_all_statement(Statement::from_sql_and_values(
        sea_orm::DatabaseBackend::Sqlite,
        sql,
        args,
    ))
    .await
    .unwrap_or_default()
}

/// GET /api/members/{id}/timeline —— 会员档案时间线（后台，需 content.members.view）。
///
/// 为什么是「聚合」而不是让前端分别查四张表：FluentCRM 的 360° 档案之所以有用，
/// 是因为它把分散的行为放在**同一条时间轴**上。「这个人发生过什么」本身就是
/// 信息，而它只存在于合并之后 —— 分四处看就不叫档案。
///
/// 数据来源全部是**已有表**（零新表、零迁移）：`orders` / `points_ledger` /
/// `comments` / `events`，加 `members.created_at` 作为起点。
///
/// **刻意不含表单提交**：`form_submissions` 没有 `member_id`，只能拿 JSON 里的
/// 邮箱做模糊匹配 —— 各表单字段名不统一，匹配必然误配。把别人的提交挂到这个
/// 会员名下比少一项更坏，所以不做。
pub async fn timeline(
    State(st): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
) -> ApiResult {
    ensure(&auth, "content.members.view")?;
    let t = st.tenant.clone();

    let mrow = st
        .db
        .query_one(
            "SELECT id, email, name, plan, status, plan_expires_at, invited_by, created_at
               FROM members WHERE id = ? AND tenant_id = ? LIMIT 1",
            vec![sval(id.clone()), sval(t.clone())],
        )
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    // 会员不存在要 404，而不是给一条空时间线 —— 后者会被读成
    // 「这个人没有任何行为」，与「id 传错了」区分不开。
    let Some(m) = mrow else {
        return Err(ApiError::not_found("会员不存在"));
    };

    let mut ev: Vec<Value> = Vec::new();
    let joined = gets(&m, "created_at");

    // 起点：加入时间。没有它，时间线会显得「凭空开始有订单」。
    if !joined.is_empty() {
        let inviter = gets(&m, "invited_by");
        ev.push(json!({
            "at": joined,
            "kind": "join",
            "title": "加入会员",
            "detail": if inviter.is_empty() { String::new() } else { format!("邀请码 {inviter}") },
            "amountCents": Value::Null,
            "ref": "",
        }));
    }

    // 订单：订阅 / 积分。已支付的用 paid_at 作发生时间 —— 那才是钱到账的时刻，
    // 用 created_at 会把「下单」与「付款」排错顺序。
    for r in rows(
        &st.db,
        "SELECT order_no, biz_type, amount_cents, status, channel, plan_days, paid_at, created_at
           FROM orders WHERE tenant_id = ? AND member_id = ?
          ORDER BY created_at DESC LIMIT 50",
        vec![sval(t.clone()), sval(id.clone())],
    )
    .await
    {
        let status = gets(&r, "status");
        let created = gets(&r, "created_at");
        let paid = gets(&r, "paid_at");
        let at = if status == "paid" && !paid.is_empty() { paid } else { created };
        let days = geti(&r, "plan_days");
        let mut detail = format!(
            "{} · {} · {}",
            gets(&r, "order_no"),
            gets(&r, "channel"),
            order_status_label(&status)
        );
        if days > 0 {
            detail.push_str(&format!(" · {days} 天"));
        }
        ev.push(json!({
            "at": at,
            "kind": "order",
            "title": order_title(&gets(&r, "biz_type")),
            "detail": detail,
            "amountCents": geti(&r, "amount_cents"),
            "ref": gets(&r, "order_no"),
        }));
    }

    // 积分流水
    for r in rows(
        &st.db,
        "SELECT delta, balance_after, reason, note, created_at
           FROM points_ledger WHERE tenant_id = ? AND member_id = ?
          ORDER BY created_at DESC LIMIT 50",
        vec![sval(t.clone()), sval(id.clone())],
    )
    .await
    {
        let delta = geti(&r, "delta");
        let note = gets(&r, "note");
        ev.push(json!({
            "at": gets(&r, "created_at"),
            "kind": "points",
            "title": if delta >= 0 { format!("积分 +{delta}") } else { format!("积分 {delta}") },
            "detail": format!(
                "{}{} · 余额 {}",
                gets(&r, "reason"),
                if note.is_empty() { String::new() } else { format!(" · {note}") },
                geti(&r, "balance_after")
            ),
            "amountCents": Value::Null,
            "ref": "",
        }));
    }

    // 评论（内容互动）
    for r in rows(
        &st.db,
        "SELECT id, article_id, content, status, created_at
           FROM comments WHERE tenant_id = ? AND member_id = ?
          ORDER BY created_at DESC LIMIT 50",
        vec![sval(t.clone()), sval(id.clone())],
    )
    .await
    {
        let status = gets(&r, "status");
        ev.push(json!({
            "at": gets(&r, "created_at"),
            "kind": "comment",
            "title": "发表评论",
            "detail": format!("{} · {}", clip(&gets(&r, "content"), 60), comment_status_label(&status)),
            "amountCents": Value::Null,
            "ref": gets(&r, "article_id"),
        }));
    }

    // 会员域事件。**必须带类型白名单**：events 表的 ref_id 是多义的
    // （文章浏览事件里它是文章 ID），不设白名单就会把「id 恰好相同」的
    // 无关事件挂到这个会员名下 —— 时间线上出现别人的行为比空着更坏。
    for r in rows(
        &st.db,
        "SELECT type, ref_key, created_at FROM events
          WHERE tenant_id = ? AND ref_id = ? AND type LIKE 'member.%'
          ORDER BY created_at DESC LIMIT 50",
        vec![sval(t.clone()), sval(id.clone())],
    )
    .await
    {
        let ty = gets(&r, "type");
        ev.push(json!({
            "at": gets(&r, "created_at"),
            "kind": "event",
            "title": clip(ty.trim_start_matches("member."), 40),
            "detail": gets(&r, "ref_key"),
            "amountCents": Value::Null,
            "ref": gets(&r, "ref_key"),
        }));
    }

    // 倒序：最新的在前。ISO 时间串可直接字典序比较（本项目统一写 UTC ISO）。
    ev.sort_by(|a, b| {
        let ka = a.get("at").and_then(|v| v.as_str()).unwrap_or("");
        let kb = b.get("at").and_then(|v| v.as_str()).unwrap_or("");
        kb.cmp(ka)
    });
    let total = ev.len();
    ev.truncate(100);

    let expires = gets(&m, "plan_expires_at");
    // P0-4 打的标签落在会员身上，档案是它唯一的出口 ——
    // 标签不给任何人看，就等于没打过。
    let tags = crate::smart_links::member_tags(&st, &id).await;
    ok(json!({
        "member": {
            "id": gets(&m, "id"),
            "email": gets(&m, "email"),
            "name": gets(&m, "name"),
            "plan": gets(&m, "plan"),
            "status": geti(&m, "status"),
            "planExpiresAt": expires,
            "invitedBy": gets(&m, "invited_by"),
            "createdAt": joined,
        },
        "tags": tags,
        "events": ev,
        "total": total,
    }))
}
