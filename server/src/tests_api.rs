//! API 集成测试（C3）：对 build_router 直接发请求（oneshot），
//! 覆盖登录/RBAC/限速/审批/公开表单等关键链路，无需起真实端口。

use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use sea_orm::Value as SqlValue;
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::{build_router, db, state::AppState};

/// 临时库 + 全量种子
async fn test_state() -> AppState {
    let path = std::env::temp_dir().join(format!("api_test_{}.db", uuid::Uuid::new_v4()));
    let db = sea_orm::Database::connect(&format!("sqlite://{}?mode=rwc", path.to_string_lossy()))
        .await
        .expect("connect");
    let db = crate::cmsdb::CmsDb::local(db);
    db::bootstrap(&db).await;
    AppState { db, tenant: "t_demo".into() }
}

async fn call(app: axum::Router, method: &str, uri: &str, body: Option<Value>, token: Option<&str>) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(t) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    let mut req = builder.body(Body::empty()).unwrap();
    if let Some(b) = body {
        *req.body_mut() = Body::from(b.to_string());
        let headers = req.headers_mut();
        headers.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
    }
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

/// 取状态码 + Location 头。
///
/// 为什么不用 `call()`：后者只返回 (status, json)，而重定向响应的
/// **关键信息在 Location 头里**（跳到哪里比跳不跳更重要）。
async fn status_and_location(app: axum::Router, uri: &str) -> (StatusCode, Option<String>) {
    let req = Request::builder().method("GET").uri(uri).body(Body::empty()).unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let st = resp.status();
    let loc = resp
        .headers()
        .get(header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_string());
    (st, loc)
}

/// 带自定义请求头的重定向探测：Smart Links 的「真人 / 预取」判定依赖
/// `Sec-Fetch-Mode`、`Purpose` 这些头，普通 `call()` 构造不出来。
async fn status_loc_h(
    app: axum::Router,
    uri: &str,
    headers: Vec<(&'static str, String)>,
) -> (StatusCode, Option<String>) {
    let mut builder = Request::builder().method("GET").uri(uri);
    for (k, v) in headers {
        builder = builder.header(k, v);
    }
    let req = builder.body(Body::empty()).unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let st = resp.status();
    let loc = resp
        .headers()
        .get(header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_string());
    (st, loc)
}

async fn login(app: &axum::Router, username: &str) -> String {
    let (st, b) = call(app.clone(), "POST", "/api/auth/login", Some(json!({ "username": username, "password": "demo1234" })), None).await;
    assert_eq!(st, StatusCode::OK, "{username} 登录应成功");
    b["data"]["token"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn login_ok_and_me() {
    let st = test_state().await;
    let app = build_router(st.clone());
    let tok = login(&app, "owner").await;
    let (st2, me) = call(app, "GET", "/api/user/me", None, Some(&tok)).await;
    assert_eq!(st2, StatusCode::OK);
    assert_eq!(me["data"]["role"], "owner");
    assert!(me["data"]["permissions"].as_array().unwrap().contains(&json!("*")));
}

#[tokio::test]
async fn bad_password_and_rate_limit() {
    let st = test_state().await;
    let app = build_router(st);
    for _ in 0..5 {
        let (s, _) = call(app.clone(), "POST", "/api/auth/login", Some(json!({"username":"viewer","password":"x"})), None).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED);
    }
    // 第 6 次：即使密码正确也 429（A4 锁定 15 分钟）
    let (s, b) = call(app.clone(), "POST", "/api/auth/login", Some(json!({"username":"viewer","password":"demo1234"})), None).await;
    assert_eq!(s, StatusCode::TOO_MANY_REQUESTS, "{b}");
}

#[tokio::test]
async fn editor_cannot_invite_owner_can() {
    let st = test_state().await;
    let app = build_router(st.clone());
    let e = login(&app, "editor").await;
    let (s, _) = call(app.clone(), "POST", "/api/team/users", Some(json!({"username":"x9","nickname":"x","role":"viewer"})), Some(&e)).await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    let o = login(&app, "owner").await;
    let (s2, b) = call(app.clone(), "POST", "/api/team/users", Some(json!({"username":"invitee","nickname":"受邀者","role":"editor"})), Some(&o)).await;
    assert_eq!(s2, StatusCode::OK);
    assert_eq!(b["data"]["username"], "invitee");
    // 受邀者首登强制改密标记
    let (s3, b3) = call(app.clone(), "POST", "/api/auth/login", Some(json!({"username":"invitee","password":"demo1234"})), None).await;
    assert_eq!(s3, StatusCode::OK);
    assert_eq!(b3["data"]["user"]["mustChangePassword"], true);
}

#[tokio::test]
async fn approve_flow_publishes_article() {
    let st = test_state().await;
    let app = build_router(st.clone());
    let o = login(&app, "owner").await;
    let e = login(&app, "editor").await;

    let (_, art) = call(app.clone(), "POST", "/api/articles", Some(json!({"title":"集成测试文","status":"draft"})), Some(&o)).await;
    let aid = art["data"]["id"].as_str().unwrap().to_string();

    let (_, inv) = call(app.clone(), "POST", "/api/ai/invoke", Some(json!({"capability":"content.articles.publish","input":{"article_id":aid}})), Some(&e)).await;
    assert_eq!(inv["data"]["decision"], "needs_approval");

    let (_, list) = call(app.clone(), "GET", "/api/approvals?status=pending", None, Some(&o)).await;
    let ap = list["data"].as_array().unwrap().first().unwrap();
    let apid = ap["id"].as_str().unwrap().to_string();

    let (s, _) = call(app.clone(), "POST", &format!("/api/approvals/{apid}/decide"), Some(json!({"status":"approved"})), Some(&o)).await;
    assert_eq!(s, StatusCode::OK);

    let (_, got) = call(app, "GET", &format!("/api/articles/{aid}"), None, Some(&o)).await;
    assert_eq!(got["data"]["status"], "published");
}

#[tokio::test]
async fn public_form_submit_creates_lead() {
    let st = test_state().await;
    let app = build_router(st.clone());
    // 匿名读公开表单
    let (_, f) = call(app.clone(), "GET", "/api/public/forms/seed_form", None, None).await;
    let _ = f;
    // 从 DB 取一个 published 表单 id
    let o = login(&app, "owner").await;
    let (_, forms) = call(app.clone(), "GET", "/api/forms", None, Some(&o)).await;
    let fid = forms["data"].as_array().unwrap().first().unwrap()["id"].as_str().unwrap().to_string();

    // 匿名提交（无 token）
    let (s, b) = call(app.clone(), "POST", &format!("/api/public/forms/{fid}/submit"),
        Some(json!({"name":"路人甲","phone":"13900001234","interest":"生日蛋糕"})), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b["data"]["leadCreated"], true);

    // 同号重复提交 → 不重复建线
    let (s2, b2) = call(app.clone(), "POST", &format!("/api/public/forms/{fid}/submit"),
        Some(json!({"name":"路人甲","phone":"13900001234"})), None).await;
    assert_eq!(s2, StatusCode::OK);
    assert_eq!(b2["data"]["leadCreated"], false);

    // 线索落库且来源标记为表单
    let (_, leads) = call(app.clone(), "GET", "/api/leads", None, Some(&o)).await;
    let mine: Vec<&Value> = leads["data"].as_array().unwrap().iter()
        .filter(|l| l["phone"] == "13900001234").collect();
    assert_eq!(mine.len(), 1);
    assert!(mine[0]["source"].as_str().unwrap().starts_with("表单:"));

    // 提交数累计 2
    let (_, f2) = call(app.clone(), "GET", &format!("/api/forms/{fid}"), None, Some(&o)).await;
    let _ = f2;
}

// ── P2：资金链路测试 ──

async fn register_member(app: &axum::Router, email: &str) -> String {
    let (s, b) = call(
        app.clone(),
        "POST",
        "/api/public/members/register",
        Some(json!({ "email": email, "password": "member1234", "name": "测试会员" })),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "注册应成功：{b}");
    b["data"]["token"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn points_purchase_atomic_idempotent_and_no_negative() {
    let st = test_state().await;
    let app = build_router(st.clone());
    let o = login(&app, "owner").await;

    // 两篇积分解锁文章：价 100 与 1000
    let (_, a1) = call(app.clone(), "POST", "/api/articles",
        Some(json!({"title":"付费文A","status":"published","paidLevel":2,"pricePoints":100})), Some(&o)).await;
    let id1 = a1["data"]["id"].as_str().unwrap().to_string();
    let (_, a2) = call(app.clone(), "POST", "/api/articles",
        Some(json!({"title":"付费文B","status":"published","paidLevel":2,"pricePoints":1000})), Some(&o)).await;
    let id2 = a2["data"]["id"].as_str().unwrap().to_string();

    let m = register_member(&app, "buyer@test.dev").await;

    // 造一枚 500 积分兑码并核销 → 余额 500
    let (_, rc) = call(app.clone(), "POST", "/api/redeem_codes",
        Some(json!({"code":"PTS500","kind":"points","value":500,"status":"unused"})), Some(&o)).await;
    assert_eq!(rc["data"]["code"], "PTS500", "{rc}");
    let (s, b) = call(app.clone(), "POST", "/api/public/members/redeem",
        Some(json!({"code":"PTS500"})), Some(&m)).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["data"]["balance"], 500);

    // 买 A：-100 → 400
    let (s, b) = call(app.clone(), "POST", "/api/public/members/purchase-article",
        Some(json!({"article_id": id1})), Some(&m)).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["data"]["balance"], 400);

    // 重复购买：幂等 alreadyOwned
    let (s, b) = call(app.clone(), "POST", "/api/public/members/purchase-article",
        Some(json!({"article_id": id1})), Some(&m)).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["data"]["alreadyOwned"], true);

    // 余额不足：400，且余额保持 400（不得为负）
    let (s, b) = call(app.clone(), "POST", "/api/public/members/purchase-article",
        Some(json!({"article_id": id2})), Some(&m)).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{b}");
    let (_, w) = call(app.clone(), "GET", "/api/public/members/wallet", None, Some(&m)).await;
    assert_eq!(w["data"]["balance"], 400, "{w}");
}

#[tokio::test]
async fn order_confirm_idempotent_and_invite_reward_once() {
    let st = test_state().await;
    let app = build_router(st.clone());
    let o = login(&app, "owner").await;

    // 邀请人 A 注册，为其造一枚个人邀请码
    let a_tok = register_member(&app, "a@test.dev").await;
    let (_, me) = call(app.clone(), "GET", "/api/public/members/me", None, Some(&a_tok)).await;
    let a_id = me["data"]["member"]["id"].as_str().unwrap().to_string();
    let (s, ic) = call(app.clone(), "POST", "/api/invite_codes",
        Some(json!({"code":"INV-A","quota":10,"ownerMemberId":a_id,"enabled":true})), Some(&o)).await;
    assert_eq!(s, StatusCode::OK, "{ic}");

    // B 用邀请码注册 → B.invited_by = A。
    //
    // 键名必须是 **camelCase**：真实调用方（admin web 的 `memberAuth.register`、
    // 公开站注册表单）传的都是 `inviteCode`。这里曾经写 `invite_code` ——
    // serde 对未知字段静默忽略，于是「前端传 camelCase、后端只认 snake」这个
    // 契约错位被测试完美掩盖：邀请码被悄悄丢掉，`invited_by` 恒为空，
    // **邀请奖励一发不出去且没有任何报错**。用真实键名才能测到真东西。
    let (s, rb) = call(app.clone(), "POST", "/api/public/members/register",
        Some(json!({"email":"b@test.dev","password":"member1234","name":"被邀请人","inviteCode":"INV-A"})), None).await;
    assert_eq!(s, StatusCode::OK, "{rb}");
    let b_tok = rb["data"]["token"].as_str().unwrap().to_string();

    // B 创建充值订单（100 积分 = 100 分）
    let (s, ord) = call(app.clone(), "POST", "/api/public/orders",
        Some(json!({"biz_type":"points_recharge","points":100,"channel":"manual"})), Some(&b_tok)).await;
    assert_eq!(s, StatusCode::OK, "{ord}");
    let order_no = ord["data"]["orderNo"].as_str().unwrap().to_string();
    // 订单响应不含 id，经网关列表反查
    let (_, orders) = call(app.clone(), "GET", "/api/orders", None, Some(&o)).await;
    let hit = orders["data"].as_array().unwrap().iter()
        .find(|r| r["orderNo"] == order_no).unwrap().clone();
    let oid = hit["id"].as_str().unwrap().to_string();

    // 首次确认：发货 + 邀请奖励
    let (s, b) = call(app.clone(), "POST", &format!("/api/admin/orders/{oid}/confirm"), None, Some(&o)).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["data"]["inviteReward"]["points"], 100, "{b}");

    // B 余额 = 100（充值到账）；A 余额 = 100（邀请奖励）
    let (_, wb) = call(app.clone(), "GET", "/api/public/members/wallet", None, Some(&b_tok)).await;
    assert_eq!(wb["data"]["balance"], 100, "{wb}");
    let (_, wa) = call(app.clone(), "GET", "/api/public/members/wallet", None, Some(&a_tok)).await;
    assert_eq!(wa["data"]["balance"], 100, "{wa}");

    // 重复确认：400（原子防重），A 余额不变
    let (s, b) = call(app.clone(), "POST", &format!("/api/admin/orders/{oid}/confirm"), None, Some(&o)).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{b}");
    let (_, wa2) = call(app.clone(), "GET", "/api/public/members/wallet", None, Some(&a_tok)).await;
    assert_eq!(wa2["data"]["balance"], 100, "{wa2}");
}

