//! Domain Model V1 的 HTTP 契约层（`/api/v1/*`）。
//!
//! ## 为什么这里用参数化 SQL 而不是 SeaORM entity 查询
//! `CmsDb`（见 `cmsdb.rs`）为兼容 Turso 远程后端，只对外暴露
//! `execute / query_one / query_all` 三个基于 SQL 字符串的方法，
//! **没有实现 `ConnectionTrait`**，因此 `Entity::find()` 这类查询用不上。
//! `src/entity/` 的 12 个 entity 是**表结构的类型化契约**（字段名、类型、
//! 关系、可空性由编译器把关），本层的 SQL 列名与之一一对应；
//! 待 `CmsDb` 补上 `ConnectionTrait` 实现后即可切换为 entity 查询。
//!
//! 这样保证：SQLite 与 Turso 两条路径行为完全一致。

use axum::Json;
use sea_orm::Value as SqlValue;
use serde_json::{Map, Value};

use crate::cmsdb::{DbErr, Row};
use crate::error::{ApiError, ApiResult};

pub mod channels;
pub mod content;
pub mod contexts;
pub mod render;
pub mod sites;
pub mod templates;
pub mod themes;

// ── 通用小工具 ────────────────────────────────────────────────────────────

pub(crate) fn now() -> String {
    crate::db::now_iso()
}

pub(crate) fn s(v: String) -> SqlValue {
    SqlValue::String(Some(v))
}

pub(crate) fn so(v: Option<String>) -> SqlValue {
    SqlValue::String(v)
}

/// 数据库错误 → 业务错误（不暴露内部细节给前端）
pub(crate) fn db_err(ctx: &str) -> impl Fn(DbErr) -> ApiError + '_ {
    move |e| ApiError::bad(format!("{ctx}：{e}"))
}

/// 取 TEXT 列（缺失视为空串）
pub(crate) fn txt(r: &Row, c: &str) -> String {
    r.try_get::<Option<String>>("", c).ok().flatten().unwrap_or_default()
}

/// 取可空 TEXT 列
pub(crate) fn opt(r: &Row, c: &str) -> Option<String> {
    r.try_get::<Option<String>>("", c).ok().flatten()
}

/// 取 INTEGER 列
pub(crate) fn int(r: &Row, c: &str) -> i64 {
    r.try_get::<i64>("", c).unwrap_or(0)
}

/// 取 REAL 列
pub(crate) fn real(r: &Row, c: &str) -> f64 {
    r.try_get::<f64>("", c).unwrap_or(0.0)
}

/// 取 JSON 文本列并解析；解析失败退化为 `{}`，不让脏数据打断整个列表
pub(crate) fn js(r: &Row, c: &str) -> Value {
    match opt(r, c) {
        Some(raw) => serde_json::from_str::<Value>(&raw).unwrap_or_else(|_| Value::Object(Map::new())),
        None => Value::Object(Map::new()),
    }
}

/// 把请求体里的 JSON 字段序列化回文本列（缺省 `{}`）
pub(crate) fn to_json_text(v: Option<&Value>) -> String {
    match v {
        Some(Value::Object(_)) | Some(Value::Array(_)) => v.unwrap().to_string(),
        _ => "{}".to_string(),
    }
}

/// 单条记录查询（找不到 → 404）
pub(crate) async fn must_get(
    st: &crate::state::AppState,
    table: &str,
    id: &str,
) -> Result<Row, ApiError> {
    st.db
        .query_one(
            &format!("SELECT * FROM {table} WHERE id = ? LIMIT 1"),
            vec![s(id.to_string())],
        )
        .await
        .map_err(db_err("查询失败"))?
        .ok_or_else(|| ApiError::not_found(format!("{table} 中不存在 {id}")))
}

/// `{ ok: true, data }` 成功信封
pub(crate) fn ok(data: Value) -> ApiResult {
    crate::error::ok(data)
}

/// `{ ok: true, data, total }` 列表信封
pub(crate) fn ok_list(data: Vec<Value>, total: usize) -> ApiResult {
    crate::error::ok_list(data, total)
}

/// 新建/更新成功后回执统一形态，前端据此刷新缓存
pub(crate) fn ok_id(id: String) -> ApiResult {
    ok(serde_json::json!({ "id": id }))
}

/// 空对象体（用于只回执 ok:true 的操作）
pub(crate) fn ok_empty() -> ApiResult {
    ok(serde_json::json!({}))
}

/// 供各模块复用的 JSON 提取器类型别名
pub(crate) type JsonBody = Json<Value>;

/// 从请求体取字符串字段
pub(crate) fn b_str(b: &Value, k: &str) -> Option<String> {
    b.get(k).and_then(|v| v.as_str()).map(|s| s.to_string())
}

/// 从请求体取 JSON 字段
pub(crate) fn b_js(b: &Value, k: &str) -> Option<Value> {
    b.get(k).cloned()
}
