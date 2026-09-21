//! API 集成测试（C3）：对 build_router 直接发请求（oneshot），
//! 覆盖登录/RBAC/限速/审批/公开表单等关键链路，无需起真实端口。

use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
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

    // B 用邀请码注册 → B.invited_by = A
    let (s, rb) = call(app.clone(), "POST", "/api/public/members/register",
        Some(json!({"email":"b@test.dev","password":"member1234","name":"被邀请人","invite_code":"INV-A"})), None).await;
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
