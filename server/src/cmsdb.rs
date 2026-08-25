//! 统一数据库层：本地 SQLite（sea-orm）或 Turso（libsql HTTP /v2/pipeline 协议）。
//!
//! 选择依据：sea-orm 1.1 无 libsql driver 且网络受限无法升级 2.x；
//! Turso 官方 HTTP 协议纯 JSON，用已有 reqwest 即可实现，零新依赖。
//!
//! 连接方式（环境变量优先）：
//! - 设 `TURSO_URL`（如 libsql://xxx.turso.io）+ `TURSO_AUTH_TOKEN` → Turso 远程；
//! - 否则本地 `sqlite://cms.db`（开发/测试不变）。
//!
//! 业务代码只依赖 CmsDb/Row，与后端选择解耦（SeaORM 固定表结构约定不变）。

use sea_orm::{ConnectionTrait, Statement, Value as SqlValue};
use std::sync::Arc;

/// 行值（SQLite 方言类型）
#[derive(Debug, Clone, PartialEq)]
pub enum DbVal {
    Text(String),
    Int(i64),
    Real(f64),
    Null,
}

#[derive(Debug)]
pub enum RowData {
    /// Turso：显式列名映射
    Named(Vec<(String, DbVal)>),
    /// 本地 SQLite：原生 QueryResult（按列名/索引读取）
    Sea(sea_orm::QueryResult),
}

#[derive(Debug)]
pub struct Row(pub RowData);

impl Row {
    fn named(cols: Vec<(String, DbVal)>) -> Self {
        Self(RowData::Named(cols))
    }

    fn sea(q: sea_orm::QueryResult) -> Self {
        Self(RowData::Sea(q))
    }

    pub fn try_get<T: FromDbVal + sea_orm::TryGetable>(&self, pre: &str, col: &str) -> Result<T, DbErr> {
        match &self.0 {
            RowData::Named(cols) => {
                let v = cols
                    .iter()
                    .find(|(c, _)| c == col)
                    .map(|(_, v)| v.clone())
                    .ok_or_else(|| DbErr(format!("列不存在：{col}")))?;
                T::from_val(v)
            }
            RowData::Sea(q) => q.try_get::<T>(pre, col).map_err(|e| DbErr(format!("读取失败：{e}"))),
        }
    }
}

/// 从行值反序列化（与 sea-orm Row::try_get 用法对齐）
pub trait FromDbVal: Sized {
    fn from_val(v: DbVal) -> Result<Self, DbErr>;
}

impl FromDbVal for String {
    fn from_val(v: DbVal) -> Result<Self, DbErr> {
        match v {
            DbVal::Text(s) => Ok(s),
            DbVal::Int(i) => Ok(i.to_string()),
            DbVal::Real(f) => Ok(f.to_string()),
            DbVal::Null => Err(DbErr("NULL 不能读为 String".into())),
        }
    }
}

impl FromDbVal for i64 {
    fn from_val(v: DbVal) -> Result<Self, DbErr> {
        match v {
            DbVal::Int(i) => Ok(i),
            DbVal::Text(s) => s.parse().map_err(|_| DbErr(format!("不是整数：{s}"))),
            DbVal::Real(f) => Ok(f as i64),
            DbVal::Null => Err(DbErr("NULL 不能读为 i64".into())),
        }
    }
}

impl FromDbVal for Option<String> {
    fn from_val(v: DbVal) -> Result<Self, DbErr> {
        Ok(match v {
            DbVal::Null => None,
            other => Some(String::from_val(other)?),
        })
    }
}

impl FromDbVal for f64 {
    fn from_val(v: DbVal) -> Result<Self, DbErr> {
        match v {
            DbVal::Real(f) => Ok(f),
            DbVal::Int(i) => Ok(i as f64),
            DbVal::Text(t) => t.parse().map_err(|_| DbErr(format!("不是浮点：{t}"))),
            DbVal::Null => Err(DbErr("NULL 不能读为 f64".into())),
        }
    }
}

impl FromDbVal for Option<i64> {
    fn from_val(v: DbVal) -> Result<Self, DbErr> {
        Ok(match v {
            DbVal::Null => None,
            other => Some(i64::from_val(other)?),
        })
    }
}

#[derive(Debug)]
pub struct DbErr(pub String);

impl std::fmt::Display for DbErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// 后端选择
#[derive(Clone)]
pub enum Backend {
    /// 本地 SQLite 文件（sea-orm）
    Local(sea_orm::DatabaseConnection),
    /// Turso 远程（HTTP pipeline）
    Turso { client: reqwest::Client, url: String, token: String },
}

#[derive(Clone)]
pub struct CmsDb(pub Arc<Backend>);

