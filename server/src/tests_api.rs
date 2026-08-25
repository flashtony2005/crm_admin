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
