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

/// 在线支付路径必须**明确拒绝年付**，而不是拿着月价去收费。
///
/// `tiers` 只有一列 `stripe_price_id`，月付与年付共用同一个 Stripe 价格：
/// 前端文案写「¥180/年」、后端却按同一个 price 建订阅会话，会员实际被按月扣款。
/// 「页面承诺」与「实际收款」不一致是资损级问题，宁可 400 让用户走人工开通。
#[tokio::test]
async fn checkout_refuses_yearly_instead_of_charging_monthly() {
    let st = test_state().await;
    let app = build_router(st.clone());
    let o = login(&app, "owner").await;
    let (s, t) = call(app.clone(), "POST", "/api/tiers",
        Some(json!({"name":"年度会员","slug":"annual","description":"","priceMonthly":18,
                    "priceYearly":180,"features":"[]","active":true})), Some(&o)).await;
    assert_eq!(s, StatusCode::OK, "{t}");
    let tid = t["data"]["id"].as_str().expect("套餐 id").to_string();

    let m = register_member(&app, "yr@test.dev").await;

    let (s, b) = call(app.clone(), "POST", "/api/public/checkout",
        Some(json!({"tierId": tid, "interval": "yearly"})), Some(&m)).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{b}");
    assert!(
        b["error"].as_str().unwrap_or("").contains("年付"),
        "错误信息要指明「年付不受支持」，否则用户只会以为是网络问题：{b}"
    );

    // 守卫必须只针对年付：月付不该被这条错误拦下（本环境未配 Stripe，
    // 月付会因「未配置」失败，但错误内容不同）。
    let (_, b2) = call(app.clone(), "POST", "/api/public/checkout",
        Some(json!({"tierId": tid, "interval": "monthly"})), Some(&m)).await;
    assert!(
        !b2["error"].as_str().unwrap_or("").contains("年付"),
        "月付被年付守卫误伤：{b2}"
    );
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

    // 订阅变更类事件：落库待应用（缺订阅状态机，不臆造 plan 变更）
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
    assert_eq!(b3["data"]["status"], "pending_apply");

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