impl CmsDb {
    /// 依据环境变量连接：TURSO_URL → Turso；否则本地文件
    pub async fn connect() -> Result<Self, String> {
        match std::env::var("TURSO_URL") {
            Ok(raw) => {
                let token = std::env::var("TURSO_AUTH_TOKEN").map_err(|_| "TURSO_URL 已设置但缺少 TURSO_AUTH_TOKEN".to_string())?;
                let url = raw
                    .trim_end_matches('/')
                    .replace("libsql://", "https://")
                    .replace("ws://", "https://")
                    .replace("wss://", "https://");
                if !url.starts_with("https://")
                    && !(url.starts_with("http://") && (url.contains("127.0.0.1") || url.contains("localhost")))
                {
                    return Err(format!("不支持的 TURSO_URL：{url}（需要 libsql:// 或 https://；http 仅限本地调试）"));
                }
                Ok(Self(Arc::new(Backend::Turso {
                    client: reqwest::Client::new(),
                    url,
                    token,
                })))
            }
            Err(_) => {
                let db = sea_orm::Database::connect("sqlite://cms.db?mode=rwc")
                    .await
                    .map_err(|e| format!("本地库连接失败：{e}"))?;
                Ok(Self(Arc::new(Backend::Local(db))))
            }
        }
    }

    /// 本地 SQLite（测试/开发）
    pub fn local(db: sea_orm::DatabaseConnection) -> Self {
        Self(Arc::new(Backend::Local(db)))
    }

    /// Turso 远程（测试）
    pub fn turso(url: &str, token: &str) -> Self {
        Self(Arc::new(Backend::Turso {
            client: reqwest::Client::new(),
            url: url.trim_end_matches('/').to_string(),
            token: token.to_string(),
        }))
    }

    pub fn backend(&self) -> &Backend {
        &self.0
    }

    fn stmt_args(st: Statement) -> (String, Vec<SqlValue>) {
        let sql = st.sql;
        let args: Vec<SqlValue> = match st.values {
            Some(v) => v.0,
            None => Vec::new(),
        };
        (sql, args)
    }

    /// 兼容 sea-orm Statement 的写执行（返回受影响行数）
    pub async fn execute_statement(&self, st: Statement) -> Result<u64, DbErr> {
        let (sql, args) = Self::stmt_args(st);
        self.execute(&sql, args).await
    }

    /// 兼容 sea-orm Statement 的单行查询
    pub async fn query_one_statement(&self, st: Statement) -> Result<Option<Row>, DbErr> {
        let (sql, args) = Self::stmt_args(st);
        self.query_one(&sql, args).await
    }

    /// 兼容 sea-orm Statement 的多行查询
    pub async fn query_all_statement(&self, st: Statement) -> Result<Vec<Row>, DbErr> {
        let (sql, args) = Self::stmt_args(st);
        self.query_all(&sql, args).await
    }

    /// 执行写语句，返回受影响行数
    pub async fn execute(&self, sql: &str, args: Vec<SqlValue>) -> Result<u64, DbErr> {
        match &*self.0 {
            Backend::Local(db) => {
                let st = Statement::from_sql_and_values(db.get_database_backend(), sql, args);
                let r = db
                    .execute_raw(st)
                    .await
                    .map_err(|e| DbErr(format!("SQLite 执行失败：{e}")))?;
                Ok(r.rows_affected())
            }
            Backend::Turso { client, url, token, .. } => {
                let n = turso_pipeline(client, url, token, "write", sql, args).await?;
                Ok(n)
            }
        }
    }

    /// 查单行（无结果返回 None）
    pub async fn query_one(&self, sql: &str, args: Vec<SqlValue>) -> Result<Option<Row>, DbErr> {
        Ok(self.query_all(sql, args).await?.into_iter().next())
    }

    /// 查多行
    pub async fn query_all(&self, sql: &str, args: Vec<SqlValue>) -> Result<Vec<Row>, DbErr> {
        match &*self.0 {
            Backend::Local(db) => {
                let st = Statement::from_sql_and_values(db.get_database_backend(), sql, args);
                let rows = db
                    .query_all_raw(st)
                    .await
                    .map_err(|e| DbErr(format!("SQLite 查询失败：{e}")))?;
                Ok(rows.into_iter().map(Row::sea).collect())
            }
            Backend::Turso { client, url, token, .. } => {
                turso_query(client, url, token, sql, args).await
            }
        }
    }
}

// ── Turso libsql HTTP 协议 ────────────────────────────

fn to_param(v: &SqlValue) -> serde_json::Value {
    use sea_orm::Value as V;
    match v {
        V::String(Some(s)) => serde_json::json!({ "type": "text", "value": s }),
        V::BigInt(Some(n)) => serde_json::json!({ "type": "integer", "value": n }),
        V::Int(Some(n)) => serde_json::json!({ "type": "integer", "value": n }),
        V::Decimal(Some(d)) => serde_json::json!({ "type": "real", "value": d }),
        V::Float(Some(f)) => serde_json::json!({ "type": "real", "value": f }),
        V::Bool(Some(b)) => serde_json::json!({ "type": "integer", "value": if *b { 1 } else { 0 } }),
        _ => serde_json::json!({ "type": "null", "value": null }),
    }
}

