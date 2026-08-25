//! 统一响应信封与错误类型 —— 与前端 api()/apiList() 约定严格对齐：
//! 成功 {ok:true,data[,total]}；失败 {ok:false,error}。

use axum::{http::StatusCode, response::{IntoResponse, Response}, Json};
use serde_json::json;

pub fn ok(data: serde_json::Value) -> ApiResult {
    Ok((StatusCode::OK, Json(json!({ "ok": true, "data": data }))).into_response())
}

pub fn ok_list(data: Vec<serde_json::Value>, total: usize) -> ApiResult {
    Ok((StatusCode::OK, Json(json!({ "ok": true, "data": data, "total": total }))).into_response())
}

/// 业务错误：映射为 HTTP 状态码 + 信封
#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub message: String,
}

impl ApiError {
    pub fn bad(msg: impl Into<String>) -> Self {
        Self { status: StatusCode::BAD_REQUEST, message: msg.into() }
    }
    pub fn unauthorized(msg: impl Into<String>) -> Self {
        Self { status: StatusCode::UNAUTHORIZED, message: msg.into() }
    }
    pub fn forbidden(perm: &str) -> Self {
        Self { status: StatusCode::FORBIDDEN, message: format!("需要权限：{perm}") }
    }
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self { status: StatusCode::NOT_FOUND, message: msg.into() }
    }
    pub fn rate_limited(msg: impl Into<String>) -> Self {
        Self { status: StatusCode::TOO_MANY_REQUESTS, message: msg.into() }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({ "ok": false, "error": self.message }))).into_response()
    }
}

pub type ApiResult = Result<Response, ApiError>;