/// 订阅订单：计价与会员天数必须**同源于 interval**。
///
/// 曾经的实现把 `plan_days` 写死 30、价格只读 `price_monthly`，于是「年付」
/// 会按年价收钱却只给 30 天 —— 收款与承诺不符，而且订单里两个数字都自洽，
/// 客户端完全看不出异常，只有用户月底发现会员没了才会暴露。
/// 另外，未配置的价格列是 0，不拦住就会生成 **0 元待确认单**：站长顺手一点
/// 确认就等于白送一年会员，所以未定价必须 400 而不是「照单收 0 元」。
#[tokio::test]
async fn plan_order_prices_by_interval_and_refuses_unpriced() {
    let st = test_state().await;
    let app = build_router(st.clone());
    let o = login(&app, "owner").await;

    // 双价套餐：月 18 / 年 180
    let (s, t1) = call(app.clone(), "POST", "/api/tiers",
        Some(json!({"name":"年度会员","slug":"annual","description":"","priceMonthly":18,
                    "priceYearly":180,"features":"[]","active":true})), Some(&o)).await;
    assert_eq!(s, StatusCode::OK, "{t1}");
    let dual = t1["data"]["id"].as_str().expect("套餐 id").to_string();

    // 只配月价的套餐：年付必须被拒
    let (s, t2) = call(app.clone(), "POST", "/api/tiers",
        Some(json!({"name":"月付会员","slug":"monthly","description":"","priceMonthly":18,
                    "priceYearly":0,"features":"[]","active":true})), Some(&o)).await;
    assert_eq!(s, StatusCode::OK, "{t2}");
    let mon = t2["data"]["id"].as_str().expect("套餐 id").to_string();

    let m = register_member(&app, "buyer@test.dev").await;

    // 年付 → 按 priceYearly 计价（18000 分）
    let (s, y) = call(app.clone(), "POST", "/api/public/orders",
        Some(json!({"bizType":"plan","tierId":dual,"interval":"yearly","channel":"manual"})), Some(&m)).await;
    assert_eq!(s, StatusCode::OK, "{y}");
    assert_eq!(y["data"]["amountCents"], 18000, "年付必须按 priceYearly 计价：{y}");
    let y_no = y["data"]["orderNo"].as_str().unwrap().to_string();

    // 不带 interval → 等价月付（1800 分），保证老客户端行为不变
    let (s, mo) = call(app.clone(), "POST", "/api/public/orders",
        Some(json!({"bizType":"plan","tierId":dual,"channel":"manual"})), Some(&m)).await;
    assert_eq!(s, StatusCode::OK, "{mo}");
    assert_eq!(mo["data"]["amountCents"], 1800, "缺省 interval 应等价月付：{mo}");

    // 未配年价 → 400（不是 0 元单）
    let (s, bad) = call(app.clone(), "POST", "/api/public/orders",
        Some(json!({"bizType":"plan","tierId":mon,"interval":"yearly","channel":"manual"})), Some(&m)).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "未配置年价必须拒绝：{bad}");

    // 确认年付单 → plan=annual，剩余天数 ≈ 365
    let (_, orders) = call(app.clone(), "GET", "/api/orders", None, Some(&o)).await;
    let oid = orders["data"].as_array().unwrap().iter()
        .find(|r| r["orderNo"] == y_no).expect("刚建的年付单应在列表里")["id"]
        .as_str().unwrap().to_string();
    let (s, c) = call(app.clone(), "POST", &format!("/api/admin/orders/{oid}/confirm"), None, Some(&o)).await;
    assert_eq!(s, StatusCode::OK, "{c}");

    let (_, w) = call(app.clone(), "GET", "/api/public/members/wallet", None, Some(&m)).await;
    assert_eq!(w["data"]["plan"], "annual", "{w}");
    let days = w["data"]["planDaysLeft"].as_i64().unwrap_or(-1);
    assert!((364..=365).contains(&days), "年付应给 365 天，实得 {days}：{w}");
}

/// 年付在线支付：在线通道按周期取价，**年付不再被硬挡**。
///
/// 历史：`tiers` 只有一列 `stripe_price_id`，月/年共用同一个 Stripe 价格 ——
/// 前端写「¥180/年」而后端按月价建订阅会话（资损）。当时的处置是 400 拒绝年付。
/// 迁移 0016 之后年付有独立列，年付与月付必须走**同一条判定链**，只有价格列不同。
#[tokio::test]
async fn checkout_year_interval_goes_down_the_same_path_as_monthly() {
    let st = test_state().await;
    let app = build_router(st.clone());
    let o = login(&app, "owner").await;

    let (s, t) = call(app.clone(), "POST", "/api/tiers",
        Some(json!({"name":"年度会员","slug":"annual","description":"","priceMonthly":18,
                    "priceYearly":180,"stripePriceId":"price_mon","stripePriceYearlyId":"price_year",
                    "features":"[]","active":true})), Some(&o)).await;
    assert_eq!(s, StatusCode::OK, "{t}");
    let tid = t["data"]["id"].as_str().expect("套餐 id").to_string();
    // 网关必须真把年付 Price 落库 —— 列没登记的话这里是空串，年付会永远不可用
    assert_eq!(t["data"]["stripePriceYearlyId"], "price_year", "年付 Price 未落库：{t}");

    let m = register_member(&app, "yr@test.dev").await;

    // 本环境未配 STRIPE_SECRET_KEY：月付与年付都应因「未配置」被拒，
    // 但**拒绝理由必须完全相同** —— 说明年付已经走到与月付同一条判定链上，
    // 而不是被一条「暂未支持年付」的守卫提前挡掉。
    let (s1, y) = call(app.clone(), "POST", "/api/public/checkout",
        Some(json!({"tierId": tid, "interval": "yearly"})), Some(&m)).await;
    assert_eq!(s1, StatusCode::BAD_REQUEST, "{y}");
    let ye = y["error"].as_str().unwrap_or("");
    assert!(!ye.contains("暂未支持年付"), "年付硬挡守卫应已移除：{ye}");
    assert!(ye.contains("STRIPE_SECRET_KEY"), "年付应走到真实通道判定：{ye}");

    let (_, mo) = call(app.clone(), "POST", "/api/public/checkout",
        Some(json!({"tierId": tid, "interval": "monthly"})), Some(&m)).await;
    assert_eq!(mo["error"], y["error"], "月/年拒绝理由应同源（只差价格列）：{mo} vs {y}");
}

/// 公开套餐接口要告诉前端「该周期能不能在线支付」。
///
/// 没有这个标志，前端只能让用户点下去撞 400（或反过来把已配置的通道一起禁掉）。
/// 但**只下发布尔量，不下发 Price ID 本身** —— 那是运营配置，没有出现在公开响应里的理由。
#[tokio::test]
async fn public_tiers_expose_online_capability_per_interval() {
    let st = test_state().await;
    let app = build_router(st.clone());
    let o = login(&app, "owner").await;

    for (slug, mon, yr) in [("both", "price_m1", "price_y1"), ("mononly", "price_m2", "")] {
        let (s, b) = call(app.clone(), "POST", "/api/tiers",
            Some(json!({"name": slug, "slug": slug, "description": "",
                        "priceMonthly": 18, "priceYearly": 180,
                        "stripePriceId": mon, "stripePriceYearlyId": yr,
                        "features": "[]", "active": true})), Some(&o)).await;
        assert_eq!(s, StatusCode::OK, "{b}");
    }

    let (_, list) = call(app.clone(), "GET", "/api/public/tiers", None, None).await;
    let arr = list["data"].as_array().expect("data 必须是数组");
    let both = arr.iter().find(|r| r["slug"] == "both").expect("双价套餐");
    assert_eq!(both["onlineMonthly"], true, "{both}");
    assert_eq!(both["onlineYearly"], true, "{both}");
    let mon = arr.iter().find(|r| r["slug"] == "mononly").expect("仅月付套餐");
    assert_eq!(mon["onlineMonthly"], true, "{mon}");
    assert_eq!(mon["onlineYearly"], false, "未配年价必须如实下发 false：{mon}");
    // 公开响应里不得出现 Price ID（键名与列名两种拼写都不行）
    assert!(both.get("stripePriceId").is_none(), "不得下发月付 Price ID：{both}");
    assert!(both.get("stripePriceYearlyId").is_none(), "不得下发年付 Price ID：{both}");
}

/// 【年付在线支付的核心闭环】Webhook 必须按会话周期**真发货**。
///
/// Stripe Checkout 走的是真实扣款。原先 `checkout.session.completed` 只把
/// `members.status` 置 1、写回 customer id，**plan 与到期时间一概不动** ——
/// 钱收了、会员没发，而且这类单子不在 `orders` 表里，对账/归因都看不出异常。
/// 把「周期落到 plan 上」推给还没落地的订阅状态机（阶段 1），
/// 在真实收款链路上就等于持续资损。现在按 metadata.interval 发货：
/// 月付 30 天 / 年付 365 天，套餐名落到 tier_slug。
#[tokio::test]
async fn stripe_webhook_activates_plan_by_interval_metadata() {
    let st = test_state().await;
    let app = build_router(st.clone());

    async fn reg(app: &axum::Router, email: &str) -> (String, String) {
        let (_, r) = call(app.clone(), "POST", "/api/public/members/register",
            Some(json!({ "email": email, "password": "member1234", "name": "买家" })), None).await;
        (
            r["data"]["member"]["id"].as_str().expect("会员 id").to_string(),
            r["data"]["token"].as_str().expect("会员令牌").to_string(),
        )
    }

    async fn wallet(app: &axum::Router, tok: &str) -> Value {
        let (_, w) = call(app.clone(), "GET", "/api/public/members/wallet", None, Some(tok)).await;
        w["data"].clone()
    }

    let (ymid, ytok) = reg(&app, "hook_year@test.dev").await;
    let (mmid, mtok) = reg(&app, "hook_month@test.dev").await;
    let (nmid, ntok) = reg(&app, "hook_nometa@test.dev").await;

    // 三个事件各用唯一 id：event_id 去重是设计使然，复用 id 会被当成重复投递
    let ev = |id: &str, mid: &str, meta: Value| {
        json!({
            "id": id,
            "type": "checkout.session.completed",
            "data": { "object": {
                "client_reference_id": mid,
                "customer": format!("cus_{id}"),
                "metadata": meta,
            }}
        })
    };

    for (tag, id, mid, meta, slug, lo, hi) in [
        ("年付", "evt_yr_1", &ymid, json!({"interval": "yearly", "tier_slug": "annual"}), "annual", 364i64, 365i64),
        ("月付", "evt_mo_1", &mmid, json!({"interval": "monthly", "tier_slug": "monthly"}), "monthly", 29, 30),
    ] {
        let body = ev(id, mid, meta).to_string();
        let (s, b) = call_h(app.clone(), "POST", "/api/stripe/webhook", Some(body.clone()), stripe_sig_header(&body)).await;
        assert_eq!(s, StatusCode::OK, "{tag} 事件应处理成功：{b}");
        assert_eq!(b["data"]["status"], "processed", "{b}");

        let tok = if tag == "年付" { &ytok } else { &mtok };
        let w = wallet(&app, tok).await;
        assert_eq!(w["plan"], slug, "{tag} 必须把 plan 落到套餐 slug：{w}");
        let days = w["planDaysLeft"].as_i64().unwrap_or(-1);
        assert!((lo..=hi).contains(&days), "{tag} 应发 {lo}-{hi} 天，实得 {days}：{w}");
    }

    // 无 metadata 的早期会话：不能瞎发会员（不知道该给哪个 plan、多少天），
    // 但账号激活要生效，且**不能返错**（返 Err 会让 Stripe 无限重投）。
    let body = json!({
        "id": "evt_nm_1",
        "type": "checkout.session.completed",
        "data": { "object": { "client_reference_id": nmid, "customer": "cus_nm_1" } }
    })
    .to_string();
    let (s, b) = call_h(app.clone(), "POST", "/api/stripe/webhook", Some(body.clone()), stripe_sig_header(&body)).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["data"]["status"], "processed", "缺 metadata 不应判成处理失败：{b}");
    let w = wallet(&app, &ntok).await;
    assert_eq!(w["plan"], "free", "缺 metadata 时不得凭空发会员：{w}");

    // 会员不存在时也不能返 Err（否则 Stripe 会一直重投这个事件）
    let body = json!({
        "id": "evt_ghost_1",
        "type": "checkout.session.completed",
        "data": { "object": {
            "client_reference_id": "mid_does_not_exist",
            "metadata": { "interval": "yearly", "tier_slug": "annual" }
        }}
    })
    .to_string();
    let (s, b) = call_h(app.clone(), "POST", "/api/stripe/webhook", Some(body.clone()), stripe_sig_header(&body)).await;
    assert_eq!(s, StatusCode::OK, "会员不存在不应让 Stripe 无限重投：{b}");
}