fn parse_row_val(v: &serde_json::Value) -> DbVal {
    match v {
        serde_json::Value::String(s) => DbVal::Text(s.clone()),
        serde_json::Value::Number(n) => match n.as_i64() {
            Some(i) => DbVal::Int(i),
            None => DbVal::Real(n.as_f64().unwrap_or(0.0)),
        },
        serde_json::Value::Bool(b) => DbVal::Int(if *b { 1 } else { 0 }),
        serde_json::Value::Object(o) => {
            // libsql 可能返回 {type, value} 包装
            let val = o.get("value").cloned().unwrap_or(serde_json::Value::Null);
            parse_row_val(&val)
        }
        _ => DbVal::Null,
    }
}

async fn turso_pipeline(
    client: &reqwest::Client,
    url: &str,
    token: &str,
    tx: &str,
    sql: &str,
    args: Vec<SqlValue>,
) -> Result<u64, DbErr> {
    let params: Vec<serde_json::Value> = args.iter().map(to_param).collect();
    let body = serde_json::json!({
        "requests": [{
            "type": "execute",
            "stmt": { "sql": sql, "args": params },
        }],
        "transaction": tx,
    });
    let resp = client
        .post(format!("{url}/v2/pipeline"))
        .bearer_auth(token)
        .json(&body)
        .send()
        .await
        .map_err(|e| DbErr(format!("Turso 请求失败：{e}")))?;
    let status = resp.status();
    let json: serde_json::Value = resp.json().await.map_err(|e| DbErr(format!("Turso 响应解析失败：{e}")))?;
    if !status.is_success() {
        return Err(DbErr(format!("Turso 错误 {}：{}", status, json.get("error").map(|e| e.to_string()).unwrap_or_default())));
    }
    let first = json
        .pointer("/results/0/response/result")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    Ok(first.get("rows_written").and_then(|n| n.as_u64()).unwrap_or(0))
}

async fn turso_query(
    client: &reqwest::Client,
    url: &str,
    token: &str,
    sql: &str,
    args: Vec<SqlValue>,
) -> Result<Vec<Row>, DbErr> {
    let params: Vec<serde_json::Value> = args.iter().map(to_param).collect();
    let body = serde_json::json!({
        "requests": [{
            "type": "execute",
            "stmt": { "sql": sql, "args": params },
        }],
        "transaction": "read",
    });
    let resp = client
        .post(format!("{url}/v2/pipeline"))
        .bearer_auth(token)
        .json(&body)
        .send()
        .await
        .map_err(|e| DbErr(format!("Turso 请求失败：{e}")))?;
    let status = resp.status();
    let json: serde_json::Value = resp.json().await.map_err(|e| DbErr(format!("Turso 响应解析失败：{e}")))?;
    if !status.is_success() {
        return Err(DbErr(format!("Turso 错误 {}：{}", status, json.get("error").map(|e| e.to_string()).unwrap_or_default())));
    }
    let result = json.pointer("/results/0/response/result").cloned().unwrap_or(serde_json::Value::Null);
    let cols: Vec<String> = result
        .get("cols")
        .and_then(|c| c.as_array())
        .map(|arr| arr.iter().filter_map(|c| c.get("name").and_then(|n| n.as_str()).map(String::from)).collect())
        .unwrap_or_default();
    let rows = result.get("rows").and_then(|r| r.as_array()).cloned().unwrap_or_default();
    Ok(rows
        .iter()
        .map(|row| {
            let vals = row.as_array().cloned().unwrap_or_default();
            let mut out = Vec::with_capacity(cols.len());
            for (i, name) in cols.iter().enumerate() {
                let v = vals.get(i).cloned().unwrap_or(serde_json::Value::Null);
                out.push((name.clone(), parse_row_val(&v)));
            }
            Row::named(out)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_try_get_variants() {
        let r = Row::named(vec![
            ("id".into(), DbVal::Text("abc".into())),
            ("n".into(), DbVal::Int(7)),
            ("opt".into(), DbVal::Null),
        ]);
        assert_eq!(r.try_get::<String>("", "id").unwrap(), "abc");
        assert_eq!(r.try_get::<i64>("", "n").unwrap(), 7);
        assert_eq!(r.try_get::<Option<String>>("", "opt").unwrap(), None);
        assert!(r.try_get::<i64>("", "id").is_err());
        assert!(r.try_get::<String>("", "missing").is_err());
    }

    #[test]
    fn parse_wrapped_and_bare_values() {
        assert_eq!(parse_row_val(&serde_json::json!("x")), DbVal::Text("x".into()));
        assert_eq!(parse_row_val(&serde_json::json!(3)), DbVal::Int(3));
        assert_eq!(parse_row_val(&serde_json::json!({"type":"text","value":"y"})), DbVal::Text("y".into()));
        assert_eq!(parse_row_val(&serde_json::json!(null)), DbVal::Null);
    }
}