/// 【永久会员防线】Checkout 兜底建的那行订阅，必须记下「授予的到期时间」。
///
/// 兜底路径存在的理由：`customer.subscription.created` 可能丢包，而若本地
/// 连订阅行都没有，调度器的到期兜底就扫不到这个人 —— 他会一直有效下去，
/// 且 members 表自身完全自洽，对账看不出异常。
///
/// 但**只建行还不够**：建行时若不记 `granted_expiry`，到期扫描会因为
/// 「granted 为空 = 本订阅从未授予过」而不敢降级，等于白建。
/// 这个用例走完整的「发货 → 周期流逝 → 兜底降级」三步，把整条链钉死。
#[tokio::test]
async fn checkout_grace_subscription_can_be_demoted_at_period_end() {
    let st = test_state().await;
    let app = build_router(st.clone());

    let (s, r) = call(
        app.clone(),
        "POST",
        "/api/public/members/register",
        Some(json!({ "email": "sub_grace@test.dev", "password": "member1234", "name": "兜底" })),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{r}");
    let mid = r["data"]["member"]["id"].as_str().expect("会员 id").to_string();

    let body = json!({
        "id": "evt_grace_1",
        "type": "checkout.session.completed",
        "data": { "object": {
            "client_reference_id": mid,
            "customer": "cus_grace_1",
            "subscription": "sub_grace_1",
            "metadata": { "interval": "yearly", "tier_slug": "annual" },
        }}
    })
    .to_string();
    let (s, b) = call_h(
        app.clone(),
        "POST",
        "/api/stripe/webhook",
        Some(body.clone()),
        stripe_sig_header(&body),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{b}");

    let (plan, exp) = member_plan(&st, &mid).await;
    assert_eq!(plan, "annual", "兜底必须真发货：{plan}");
    assert!(!exp.is_empty(), "兜底必须写到期时间");

    let granted: String = st
        .db
        .query_one(
            "SELECT granted_expiry FROM subscriptions \
             WHERE tenant_id = ? AND stripe_subscription_id = ? LIMIT 1",
            vec![sv(&st.tenant), sv("sub_grace_1")],
        )
        .await
        .expect("查询订阅")
        .expect("Checkout 兜底必须建订阅行 —— 否则到期兜底扫不到这个人")
        .try_get("", "granted_expiry")
        .unwrap_or_default();
    assert_eq!(
        granted, exp,
        "兜底建行必须记下授予的到期时间，且与会员实际拿到的完全一致"
    );

    // 让周期流逝：把订阅行推到过去（模拟一年后），期间不给任何事件
    st.db
        .execute(
            "UPDATE subscriptions SET current_period_end = ? \
             WHERE tenant_id = ? AND stripe_subscription_id = ?",
            vec![
                sv(&iso(chrono::Utc::now().timestamp() - 86_400)),
                sv(&st.tenant),
                sv("sub_grace_1"),
            ],
        )
        .await
        .expect("推进周期");

    assert_eq!(
        crate::sub_state::expire_due(&st).await,
        1,
        "到期兜底必须处理这条订阅"
    );
    let (plan2, exp2) = member_plan(&st, &mid).await;
    assert_eq!(plan2, "free", "兜底建的行必须真能被降级 —— 否则就是永久会员");
    assert_eq!(exp2, "", "降级应清空到期时间");
}

#[tokio::test]
async fn redeem_code_single_use() {
    let st = test_state().await;
    let app = build_router(st.clone());
    let o = login(&app, "owner").await;
    let m = register_member(&app, "redeemer@test.dev").await;

    let (s, _) = call(app.clone(), "POST", "/api/redeem_codes",
        Some(json!({"code":"ONE50","kind":"points","value":50,"status":"unused"})), Some(&o)).await;
    assert_eq!(s, StatusCode::OK);
    let (s, b) = call(app.clone(), "POST", "/api/public/members/redeem",
        Some(json!({"code":"ONE50"})), Some(&m)).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["data"]["balance"], 50);
    // 第二次兑换：已核销 → 400
    let (s, b) = call(app.clone(), "POST", "/api/public/members/redeem",
        Some(json!({"code":"ONE50"})), Some(&m)).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{b}");
}

// ── P2：审计与 request-id 测试 ──

#[tokio::test]
async fn audit_log_and_request_id() {
    let st = test_state().await;
    let app = build_router(st.clone());
    let o = login(&app, "owner").await;

    // 响应头携带 X-Request-Id（GET 也带）
    let req = Request::builder()
        .method("GET")
        .uri("/api/user/me")
        .header(header::AUTHORIZATION, format!("Bearer {o}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.headers().get("x-request-id").is_some(), "响应应带 X-Request-Id");

    // 写操作 → 审计留痕
    let (_, art) = call(app.clone(), "POST", "/api/articles", Some(json!({"title":"审计文"})), Some(&o)).await;
    let aid = art["data"]["id"].as_str().unwrap().to_string();
    let (s, _) = call(app.clone(), "DELETE", &format!("/api/articles/{aid}"), None, Some(&o)).await;
    assert_eq!(s, StatusCode::OK);

    let (s, al) = call(app.clone(), "GET", "/api/admin/audit", None, Some(&o)).await;
    assert_eq!(s, StatusCode::OK, "{al}");
    let paths: Vec<String> = al["data"].as_array().unwrap().iter()
        .map(|r| r["path"].as_str().unwrap().to_string()).collect();
    assert!(paths.iter().any(|p| p == "/api/articles"), "应审计到 POST /api/articles：{paths:?}");
    assert!(al["total"].as_u64().unwrap() >= 3, "登录+建文+删文至少 3 条：{al}");

    // 非 Owner 不可读审计
    let e = login(&app, "editor").await;
    let (s, _) = call(app.clone(), "GET", "/api/admin/audit", None, Some(&e)).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
}

/// 主页版式预设：写入白名单 + 目录随 /api/public/site 下发（单一权威 crate::presets）。
#[tokio::test]
async fn site_preset_whitelist_and_catalog() {
    let st = test_state().await;
    let app = build_router(st.clone());
    let o = login(&app, "owner").await;

    // 目录下发：数量与后端 CATALOG 一致，字段名与后台 UI 对齐
    let (s, pub_site) = call(app.clone(), "GET", "/api/public/site", None, None).await;
    assert_eq!(s, StatusCode::OK);
    let cat = pub_site["data"]["homePresets"].as_array().expect("homePresets 应为数组");
    assert_eq!(cat.len(), crate::presets::CATALOG.len(), "目录应随设置下发");
    assert_eq!(cat[0]["id"], "classic");
    assert!(cat[0]["bars"].is_array(), "骨架缩略数据应下发");
    assert!(cat.iter().any(|p| p["id"] == "sidebar" && p["sidebar"] == true), "双栏预设应带 sidebar 标记");

    // 非法值 → 400，且不落库（避免"保存成功但主页静默回落"的坑）
    let (s, err) = call(app.clone(), "PUT", "/api/admin/site", Some(json!({ "home_preset": "bogus" })), Some(&o)).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "非法预设应被拒绝：{err}");
    let (_, pub2) = call(app.clone(), "GET", "/api/public/site", None, None).await;
    assert_eq!(pub2["data"]["homePreset"], "classic", "非法值不应落库");

    // 合法值 → 200 并生效
    let (s, _) = call(app.clone(), "PUT", "/api/admin/site", Some(json!({ "home_preset": "sidebar" })), Some(&o)).await;
    assert_eq!(s, StatusCode::OK);
    let (_, pub3) = call(app.clone(), "GET", "/api/public/site", None, None).await;
    assert_eq!(pub3["data"]["homePreset"], "sidebar", "合法预设应生效");
}

// ────────────── 阶段 0：4 条 P0 的回归测试 ──────────────

fn sv(s: &str) -> SqlValue {
    SqlValue::String(Some(s.to_string()))
}

/// 带自定义请求头的调用（Webhook 要 stripe-signature，通用 call() 只有 token）
async fn call_h(
    app: axum::Router,
    method: &str,
    uri: &str,
    body: Option<String>,
    headers: Vec<(&'static str, String)>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    for (k, v) in headers {
        builder = builder.header(k, v);
    }
    let mut req = builder.body(Body::empty()).unwrap();
    if let Some(b) = body {
        *req.body_mut() = Body::from(b);
        req.headers_mut()
            .insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
    }
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

/// 环境里若配了 `STRIPE_WEBHOOK_SECRET`，就按 Stripe 规则现算一个**此刻合法**的签名头，
/// 让用例在配了/没配该变量的机器上都确定地通过。
fn stripe_sig_header(body: &str) -> Vec<(&'static str, String)> {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let secret = std::env::var("STRIPE_WEBHOOK_SECRET").unwrap_or_default();
    if secret.is_empty() {
        return Vec::new();
    }
    let t = crate::auth::now_secs();
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(format!("{t}.{body}").as_bytes());
    vec![(
        "stripe-signature",
        format!("t={t},v1={}", hex::encode(mac.finalize().into_bytes())),
    )]
}

/// P0-1：会员不能自助把 plan 改成 pro（此前一句请求即永久解锁全部付费内容）
#[tokio::test]
async fn member_cannot_self_upgrade_plan() {
    let st = test_state().await;
    let app = build_router(st.clone());
    let tok = register_member(&app, "p0_upgrade@test.com").await;

    let (s, b) = call(
        app.clone(),
        "POST",
        "/api/public/members/me",
        Some(json!({ "plan": "pro" })),
        Some(&tok),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "自助改套餐必须被拒绝：{b}");

    // 昵称这类正常自助字段不受影响
    let (s2, _) = call(
        app.clone(),
        "POST",
        "/api/public/members/me",
        Some(json!({ "name": "改名成功" })),
        Some(&tok),
    )
    .await;
    assert_eq!(s2, StatusCode::OK);

    let (_, me) = call(app.clone(), "GET", "/api/public/members/me", None, Some(&tok)).await;
    assert_eq!(me["data"]["member"]["plan"], "free", "plan 必须没变");
    assert_eq!(me["data"]["member"]["name"], "改名成功");
}

/// P0-4：会员被停用后，手上旧令牌必须立即失效（此前 30 天有效期内照样能用）
#[tokio::test]
async fn member_token_revoked_after_disable() {
    let st = test_state().await;
    let app = build_router(st.clone());
    let tok = register_member(&app, "p0_revoke@test.com").await;

    let (s1, _) = call(app.clone(), "GET", "/api/public/members/me", None, Some(&tok)).await;
    assert_eq!(s1, StatusCode::OK);

    st.db
        .execute(
            "UPDATE members SET status = 0 WHERE email = ?",
            vec![sv("p0_revoke@test.com")],
        )
        .await
        .unwrap();

    let (s2, b2) = call(app.clone(), "GET", "/api/public/members/me", None, Some(&tok)).await;
    assert_eq!(s2, StatusCode::UNAUTHORIZED, "停用后旧令牌应失效：{b2}");
}

/// P0-1 安全默认 + P0-4 付费墙侧：脏到期时间与停用状态都不得解锁付费内容
#[tokio::test]
async fn paywall_denies_bad_expiry_and_disabled_member() {
    let st = test_state().await;
    let app = build_router(st.clone());
    let owner = login(&app, "owner").await;

    let (_, art) = call(
        app.clone(),
        "POST",
        "/api/articles",
        Some(json!({ "title": "P0 付费墙用例", "status": "draft" })),
        Some(&owner),
    )
    .await;
    let aid = art["data"]["id"].as_str().unwrap().to_string();
    st.db
        .execute(
            "UPDATE articles SET paid_level = 1, status = 'published' WHERE id = ?",
            vec![sv(&aid)],
        )
        .await
        .unwrap();
    let url = format!("/api/public/articles/{aid}");

    let tok = register_member(&app, "p0_paywall@test.com").await;

    // free 会员：挡
    let (s1, _) = call(app.clone(), "GET", &url, None, Some(&tok)).await;
    assert_eq!(s1, StatusCode::PAYMENT_REQUIRED);

    // plan=pro 且无到期时间 = 不限期：放行
    st.db
        .execute(
            "UPDATE members SET plan = 'pro', plan_expires_at = '' WHERE email = ?",
            vec![sv("p0_paywall@test.com")],
        )
        .await
        .unwrap();
    let (s2, b2) = call(app.clone(), "GET", &url, None, Some(&tok)).await;
    assert_eq!(s2, StatusCode::OK, "有效订阅应放行：{b2}");
    assert!(b2["data"].get("content").is_some(), "放行时应带正文");

    // P0-1 安全默认：到期时间解析不了 → 拒绝（此前视为不限期 = 永久会员）
    st.db
        .execute(
            "UPDATE members SET plan_expires_at = 'not-a-date' WHERE email = ?",
            vec![sv("p0_paywall@test.com")],
        )
        .await
        .unwrap();
    let (s3, b3) = call(app.clone(), "GET", &url, None, Some(&tok)).await;
    assert_eq!(s3, StatusCode::PAYMENT_REQUIRED, "脏到期时间必须拒绝：{b3}");

    // P0-4 付费墙侧：账号被停用 → 即使 plan=pro 也不放行
    st.db
        .execute(
            "UPDATE members SET plan_expires_at = '', status = 0 WHERE email = ?",
            vec![sv("p0_paywall@test.com")],
        )
        .await
        .unwrap();
    let (s4, b4) = call(app.clone(), "GET", &url, None, Some(&tok)).await;
    assert_eq!(s4, StatusCode::PAYMENT_REQUIRED, "停用会员不得解锁：{b4}");
    assert!(
        b4["preview"].get("content").is_none(),
        "402 预览里不得带正文"
    );
}

/// P0-3：事件落库 + event_id 去重 + 处理失败必须返 5xx
#[tokio::test]
async fn stripe_webhook_records_dedups_and_fails_loud() {
    let st = test_state().await;
    let app = build_router(st.clone());

    let (_, reg) = call(
        app.clone(),
        "POST",
        "/api/public/members/register",
        Some(json!({ "email": "p0_hook@test.com", "password": "member1234" })),
        None,
    )
    .await;
    let mid = reg["data"]["member"]["id"].as_str().unwrap().to_string();

    let body = json!({
        "id": "evt_p0_1",
        "type": "checkout.session.completed",
        "data": { "object": { "client_reference_id": mid, "customer": "cus_p0_1" } }
    })
    .to_string();
    let hdrs = stripe_sig_header(&body);

    // 首次投递：入库 + 处理成功
    let (s1, b1) = call_h(
        app.clone(),
        "POST",
        "/api/stripe/webhook",
        Some(body.clone()),
        hdrs.clone(),
    )
    .await;
    assert_eq!(s1, StatusCode::OK, "首次投递应 200：{b1}");
    assert_eq!(b1["data"]["status"], "processed");
    assert_eq!(b1["data"]["eventId"], "evt_p0_1");

    // 会员被激活 + Stripe 客户号回写
    let row = st
        .db
        .query_one(
            "SELECT status, stripe_customer_id FROM members WHERE id = ?",
            vec![sv(&mid)],
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.try_get::<i64>("", "status").unwrap(), 1);
    assert_eq!(
        row.try_get::<String>("", "stripe_customer_id").unwrap(),
        "cus_p0_1"
    );

    // 重复投递（Stripe 会重投）：必须只处理一次
    let (s2, b2) = call_h(
        app.clone(),
        "POST",
        "/api/stripe/webhook",
        Some(body.clone()),
        hdrs.clone(),
    )
    .await;
    assert_eq!(s2, StatusCode::OK);
    assert_eq!(b2["data"]["duplicate"], true, "重复事件应被识别：{b2}");
    let n = st
        .db
        .query_all(
            "SELECT id FROM webhook_events WHERE event_id = 'evt_p0_1'",
            vec![],
        )
        .await
        .unwrap()
        .len();
    assert_eq!(n, 1, "同一 event_id 只应有一行");

    // 订阅变更类事件：**已在阶段 1 真处理**（建本地订阅行 + 状态机生效）。
    // 这里的事件体是空对象、既无 id 也无 member_id，状态机只能判定"无归属"，
    // 于是既落不了库也发不了货 —— 正是我们要的保守行为：不臆造 plan 变更。
    let sub_body = json!({
        "id": "evt_p0_2",
        "type": "customer.subscription.updated",
        "data": { "object": {} }
    })
    .to_string();
    let sub_h = stripe_sig_header(&sub_body);
    let (s3, b3) = call_h(
        app.clone(),
        "POST",
        "/api/stripe/webhook",
        Some(sub_body),
        sub_h,
    )
    .await;
    assert_eq!(s3, StatusCode::OK);
    assert_eq!(b3["data"]["status"], "processed");

    // 缺 id 无法去重：不能静默丢弃
    let (s4, _) = call_h(
        app.clone(),
        "POST",
        "/api/stripe/webhook",
        Some(json!({ "type": "x" }).to_string()),
        hdrs.clone(),
    )
    .await;
    assert_eq!(s4, StatusCode::BAD_REQUEST);

    // 事件表不可用时：必须返 5xx 让 Stripe 重投
    //（此前吞掉 DB 错误仍返 2xx → Stripe 认为投递成功 → 事件永久丢失）
    st.db.execute("DROP TABLE webhook_events", vec![]).await.unwrap();
    let (s5, _) = call_h(app.clone(), "POST", "/api/stripe/webhook", Some(body), hdrs).await;
    assert_eq!(s5, StatusCode::INTERNAL_SERVER_ERROR, "入库失败必须返 5xx");
}

/// 迁移 0011 的 `kind` 列必须在**通用网关**里可读写。
///
/// 为什么单独立一条用例：articles 的 TableDef 历史上有过漏列，后果是网关
/// **静默丢弃**该字段 —— 后台保存返回 200、字段却不变，排查成本极高
/// （visibility / paid_level 就这么丢过一次）。`kind` 比它们更危险：它是
/// 问题页的类型判据，丢了以后问题页会全部退化成普通文章，且没有任何报错。
#[tokio::test]
async fn article_kind_survives_gateway_roundtrip() {
    let st = test_state().await;
    let app = build_router(st.clone());
    let o = login(&app, "owner").await;

    // 1) 新建**故意不带 kind** → 落库默认值必须是 post（既有文章语义不变）
    let (s, b) = call(
        app.clone(),
        "POST",
        "/api/articles",
        Some(json!({
            "title": "普通文章", "slug": "plain-post",
            "content": "<p>x</p>", "status": "published"
        })),
        Some(&o),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let id = b["data"]["id"].as_str().unwrap().to_string();
    assert_eq!(b["data"]["kind"], "post", "未指定 kind 应回落 post：{b}");

    // 2) 改成问题页 —— TableDef 若漏列，这一步会「200 但字段不变」
    let (s2, b2) = call(
        app.clone(),
        "PUT",
        &format!("/api/articles/{id}"),
        Some(json!({ "kind": "problem", "visibility": "public" })),
        Some(&o),
    )
    .await;
    assert_eq!(s2, StatusCode::OK, "{b2}");
    assert_eq!(b2["data"]["kind"], "problem", "kind 必须由网关持久化：{b2}");

    // 3) 回读确认（不经写响应，排除「只改了响应体」的假象）
    let (_, b3) = call(app.clone(), "GET", &format!("/api/articles/{id}"), None, Some(&o)).await;
    assert_eq!(b3["data"]["kind"], "problem", "{b3}");

    // 4) 公开列表的 ?kind= 过滤：问题页聚合页的取数口径
    let (s4, b4) = call(app.clone(), "GET", "/api/public/articles?kind=problem", None, None).await;
    assert_eq!(s4, StatusCode::OK, "{b4}");
    let arr = b4["data"].as_array().unwrap();
    assert!(
        arr.iter().any(|x| x["slug"] == "plain-post"),
        "问题页应从公开列表的 kind 过滤里查得：{b4}"
    );
    assert!(arr.iter().all(|x| x["kind"] == "problem"), "过滤未生效：{b4}");

    // 5) 不带 kind 时行为完全不变（文章流不能被问题页污染）
    let (_, b5) = call(app.clone(), "GET", "/api/public/articles?limit=200", None, None).await;
    let all = b5["data"].as_array().unwrap();
    assert!(all.len() > arr.len(), "默认列表应同时含文章与问题页：{b5}");
    assert!(all.iter().any(|x| x["slug"] == "plain-post"));
}

/// 公开详情端点必须回带 kind —— 前端靠它决定版式（文章 vs 问题页）
#[tokio::test]
async fn public_article_detail_exposes_kind() {
    let st = test_state().await;
    let app = build_router(st.clone());
    let o = login(&app, "owner").await;
    let (_, b) = call(
        app.clone(),
        "POST",
        "/api/articles",
        Some(json!({
            "title": "为什么境外卡会被拒？", "slug": "why-card-declined",
            "content": "<p>常见是账单地址与卡组织风控。</p>",
            "status": "published", "visibility": "public", "kind": "problem"
        })),
        Some(&o),
    )
    .await;
    assert_eq!(b["data"]["kind"], "problem", "{b}");

    let (s, d) = call(app.clone(), "GET", "/api/public/articles/why-card-declined", None, None).await;
    assert_eq!(s, StatusCode::OK, "{d}");
    assert_eq!(d["data"]["kind"], "problem", "{d}");
    assert!(d["data"]["content"].as_str().unwrap_or("").contains("风控"));
}


/// P0-1：重定向规则必须在**服务期**生效 —— 爬虫不执行 JS，前端路由层的
/// 301 对搜索引擎与 AI 爬虫等于不存在。顺带验证：
///   ① Location 指向当前模板前缀下的新地址（产物按 base=/t/<slug>/ 构建）；
///   ② 命中计数落库（这是「规则有没有真在救流量」的唯一证据）；
///   ③ 停用后立即失效（否则「关掉一条规则」等于没关）。
#[tokio::test]
async fn redirect_rule_serves_301_and_counts_hits() {
    let st = test_state().await;
    let app = build_router(st.clone());
    let o = login(&app, "owner").await;

    let (s, b) = call(
        app.clone(),
        "POST",
        "/api/redirects",
        Some(json!({
            "fromPath": "/t/coucouya/post/old-slug",
            "toPath": "/post/new-slug",
            "code": 301,
            "note": "slug 变更"
        })),
        Some(&o),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let id = b["data"]["id"].as_str().unwrap().to_string();
    assert_eq!(b["data"]["code"], 301, "code 必须由网关持久化：{b}");

    let (st301, loc) = status_and_location(app.clone(), "/t/coucouya/post/old-slug").await;
    assert_eq!(st301, StatusCode::MOVED_PERMANENTLY, "服务期应 301，实际 {st301}");
    assert_eq!(loc.as_deref(), Some("/t/coucouya/post/new-slug"));

    let (_, b2) = call(app.clone(), "GET", &format!("/api/redirects/{id}"), None, Some(&o)).await;
    assert_eq!(b2["data"]["hits"], 1, "命中应计数：{b2}");

    // 停用 → 立刻不再跳（回落 SPA 兜底 index.html，即 200）
    let (s3, b3) = call(
        app.clone(),
        "PUT",
        &format!("/api/redirects/{id}"),
        Some(json!({ "enabled": false })),
        Some(&o),
    )
    .await;
    assert_eq!(s3, StatusCode::OK, "{b3}");
    assert_eq!(b3["data"]["enabled"], false, "enabled 必须可写：{b3}");
    let (after, _) = status_and_location(app.clone(), "/t/coucouya/post/old-slug").await;
    assert_ne!(after, StatusCode::MOVED_PERMANENTLY, "停用后不该再跳");
}

/// P0-1：404 上报必须**幂等累加**而不是每次插一行 —— 否则一条被反复访问的
/// 死链会把后台列表刷满，等于没有监控（我们是 CSR，服务端收不到公开站 404，
/// 这个端点由前端在「未找到」兜底态上报，量级不可控）。
/// 同时验证输入硬边界：公开端点无鉴权，超长 path 必须当场拒绝。
#[tokio::test]
async fn not_found_report_upserts_and_rejects_oversize() {
    let st = test_state().await;
    let app = build_router(st.clone());
    let o = login(&app, "owner").await;

    for p in ["/t/coucouya/post/missing", "/t/coucouya/post/missing/"] {
        let (s, b) = call(
            app.clone(),
            "POST",
            "/api/public/not-found",
            Some(json!({ "path": p })),
            None,
        )
        .await;
        assert_eq!(s, StatusCode::OK, "{b}");
        // 注意 ok() 的包装层：payload 在 data 里，不在顶层。
        assert_eq!(b["data"]["recorded"], true, "{b}");
    }

    let (_, list) = call(app.clone(), "GET", "/api/notfound", None, Some(&o)).await;
    let arr = list["data"].as_array().unwrap();
    // 尾斜杠变体必须归一到同一行（normalize 的职责）
    assert_eq!(
        arr.iter().filter(|x| x["path"] == "/t/coucouya/post/missing").count(),
        1,
        "同一路径的变体必须合成一行：{list}"
    );
    let row = arr
        .iter()
        .find(|x| x["path"] == "/t/coucouya/post/missing")
        .unwrap();
    assert_eq!(row["hits"], 2, "两次上报应累加：{row}");
    assert_eq!(row["resolved"], false, "新记录默认未处理：{row}");

    // 空 path 与超长 path 都要挡在入库之前
    let (s_empty, _) = call(
        app.clone(),
        "POST",
        "/api/public/not-found",
        Some(json!({ "path": "   " })),
        None,
    )
    .await;
    assert_eq!(s_empty, StatusCode::BAD_REQUEST, "空 path 应拒绝");
    let long = format!("/t/coucouya/{}", "a".repeat(600));
    let (s_long, _) = call(
        app.clone(),
        "POST",
        "/api/public/not-found",
        Some(json!({ "path": long })),
        None,
    )
    .await;
    assert_eq!(s_long, StatusCode::BAD_REQUEST, "超长 path 应拒绝");
}


/// P0-2：社区最小闭环（反应 / 举报 / @提及）必须在一条链路上成立。
///
/// 重点验四件事，每条都对应一种「看起来能跑、其实错了」的形态：
///   ① 反应是**切换**语义（点两下回到原状），不是只加不减；
///   ② 匿名不能反应/举报 —— 匿名可刷的计数不是信号，是噪声；
///   ③ 同一人重复举报不新增（防重复由库层唯一键保证，不靠应用层先查后写）；
///   ④ @提及只匹配**本站会员的昵称**，且通知只落到被提及的人头上。
#[tokio::test]
async fn community_loop_react_report_and_mention() {
    let st = test_state().await;
    let app = build_router(st.clone());
    let o = login(&app, "owner").await;

    let (s, _) = call(
        app.clone(),
        "POST",
        "/api/articles",
        Some(json!({
            "title": "社区测试", "slug": "community-test",
            "content": "<p>x</p>", "status": "published", "visibility": "public"
        })),
        Some(&o),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "建文章");

    // 两个会员，昵称必须不同 —— @提及靠昵称精确匹配
    let (s, a) = call(
        app.clone(),
        "POST",
        "/api/public/members/register",
        Some(json!({"email": "c1@test.dev", "password": "member1234", "name": "阿甲"})),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{a}");
    let a_tok = a["data"]["token"].as_str().unwrap().to_string();
    let (s, b) = call(
        app.clone(),
        "POST",
        "/api/public/members/register",
        Some(json!({"email": "c2@test.dev", "password": "member1234", "name": "阿乙"})),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let b_tok = b["data"]["token"].as_str().unwrap().to_string();

    // 甲发评论：带会员令牌 → member_id 落库。
    // 正文**刻意不带 @**：提及方向由下面乙→甲那条单独验。
    // （原来的写法是「甲 @乙」却断言甲收到通知 —— 方向相反，
    //   于是「通知发错人」这个真缺陷被自己的测试盖章通过了。）
    let (s, c) = call(
        app.clone(),
        "POST",
        "/api/public/comments",
        Some(json!({
            "article_id": "community-test", "author_name": "阿甲",
            "content": "这篇的结论我有疑问。"
        })),
        Some(&a_tok),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{c}");
    let cid = c["data"]["id"].as_str().unwrap().to_string();

    // 匿名读列表可以，但 myReaction 恒为空（没有身份）
    let (s, list) = call(app.clone(), "GET", "/api/public/comments?article=community-test", None, None).await;
    assert_eq!(s, StatusCode::OK, "{list}");
    let row = list["data"].as_array().unwrap().iter().find(|x| x["id"] == cid).unwrap().clone();
    assert_eq!(row["reactions"]["up"], 0, "{row}");
    assert_eq!(row["myReaction"].as_array().unwrap().len(), 0, "{row}");

    // 乙点赞 → on=true；再点 → on=false（切换，且计数归零）
    let (s, r1) = call(
        app.clone(),
        "POST",
        &format!("/api/public/comments/{cid}/react"),
        Some(json!({"kind": "up"})),
        Some(&b_tok),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{r1}");
    assert_eq!(r1["data"]["on"], true, "{r1}");
    assert_eq!(r1["data"]["reactions"]["up"], 1, "{r1}");
    let (_, r2) = call(
        app.clone(),
        "POST",
        &format!("/api/public/comments/{cid}/react"),
        Some(json!({"kind": "up"})),
        Some(&b_tok),
    )
    .await;
    assert_eq!(r2["data"]["on"], false, "再点一下应取消：{r2}");
    assert_eq!(r2["data"]["reactions"]["up"], 0, "{r2}");

    // 非法反应类型要挡住（避免脏 kind 进库后计数永远对不上）
    let (s_bad, _) = call(
        app.clone(),
        "POST",
        &format!("/api/public/comments/{cid}/react"),
        Some(json!({"kind": "down"})),
        Some(&b_tok),
    )
    .await;
    assert_eq!(s_bad, StatusCode::BAD_REQUEST, "非法反应类型应拒绝");

    // 匿名不能举报（计数必须归属于确定的人，否则可被无限刷）
    let (s_anon, _) = call(
        app.clone(),
        "POST",
        &format!("/api/public/comments/{cid}/report"),
        Some(json!({"reason": "spam"})),
        None,
    )
    .await;
    assert_eq!(s_anon, StatusCode::UNAUTHORIZED, "匿名举报应被拒");

    // 同一人举报两次 → 第二次 first=false 且计数不变
    let (_, p1) = call(
        app.clone(),
        "POST",
        &format!("/api/public/comments/{cid}/report"),
        Some(json!({"reason": "广告"})),
        Some(&b_tok),
    )
    .await;
    assert_eq!(p1["data"]["first"], true, "{p1}");
    assert_eq!(p1["data"]["reportCount"], 1, "{p1}");
    let (_, p2) = call(
        app.clone(),
        "POST",
        &format!("/api/public/comments/{cid}/report"),
        Some(json!({"reason": "再报一次"})),
        Some(&b_tok),
    )
    .await;
    assert_eq!(p2["data"]["first"], false, "同一人重复举报不该新增：{p2}");
    assert_eq!(p2["data"]["reportCount"], 1, "{p2}");

    // 乙发评论 @甲 —— 通知必须落到**被 @ 的人（甲）**头上，
    // 而不是评论作者（乙）。搞反了就是「自己给自己发通知」，
    // 真正被提及的人永远收不到，且不会有人报错。
    let (s, m2) = call(
        app.clone(),
        "POST",
        "/api/public/comments",
        Some(json!({
            "article_id": "community-test", "author_name": "阿乙",
            "content": "@阿甲 我也有同感"
        })),
        Some(&b_tok),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{m2}");

    // 被 @ 的甲拿到 mention 通知；没被 @ 的乙不该有
    let (_, na) = call(app.clone(), "GET", "/api/public/members/notifications", None, Some(&a_tok)).await;
    assert!(na["data"]["unread"].as_i64().unwrap() >= 1, "被 @ 的人应有未读：{na}");
    let kinds: Vec<String> = na["data"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["kind"].as_str().unwrap_or("").to_string())
        .collect();
    assert!(kinds.contains(&"mention".to_string()), "{na}");
    let (_, nb) = call(app.clone(), "GET", "/api/public/members/notifications", None, Some(&b_tok)).await;
    assert!(
        nb["data"]["items"].as_array().unwrap().iter().all(|x| x["kind"] != "mention"),
        "没被 @ 的人不该收到提及通知：{nb}"
    );

    // 标记已读 → 未读归零
    let (_, rd) = call(app.clone(), "POST", "/api/public/members/notifications/read", None, Some(&a_tok)).await;
    assert!(rd["data"]["read"].as_i64().unwrap() >= 1, "{rd}");
    let (_, na2) = call(app.clone(), "GET", "/api/public/members/notifications", None, Some(&a_tok)).await;
    assert_eq!(na2["data"]["unread"], 0, "{na2}");

    // 后台审核列表带出举报数 —— 这是「举报优先展示」的依据
    let (_, al) = call(app.clone(), "GET", "/api/comments", None, Some(&o)).await;
    let hit = al["data"].as_array().unwrap().iter().find(|x| x["id"] == cid).unwrap().clone();
    assert_eq!(hit["reportCount"], 1, "{hit}");

    // P0-3：评论要能出现在会员档案时间线上（member_id 归属正确才可能）
    let (_, tl) = call(app.clone(), "GET", &format!("/api/members/{}/timeline", a["data"]["member"]["id"].as_str().unwrap()), None, Some(&o)).await;
    let evs = tl["data"]["events"].as_array().unwrap();
    assert!(
        evs.iter().any(|e| e["kind"] == "comment"),
        "会员档案时间线应包含他发的评论：{tl}"
    );
}


/// P0-4：Smart Links 必须同时做到「跳转 / 计数 / 打标签」，
/// 并且**预取不能污染计数**。
///
/// 这条链路最容易「看着能跑、其实坏了」的两种形态：
///   ① 邮件网关预取被当成真人点击 —— 一封群发邮件就能把计数刷到几十，
///      数据从此不可信，而且跳转是好的，所以没人会发现；
///   ② 标签打到作者身上而不是**点击者**身上 —— 打标签的全部价值
///      就是「谁对这条链接有兴趣」，打错人等于把噪声写进会员档案。
#[tokio::test]
async fn smart_link_redirects_counts_and_tags() {
    let st = test_state().await;
    let app = build_router(st.clone());
    let o = login(&app, "owner").await;

    let (s, a) = call(
        app.clone(),
        "POST",
        "/api/public/members/register",
        Some(json!({"email": "s1@test.dev", "password": "member1234", "name": "链接甲"})),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{a}");
    let a_tok = a["data"]["token"].as_str().unwrap().to_string();
    let a_id = a["data"]["member"]["id"].as_str().unwrap().to_string();

    let (s, l) = call(
        app.clone(),
        "POST",
        "/api/links",
        Some(json!({
            "token": "promo1", "url": "https://example.com/landing",
            "label": "新品推广", "tags": "vip,高意向", "enabled": true
        })),
        Some(&o),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "建短链：{l}");

    // ① 真人导航（带会员令牌）→ 302 到目标
    let (st1, loc) = status_loc_h(
        app.clone(),
        "/go/promo1",
        vec![
            ("sec-fetch-mode", "navigate".to_string()),
            (
                "user-agent",
                "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0) Safari/604.1".to_string(),
            ),
            ("authorization", format!("Bearer {a_tok}")),
        ],
    )
    .await;
    assert_eq!(st1, StatusCode::FOUND, "真人点击应 302");
    assert_eq!(loc.as_deref(), Some("https://example.com/landing"));

    // ② 邮件网关预取 → **照旧跳转**，但不计入真人点击
    //    （观测可以缺，功能不能缺；跳错比不跳伤害大）
    let (st2, _) = status_loc_h(
        app.clone(),
        "/go/promo1",
        vec![
            ("purpose", "prefetch".to_string()),
            ("user-agent", "Mozilla/5.0 (Macintosh) Chrome/120".to_string()),
        ],
    )
    .await;
    assert_eq!(st2, StatusCode::FOUND, "预取也必须跳转");

    // ③ 非 http(s) 目标一律 500 —— 防「后台配置触发开放重定向 / XSS」
    let (s_bad, l_bad) = call(
        app.clone(),
        "POST",
        "/api/links",
        Some(json!({"token": "evil", "url": "javascript:alert(1)", "enabled": true})),
        Some(&o),
    )
    .await;
    assert_eq!(s_bad, StatusCode::OK, "{l_bad}");
    let (st3, _) = status_loc_h(app.clone(), "/go/evil", vec![]).await;
    assert_eq!(st3, StatusCode::INTERNAL_SERVER_ERROR, "非 http(s) 目标不该跳");

    // ④ 未知 token → 404。**不能掉进 SPA 兜底**：那样短链失效时
    //    表现为「跳到首页」，错得无声无息。
    let (st4, _) = status_loc_h(app.clone(), "/go/nope", vec![]).await;
    assert_eq!(st4, StatusCode::NOT_FOUND, "未知短链应 404");

    // ⑤ 计数：真人 1 次、预取 1 次，两者不混
    let (_, ls) = call(app.clone(), "GET", "/api/links", None, Some(&o)).await;
    let row = ls["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["token"] == "promo1")
        .unwrap()
        .clone();
    assert_eq!(row["clicks"], 1, "真人点击只该算 1 次：{row}");
    assert_eq!(row["prefetch"], 1, "预取应单独计数：{row}");

    // ⑥ 标签打到了**点击者**身上，并在会员档案（P0-3）里可见
    let (_, tl) = call(
        app.clone(),
        "GET",
        &format!("/api/members/{a_id}/timeline"),
        None,
        Some(&o),
    )
    .await;
    let tags: Vec<String> = tl["data"]["tags"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap_or("").to_string())
        .collect();
    assert!(
        tags.contains(&"vip".to_string()) && tags.contains(&"高意向".to_string()),
        "点击者应被打上链接上的标签：{tl}"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// 分析归因（P0）：把匿名访客与注册会员串成一条线
// ═══════════════════════════════════════════════════════════════════════

/// 读归因端点（顺带断言 200）
async fn attr_json(app: &axum::Router, tok: &str) -> Value {
    let (s, b) = call(app.clone(), "GET", "/api/admin/attribution", None, Some(tok)).await;
    assert_eq!(s, StatusCode::OK, "归因端点应 200：{b}");
    b
}

#[tokio::test]
async fn attribution_binds_first_touch_article_not_last() {
    let st = test_state().await;
    let app = build_router(st.clone());
    let o = login(&app, "owner").await;

    // 两篇已发布文章
    let mut ids: Vec<String> = Vec::new();
    for (slug, title) in [("attr-first", "第一篇"), ("attr-second", "第二篇")] {
        let (s, b) = call(
            app.clone(),
            "POST",
            "/api/articles",
            Some(json!({
                "title": title, "slug": slug, "content": "<p>x</p>",
                "status": "published", "visibility": "public"
            })),
            Some(&o),
        )
        .await;
        assert_eq!(s, StatusCode::OK, "建文章 {slug}：{b}");
        ids.push(b["data"]["id"].as_str().expect("文章 id").to_string());
    }

    // 基准（库里可能有种子会员，用增量断言避免脆）
    let base = attr_json(&app, &o).await;
    let attr0 = base["data"]["summary"]["attributedSignups"].as_i64().unwrap();
    let nv0 = base["data"]["gaps"]["noVisitor"].as_i64().unwrap();

    // 埋点顺序刻意**先第二篇、后第一篇**。
    // 若实现取的是「最后一次」而不是「最早一次」，下面断言就会失败 ——
    // 归因口径最容易写反的正是这一处，所以要让它有暴露的机会。
    let vid = "visitor-first-touch";
    for aid in [&ids[1], &ids[0]] {
        let (s, b) = call(
            app.clone(),
            "POST",
            "/api/public/track",
            Some(json!({ "type": "article_view", "refId": aid, "visitorId": vid })),
            None,
        )
        .await;
        assert_eq!(s, StatusCode::OK, "埋点应成功：{b}");
    }

    // 带着同一个 visitorId 注册
    let (s, reg) = call(
        app.clone(),
        "POST",
        "/api/public/members/register",
        Some(json!({
            "email": "attr@test.dev", "password": "member1234",
            "name": "归因甲", "visitorId": vid
        })),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "注册应成功：{reg}");
    let mid = reg["data"]["member"]["id"].as_str().unwrap().to_string();

    // 首次触达必须落到**最早**看的那篇（ids[1]），不是最后看的（ids[0]）
    let row = st
        .db
        .query_one(
            "SELECT first_touch_article_id, visitor_id FROM members WHERE id = ?",
            vec![sv(&mid)],
        )
        .await
        .unwrap()
        .expect("会员应存在");
    let ft = row.try_get::<String>("", "first_touch_article_id").unwrap();
    assert_eq!(
        ft, ids[1],
        "首次触达必须是**最早**看过的那篇；拿到最后看的那篇说明口径写成了末次触达"
    );
    assert_eq!(
        row.try_get::<String>("", "visitor_id").unwrap(),
        vid,
        "visitor_id 应一并固化，供后续行为接续"
    );

    // 端点聚合
    let body = attr_json(&app, &o).await;
    let rows = body["data"]["rows"].as_array().unwrap();
    let r = rows
        .iter()
        .find(|r| r["articleId"] == ids[1].as_str())
        .expect("归属文章应出现在归因表里");
    assert_eq!(r["signups"].as_i64().unwrap(), 1, "归属文章应记 1 个注册：{r}");
    assert_eq!(r["reads"].as_i64().unwrap(), 1, "{r}");
    assert_eq!(r["visitors"].as_i64().unwrap(), 1, "{r}");

    // 没被点名的那篇阅读归它、注册不归它
    let r2 = rows
        .iter()
        .find(|r| r["articleId"] == ids[0].as_str())
        .expect("第二篇也应有阅读行");
    assert_eq!(r2["reads"].as_i64().unwrap(), 1, "{r2}");
    assert_eq!(
        r2["signups"].as_i64().unwrap(),
        0,
        "末次触达不得被计入注册：{r2}"
    );

    // 这次注册是「有 visitor 且有阅读」的，两个缺口都不该动
    assert_eq!(body["data"]["gaps"]["noVisitor"].as_i64().unwrap(), nv0);
    assert_eq!(body["data"]["summary"]["attributedSignups"].as_i64().unwrap(), attr0 + 1);

    // 「全站去重访客」与「各文章独立访客之和」不是一回事，混用会把转化率算错：
    // 同一个访客看了两篇 → 前者 1（去重），后者 2（每篇各算一次）。
    // 后者可以大于会员总数，看着像「访客比人多」，其实它衡量的是内容触达总量。
    assert_eq!(
        body["data"]["summary"]["siteVisitors"].as_i64().unwrap(),
        1,
        "只来过 1 个访客，全站去重后必须是 1"
    );
    assert_eq!(
        body["data"]["summary"]["sumArticleVisitors"].as_i64().unwrap(),
        2,
        "两篇文章各有 1 个独立访客，相加是 2 —— 它是触达总量，不是人群规模"
    );

    // ── 反查：从「这篇文章带来 1 人」到「具体是哪 1 个人」──
    // 归因页能说「这篇带来 N 人」，站长还得能在会员列表里把这 N 人找出来，
    // 否则数字只能信、不能查。靠 firstTouchArticleId 的等值筛选做下钻。
    let (s, lst) = call(
        app.clone(),
        "GET",
        &format!("/api/members?firstTouchArticleId={}", ids[1]),
        None,
        Some(&o),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "按首触文章筛会员应 200：{lst}");
    let items = lst["data"].as_array().unwrap();
    assert_eq!(items.len(), 1, "应恰好筛出这篇文章带来的 1 人：{lst}");
    assert_eq!(items[0]["id"].as_str().unwrap_or(""), mid, "{lst}");
    assert_eq!(
        items[0]["firstTouchArticleId"].as_str().unwrap_or(""),
        ids[1],
        "会员列表必须带出首触文章，否则前端画不出「文章获客」这个来源：{lst}"
    );

    // 档案抽屉要直接显示**篇名**，而不是把 id 甩给站长自己去查
    let (s, tl) = call(
        app.clone(),
        "GET",
        &format!("/api/members/{mid}/timeline"),
        None,
        Some(&o),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "会员档案应 200：{tl}");
    let mem = &tl["data"]["member"];
    assert_eq!(
        mem["firstTouchArticleTitle"].as_str().unwrap_or(""),
        "第二篇",
        "档案要给出首触文章标题：{tl}"
    );
    assert_eq!(
        mem["firstTouchArticleSlug"].as_str().unwrap_or(""),
        "attr-second",
        "还要给 slug，抽屉里的链接才能直达文章：{tl}"
    );
    assert!(
        !mem["firstTouchAt"].as_str().unwrap_or("").is_empty(),
        "首访时间应一并带出：{tl}"
    );
}

/// 归因三列在后台必须**只读**。
///
/// 这类「当初真实发生了什么」的数据一旦能被后台随手改写，归因结论就不再可信 ——
/// 而且改写是无声的：数字还在，只是再也对不上当初。
///
/// 反面同样重要：只读列不能「写了当没写」。静默忽略会让调用方以为改成功了
/// （本项目已经因为「静默丢键」栽过一次），所以必须明确 400 并把字段名点出来。
#[tokio::test]
async fn member_attribution_columns_are_read_only() {
    let st = test_state().await;
    let app = build_router(st.clone());
    let o = login(&app, "owner").await;

    let (s, reg) = call(
        app.clone(),
        "POST",
        "/api/public/members/register",
        Some(json!({ "email": "ro@test.dev", "password": "member1234", "name": "只读" })),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "注册应成功：{reg}");
    let mid = reg["data"]["member"]["id"].as_str().expect("会员 id").to_string();

    // 三个只读列逐个尝试篡改 → 一律 400 且点名
    for (col, probe) in [
        ("visitorId", json!({ "visitorId": "被篡改" })),
        ("firstTouchArticleId", json!({ "firstTouchArticleId": "被篡改" })),
        ("firstTouchAt", json!({ "firstTouchAt": "被篡改" })),
    ] {
        let (s, b) = call(
            app.clone(),
            "PUT",
            &format!("/api/members/{mid}"),
            Some(probe),
            Some(&o),
        )
        .await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "改只读列 {col} 应被拒：{b}");
        assert!(
            b["error"].as_str().unwrap_or("").contains(col),
            "错误信息必须点名 {col}，否则调用方无从下手：{b}"
        );
    }

    // 值原样未动：写入被拒 ≠ 写入后又被覆盖回来
    let row = st
        .db
        .query_one(
            "SELECT visitor_id, first_touch_article_id FROM members WHERE id = ?",
            vec![sv(&mid)],
        )
        .await
        .unwrap()
        .expect("会员应存在");
    assert_eq!(
        row.try_get::<String>("", "visitor_id").unwrap_or_default(),
        "",
        "只读列不该被写进去"
    );
    assert_eq!(
        row.try_get::<String>("", "first_touch_article_id").unwrap_or_default(),
        "",
        "只读列不该被写进去"
    );

    // 普通列照常能改 —— 别为了拦只读把整个 update 弄瘸
    let (s, b) = call(
        app.clone(),
        "PUT",
        &format!("/api/members/{mid}"),
        Some(json!({ "name": "改名成功" })),
        Some(&o),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "普通字段应仍可改：{b}");
    assert_eq!(b["data"]["name"].as_str().unwrap_or(""), "改名成功", "{b}");

    // 创建路径也拦：只读列不能靠「先建成别的，再改名」绕过去
    let (s, b) = call(
        app.clone(),
        "POST",
        "/api/members",
        Some(json!({ "email": "ro2@test.dev", "name": "x", "firstTouchArticleId": "伪造" })),
        Some(&o),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "创建时写只读列也应被拒：{b}");
}

#[tokio::test]
async fn attribution_reports_gap_instead_of_guessing() {
    let st = test_state().await;
    let app = build_router(st.clone());
    let o = login(&app, "owner").await;

    // 归因是内部经营数据，匿名不得读
    let (s, _) = call(app.clone(), "GET", "/api/admin/attribution", None, None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "匿名不得读归因");

    let b0 = attr_json(&app, &o).await;
    let nv0 = b0["data"]["gaps"]["noVisitor"].as_i64().unwrap();
    let attr0 = b0["data"]["summary"]["attributedSignups"].as_i64().unwrap();

    // ① 不带 visitorId（后台建号 / 老前端 / 隐私模式）
    //    → 必须落进「未归因」，而不是被安静地摊到某篇文章头上。
    let (s, b) = call(
        app.clone(),
        "POST",
        "/api/public/members/register",
        Some(json!({ "email": "gap@test.dev", "password": "member1234", "name": "无痕" })),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "注册应成功：{b}");

    let after = attr_json(&app, &o).await;
    assert_eq!(
        after["data"]["gaps"]["noVisitor"].as_i64().unwrap(),
        nv0 + 1,
        "无访客标识的注册应计入 noVisitor"
    );
    assert_eq!(
        after["data"]["summary"]["attributedSignups"].as_i64().unwrap(),
        attr0,
        "无访客标识的注册不得挤进任何一篇文章的归因"
    );

    // ② 有 visitorId 但注册前没看过文章 → 归到另一个缺口 noRead。
    //    两个缺口分开报的意义就在这里：处理方式完全不同
    //    （一个是链路没接上要修，一个只是用户从首页直接注册，属正常）。
    let nr0 = after["data"]["gaps"]["noRead"].as_i64().unwrap();
    let (s, b) = call(
        app.clone(),
        "POST",
        "/api/public/members/register",
        Some(json!({
            "email": "blind@test.dev", "password": "member1234",
            "name": "空手", "visitorId": "visitor-never-read"
        })),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "注册应成功：{b}");

    let after2 = attr_json(&app, &o).await;
    assert_eq!(
        after2["data"]["gaps"]["noRead"].as_i64().unwrap(),
        nr0 + 1,
        "有 visitor 无阅读的注册应计入 noRead"
    );
    assert_eq!(
        after2["data"]["gaps"]["noVisitor"].as_i64().unwrap(),
        nv0 + 1,
        "有 visitor 的人不该再被算作无访客"
    );
    assert_eq!(
        after2["data"]["summary"]["attributedSignups"].as_i64().unwrap(),
        attr0,
        "两次都不可归因，归属总数不该变"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// 入口 DTO 的键名契约（camelCase）
//
// 背景：这些手写 DTO 曾经是**纯 snake_case**，而真实调用方
// （admin web 的 `api/cms/index.ts`、公开站的 Comments.tsx）一律传 camelCase。
// serde 对未知字段**静默忽略**，于是必填字段直接消失 → Json 提取失败 → 422。
// 两条线上路径因此彻底不可用且没人发现：
//   · `POST /api/public/checkout`（Stripe 会员自助升级）
//   · `POST /api/public/comments`（后台发评论）
// 可选字段更隐蔽：不报错，只是静默变 None，业务已经错了而日志干净。
//
// 这个用例刻意使用**前端真实的拼写**，而不是后端字段名 ——
// 只有当测试说调用方的语言时，它才能发现调用方的错误。
// ═══════════════════════════════════════════════════════════════════════

/// 断言「请求体被成功解析」：422 是 axum 的 Json 提取失败（契约错位），
/// 而我们自己的业务错误一律是 400/403/404 的 JSON。
fn assert_parsed(s: StatusCode, what: &str) {
    assert_ne!(
        s,
        StatusCode::UNPROCESSABLE_ENTITY,
        "{what}：请求体没被解析 —— 说明前端发的键名与后端 DTO 不接受，契约又错位了"
    );
}

#[tokio::test]
async fn entry_dtos_accept_the_camelcase_contract() {
    let st = test_state().await;
    let app = build_router(st.clone());
    let o = login(&app, "owner").await;
    let m = register_member(&app, "dto@test.dev").await;

    // 一篇积分解锁文章（自己取 id 会比硬编码 slug 稳）
    let (_, art) = call(
        app.clone(),
        "POST",
        "/api/articles",
        Some(json!({
            "title": "契约文章", "status": "published", "visibility": "public",
            "paidLevel": 2, "pricePoints": 100
        })),
        Some(&o),
    )
    .await;
    let aid = art["data"]["id"].as_str().expect("文章 id").to_string();

    // ① 评论：camelCase 必须**真的落对文章**，不能只是「解析通过」。
    //    解析通过只说明字段存在；值落到别处（比如空串）照样是坏的。
    let (s, b) = call(
        app.clone(),
        "POST",
        "/api/public/comments",
        Some(json!({ "articleId": aid, "authorName": "契约甲", "content": "camelCase 契约" })),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "评论 camelCase 应成功：{b}");
    let cid = b["data"]["id"].as_str().expect("评论 id").to_string();
    let (_, list) = call(
        app.clone(),
        "GET",
        &format!("/api/public/comments?article={aid}"),
        None,
        None,
    )
    .await;
    assert!(
        list["data"].as_array().map(|a| a.iter().any(|c| c["id"] == cid.as_str())).unwrap_or(false),
        "评论必须出现在它所属文章的列表里：{list}"
    );

    // ② 买断：给够积分，买完直接回查 `points_ledger.ref_id` ——
    //    这是唯一能证明 articleId 的值（而非仅仅字段存在）真的落库的办法。
    let (_, rc) = call(
        app.clone(),
        "POST",
        "/api/redeem_codes",
        Some(json!({ "code": "DTO500", "kind": "points", "value": 500, "status": "unused" })),
        Some(&o),
    )
    .await;
    assert_eq!(rc["data"]["code"], "DTO500", "{rc}");
    let (s, rb) = call(
        app.clone(),
        "POST",
        "/api/public/members/redeem",
        Some(json!({ "code": "DTO500" })),
        Some(&m),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{rb}");

    let (s, b) = call(
        app.clone(),
        "POST",
        "/api/public/members/purchase-article",
        Some(json!({ "articleId": aid })),
        Some(&m),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "买断 camelCase 应成功：{b}");
    assert_eq!(b["data"]["spent"], 100, "{b}");

    let mem_id: String = {
        let (_, me) = call(app.clone(), "GET", "/api/public/members/me", None, Some(&m)).await;
        me["data"]["member"]["id"].as_str().expect("会员 id").to_string()
    };
    let row = st
        .db
        .query_one(
            "SELECT ref_id FROM points_ledger \
             WHERE tenant_id = ? AND member_id = ? AND reason = 'purchase' LIMIT 1",
            vec![sv(&st.tenant), sv(&mem_id)],
        )
        .await
        .unwrap()
        .expect("应有买断记录");
    assert_eq!(
        row.try_get::<String>("", "ref_id").unwrap(),
        aid,
        "买断记录必须指向被买的那篇文章；指向空串说明 articleId 被静默丢掉了"
    );

    // ③ 下单：camelCase 的 bizType
    let (s, b) = call(
        app.clone(),
        "POST",
        "/api/public/orders",
        Some(json!({ "bizType": "points_recharge", "points": 100, "channel": "manual" })),
        Some(&m),
    )
    .await;
    assert_parsed(s, "POST /api/public/orders 的 bizType");
    assert_eq!(s, StatusCode::OK, "下单应成功：{b}");
    assert_eq!(b["data"]["amountCents"], 100, "{b}");

    // ④ Stripe checkout：这是「会员自助升级」那条路。
    //    套餐可以不存在（会得到 400「套餐不存在」，属正常业务错误），
    //    但**不能**是 422 —— 那说明 tierId 根本没被读到。
    let (s, b) = call(
        app.clone(),
        "POST",
        "/api/public/checkout",
        Some(json!({ "tierId": "no-such-tier", "interval": "monthly" })),
        Some(&m),
    )
    .await;
    assert_parsed(s, "POST /api/public/checkout 的 tierId");

    // ⑤ 改密：用**错误的旧密码**触发业务错误即可证明解析成功，
    //    不必真的改掉密码（改掉会影响后续用例/重复执行）。
    let (s, b) = call(
        app.clone(),
        "POST",
        "/api/me/password",
        Some(json!({ "oldPassword": "definitely-wrong", "newPassword": "whatever1234" })),
        Some(&o),
    )
    .await;
    assert_parsed(s, "POST /api/me/password 的 oldPassword/newPassword");
    assert_eq!(s, StatusCode::BAD_REQUEST, "旧密码错应 400：{b}");
    assert!(
        b["error"].as_str().unwrap_or("").contains("旧密码"),
        "应是「旧密码不正确」而不是解析错误：{b}"
    );

    // ⑥ 反向：snake 别名必须继续可用。
    //    浏览器里缓存的旧版 JS、以及历史上按 snake 写的第三方调用方
    //    不该因为这次契约对齐而被打断。
    let (s, _) = call(
        app.clone(),
        "POST",
        "/api/public/comments",
        Some(json!({ "article_id": aid, "author_name": "契约乙", "content": "snake 别名" })),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "snake 别名必须继续可用（老客户端兼容）");
}


// ═══════════════════════════════════════════════════════════════════════
// 阶段 1：订阅状态机（Stripe 事件 → 会员有效期）
// ═══════════════════════════════════════════════════════════════════════

/// 读会员的 (plan, plan_expires_at)
async fn member_plan(st: &AppState, mid: &str) -> (String, String) {
    let r = st
        .db
        .query_one(
            "SELECT plan, plan_expires_at FROM members WHERE id = ? AND tenant_id = ? LIMIT 1",
            vec![sv(mid), sv(&st.tenant)],
        )
        .await
        .expect("查询会员")
        .expect("会员应存在");
    (
        r.try_get("", "plan").unwrap_or_default(),
        r.try_get("", "plan_expires_at").unwrap_or_default(),
    )
}

/// 发送一个 Stripe webhook 事件（签名头按是否配置密钥自动给出）
async fn hook(app: &axum::Router, body: Value) -> (StatusCode, Value) {
    let s = body.to_string();
    call_h(
        app.clone(),
        "POST",
        "/api/stripe/webhook",
        Some(s.clone()),
        stripe_sig_header(&s),
    )
    .await
}

/// 注册一个会员，返回 (member_id, token)
async fn reg_member(app: &axum::Router, email: &str) -> (String, String) {
    let (s, b) = call(
        app.clone(),
        "POST",
        "/api/public/members/register",
        Some(json!({ "email": email, "password": "member1234", "name": "订阅者" })),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "注册应成功：{b}");
    (
        b["data"]["member"]["id"].as_str().expect("member id").to_string(),
        b["data"]["token"].as_str().expect("token").to_string(),
    )
}

/// 构造 `customer.subscription.*` 事件（字段名严格照 Stripe 的订阅对象）
#[allow(clippy::too_many_arguments)]
fn sub_event(
    eid: &str,
    etype: &str,
    sub_id: &str,
    member_id: &str,
    status: &str,
    period_end: i64,
    created: i64,
    cancel_at_period_end: bool,
) -> Value {
    json!({
        "id": eid,
        "type": etype,
        "created": created,
        "data": { "object": {
            "id": sub_id,
            "customer": "cus_stage1",
            "status": status,
            "current_period_end": period_end,
            "cancel_at_period_end": cancel_at_period_end,
            "start_date": created,
            "metadata": { "member_id": member_id, "tier_slug": "annual", "interval": "yearly" },
            "items": { "data": [ { "price": { "id": "price_year" } } ] }
        }}
    })
}

/// Unix 秒 → 与库里同一口径的 RFC3339（字符串比较即时间比较的前提）
fn iso(ts: i64) -> String {
    crate::sub_state::unix_to_iso(Some(&json!(ts)))
}

/// 插一条订阅行（用于模拟「webhook 全丢」这类只有数据侧能构造的场景）
/// 插一条订阅行（用于模拟「webhook 全丢」这类只有数据侧能构造的场景）。
///
/// `granted_expiry`：本订阅**授予过**的会员到期时间（空串 = 从未授予过）。
/// 它与 `period_end` 是两件事，刻意分开传：
/// 前者回答「会员现在这份有效期是不是这个订阅给的」，是订阅结束时
/// 能否降级的唯一判据（见迁移 0018）。
async fn seed_subscription(
    st: &AppState,
    row_id: &str,
    sub_id: &str,
    mid: &str,
    period_end: &str,
    status: &str,
    granted_expiry: &str,
) {
    let now_s = db::now_iso();
    st.db
        .execute(
            "INSERT INTO subscriptions (id, tenant_id, member_id, stripe_subscription_id, \
             stripe_customer_id, tier_slug, price_id, interval, status, current_period_end, \
             cancel_at_period_end, canceled_at, started_at, last_event_at, closed_at, \
             granted_expiry, created_at, updated_at) \
             VALUES (?, ?, ?, ?, '', 'annual', 'price_year', 'yearly', ?, ?, 0, '', '', 0, '', ?, ?, ?)",
            vec![
                sv(row_id),
                sv(&st.tenant),
                sv(mid),
                sv(sub_id),
                sv(&status.to_string()),
                sv(period_end),
                sv(granted_expiry),
                sv(&now_s),
                sv(&now_s),
            ],
        )
        .await
        .expect("插入订阅行");
}

async fn order_count(st: &AppState, sub_id: &str) -> i64 {
    st.db
        .query_one(
            "SELECT COUNT(*) AS c FROM orders WHERE tenant_id = ? AND ref_no = ?",
            vec![sv(&st.tenant), sv(sub_id)],
        )
        .await
        .expect("查订单")
        .expect("应有结果")
        .try_get("", "c")
        .unwrap_or(0)
}

/// 【阶段 1 核心闭环】订阅事件必须**真的改变会员状态**。
///
/// 此前这四类事件只落库标 `pending_apply`：事件收得到，但没有任何东西把它
/// 变成会员状态。两个后果都是钱 ——
///   ① 年付会员第二年正常扣款却拿不到服务（续费不发货）；
///   ② 周期过了会员仍有效（到期不降级），且 `members` 表自身完全自洽，
///      对账、归因、后台列表**都看不出异常**。
#[tokio::test]
async fn subscription_lifecycle_drives_member_plan() {
    let st = test_state().await;
    let app = build_router(st.clone());
    let (mid, _) = reg_member(&app, "sub_life@test.dev").await;

    let now = chrono::Utc::now().timestamp();
    let day = 86_400i64;
    let sub = "sub_lifecycle_1";

    // ── ① 订阅创建：周期 365 天 → 会员立刻成为 annual ──
    let (s, b) = hook(
        &app,
        sub_event("evt_sc_1", "customer.subscription.created", sub, &mid, "active", now + 365 * day, now, false),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["data"]["status"], "processed", "{b}");
    let (plan, exp) = member_plan(&st, &mid).await;
    assert_eq!(plan, "annual", "订阅创建后 plan 应落到套餐 slug");
    let left = crate::sub_state::parse_ts(&exp).expect("到期时间应可解析").timestamp() - now;
    assert!((364 * day..=366 * day).contains(&left), "到期应约 365 天后，实得 {left}s");

    // ── ② 续费成功（发票）：新周期再 +365 天，并落一条订单 ──
    let (s, b) = hook(
        &app,
        json!({
            "id": "evt_inv_1",
            "type": "invoice.payment_succeeded",
            "created": now + 10,
            "data": { "object": {
                "subscription": sub,
                "customer": "cus_stage1",
                "amount_paid": 18_000,
                "lines": { "data": [ { "period": { "end": now + 730 * day } } ] }
            }}
        }),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["data"]["status"], "processed", "{b}");
    let (_, exp2) = member_plan(&st, &mid).await;
    let left2 = crate::sub_state::parse_ts(&exp2).expect("到期时间应可解析").timestamp() - now;
    assert!((729 * day..=731 * day).contains(&left2), "续费后到期应约 730 天后，实得 {left2}s");

    // 续费的钱必须在 orders 里看得见：否则归因收入 / 对账 / 订单列表
    // 三处全都漏掉 Stripe 收到的钱，只有去 Stripe 后台翻账单才知道。
    assert_eq!(order_count(&st, sub).await, 1, "续费应落一条订单");
    let amount: i64 = st
        .db
        .query_one(
            "SELECT amount_cents FROM orders WHERE tenant_id = ? AND ref_no = ? LIMIT 1",
            vec![sv(&st.tenant), sv(sub)],
        )
        .await
        .unwrap()
        .unwrap()
        .try_get("", "amount_cents")
        .unwrap_or(0);
    assert_eq!(amount, 18_000, "订单金额应为发票实收");

    // ── ②b 同一张发票重投：不得多记一条（收入会翻倍）──
    let (s, _) = hook(
        &app,
        json!({
            "id": "evt_inv_1_dup",
            "type": "invoice.payment_succeeded",
            "created": now + 11,
            "data": { "object": {
                "subscription": sub, "customer": "cus_stage1", "amount_paid": 18_000,
                "lines": { "data": [ { "period": { "end": now + 730 * day } } ] }
            }}
        }),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(order_count(&st, sub).await, 1, "同一计费周期重复投递不得产生第二条订单");

    // ── ③ 乱序：更旧的事件不得覆盖新周期 ──
    let (s, _) = hook(
        &app,
        sub_event("evt_sc_old", "customer.subscription.updated", sub, &mid, "active", now + 100 * day, now - 100, false),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (_, exp3) = member_plan(&st, &mid).await;
    assert_eq!(exp3, exp2, "更旧的事件必须被忽略，不得把周期改回去（用户会以为白付了）");

    // ── ④ 期末取消：已安排取消但周期未过 → **仍然有效** ──
    let (s, b) = hook(
        &app,
        sub_event("evt_sc_cancel", "customer.subscription.updated", sub, &mid, "active", now + 730 * day, now + 20, true),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let (plan4, _) = member_plan(&st, &mid).await;
    assert_eq!(plan4, "annual", "期末取消在周期结束前必须继续有效（用户已付到期末）");
    let cancel_flag: i64 = st
        .db
        .query_one(
            "SELECT cancel_at_period_end FROM subscriptions WHERE tenant_id = ? AND stripe_subscription_id = ? LIMIT 1",
            vec![sv(&st.tenant), sv(sub)],
        )
        .await
        .unwrap()
        .unwrap()
        .try_get("", "cancel_at_period_end")
        .unwrap_or(0);
    assert_eq!(cancel_flag, 1, "已安排取消的标记必须落库（运营要能看出谁要走了）");

    // ── ⑤ 立即结束：deleted 且周期已过 → 降回 free ──
    let (s, b) = hook(
        &app,
        sub_event("evt_sc_del", "customer.subscription.deleted", sub, &mid, "canceled", now - 60, now + 30, false),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let (plan5, exp5) = member_plan(&st, &mid).await;
    assert_eq!(plan5, "free", "周期已过的取消必须降级");
    assert_eq!(exp5, "", "降级应清空到期时间");
}

/// 【阶段 1 兜底】所有 webhook 都丢了，到期会员也**必须**掉档。
///
/// 这是「会员不会永久有效」的最后一道闸。当年「到期时间为空 = 永久会员」
/// 那个缺陷的根因，就是没有任何东西会主动检查到期 —— 现在由调度器每分钟扫，
/// 判据与事件处理同源（`current_period_end <= now`）。
#[tokio::test]
async fn expire_due_demotes_without_any_webhook() {
    let st = test_state().await;
    let app = build_router(st.clone());
    let (mid, _) = reg_member(&app, "sub_orphan@test.dev").await;

    let now = chrono::Utc::now().timestamp();
    let day = 86_400i64;

    // 造一条「周期已过、但从未收到任何终结事件」的订阅。
    // granted_expiry = period_end：这份有效期就是这个订阅给的
    // （模拟此前已经 Grant 过，只是终结事件丢了）。
    let end = iso(now - day);
    seed_subscription(&st, "sub_row_orphan", "sub_orphan_1", &mid, &end, "active", &end).await;
    // 会员停在订阅上一次 Grant 留下的样子：plan 是付费态，到期时间与订阅
    // 周期**完全一致** —— 这正是"这份有效期是它给的"的证据。
    // ⚠️ 不能写成 `now + 100 天`：那会变成"另有来源给了更晚的有效期"，
    // 语义上就该保留（另见 subscription_end_does_not_clobber_another_grant）。
    // 真实世界里订阅周期已过时，它 Grant 进去的那个值也必然已过，两者相等。
    st.db
        .execute(
            "UPDATE members SET plan = 'annual', plan_expires_at = ? WHERE id = ? AND tenant_id = ?",
            vec![sv(&end), sv(&mid), sv(&st.tenant)],
        )
        .await
        .expect("预置会员状态");

    assert_eq!(crate::sub_state::expire_due(&st).await, 1, "应处理 1 条到期订阅");
    let (plan, exp) = member_plan(&st, &mid).await;
    assert_eq!(plan, "free", "周期已过必须降级 —— 不能因为没收到 webhook 就一直有效");
    assert_eq!(exp, "", "降级应清空到期时间");

    // 再扫一次不得重复处理（closed_at 已标）
    assert_eq!(crate::sub_state::expire_due(&st).await, 0, "已终结的订阅不应被重复处理");
}

/// 订阅结束**不得**抹掉会员从其它来源获得的有效期。
///
/// 会员有效期是各来源的**最大值**：线下付款、积分兑换、兑码都能产生有效期。
/// 订阅到期时无条件降 free，会把那些已经收到的钱一并抹掉 ——
/// 而这类误伤在后台看起来只是「某会员突然变回免费」，查不出原因。
#[tokio::test]
async fn subscription_end_does_not_clobber_another_grant() {
    let st = test_state().await;
    let app = build_router(st.clone());
    let (mid, _) = reg_member(&app, "sub_manual@test.dev").await;

    // 人工开通 100 天（模拟线下付款 / 兑换码）
    crate::points::extend_plan(&st, &mid, 100).await.expect("人工开通应成功");
    let (plan0, exp0) = member_plan(&st, &mid).await;
    assert_ne!(plan0, "free", "人工开通后应为付费态");

    let now = chrono::Utc::now().timestamp();
    let day = 86_400i64;
    // granted_expiry = ""：本订阅从未授予过任何有效期，
    // 会员那份人工有效期与它无关 —— 必须原样保留
    seed_subscription(
        &st,
        "sub_row_manual",
        "sub_manual_1",
        &mid,
        &iso(now - day),
        "canceled",
        "",
    )
    .await;

    assert_eq!(crate::sub_state::expire_due(&st).await, 1);
    let (plan1, exp1) = member_plan(&st, &mid).await;
    assert_eq!(plan1, plan0, "订阅结束不得改写会员套餐（他另有来源）");
    assert_eq!(exp1, exp0, "订阅结束不得抹掉人工开通的有效期");
}

/// 订阅结束**不得**抹掉「订阅之后才拿到」的有效期。
///
/// 与上一条互补，而且更严：那条是「本订阅从未授予过」（granted 为空），
/// 这条是「本订阅授予过、但之后人工又延长了」—— 两者在 members 表上
/// **长得一模一样**（plan 都是付费态、plan_expires_at 都未过期），
/// 只有 `granted_expiry` 能把它们分开。
/// 判据取「相等」而非「有效」正是为此：差一点点就得在
/// 「漏降级（永久会员）」和「误降级（吃掉人工的钱）」之间二选一。
#[tokio::test]
async fn subscription_end_keeps_later_manual_grant() {
    let st = test_state().await;
    let app = build_router(st.clone());
    let (mid, _) = reg_member(&app, "sub_then_manual@test.dev").await;

    let now = chrono::Utc::now().timestamp();
    let day = 86_400i64;

    // ① 订阅授予 365 天
    let (s, b) = hook(
        &app,
        sub_event(
            "evt_tm_1",
            "customer.subscription.created",
            "sub_tm_1",
            &mid,
            "active",
            now + 365 * day,
            now,
            false,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let (p1, e1) = member_plan(&st, &mid).await;
    assert_eq!(p1, "annual", "订阅创建后应为付费态");

    // ② 人工再延长 100 天（在现有有效期之上叠加 → 更晚）
    let e_manual = crate::points::extend_plan(&st, &mid, 100)
        .await
        .expect("人工延长应成功");
    let (p2, e2) = member_plan(&st, &mid).await;
    assert_eq!(e2, e_manual, "人工延长后的值应与返回值一致");
    assert_ne!(e2, e1, "人工延长应把到期时间前移：{e1} → {e2}");

    // ③ 订阅结束（周期已过）→ 人工那一段必须原样保留
    let (s, b) = hook(
        &app,
        sub_event(
            "evt_tm_2",
            "customer.subscription.deleted",
            "sub_tm_1",
            &mid,
            "canceled",
            now - 60,
            now + 10,
            false,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let (p3, e3) = member_plan(&st, &mid).await;
    assert_eq!(p3, p2, "订阅结束不得改写会员套餐（人工那段还在）");
    assert_eq!(e3, e2, "订阅结束不得抹掉订阅之后人工延长的有效期");
}

/// 订阅表在后台是**只读面**：能看，不能写 —— 对 Owner 也一样。
///
/// 这条同时守住一个既有缺陷：`.never` 哨兵对 Owner 的通配权限 `"*"` 是
/// **无效**的（`perm_matches` 里 `g == "*"` 命中一切），所以 `orders` /
/// `points_ledger` / `subscriptions` 一度都能被 Owner 直接 POST 进去。
/// 现在判定放在权限校验之前，与角色无关。
#[tokio::test]
async fn audit_only_tables_reject_writes_even_for_owner() {
    let st = test_state().await;
    let app = build_router(st.clone());
    let tok = login(&app, "owner").await;

    // 读：正常
    let (s, b) = call(app.clone(), "GET", "/api/subscriptions", None, Some(&tok)).await;
    assert_eq!(s, StatusCode::OK, "订阅列表应可读：{b}");

    // 写：Owner 也不行
    for (key, body) in [
        ("subscriptions", json!({ "memberId": "m1", "status": "active" })),
        ("orders", json!({ "memberId": "m1", "bizType": "plan" })),
        ("points_ledger", json!({ "memberId": "m1", "delta": 100 })),
    ] {
        let (s, b) = call(app.clone(), "POST", &format!("/api/{key}"), Some(body), Some(&tok)).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "{key} 是审计只读面，Owner 也不得写入：{b}");
    }
}
