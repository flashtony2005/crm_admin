//! 数据库连接 + 固定表结构（TD-1：SeaORM 固定表，无本体动态建模）+ 种子数据。
//!
//! 表结构是**编译期固定的 DDL**；Phase 2 引入复杂查询时再为各表补
//! SeaORM entity 结构体，Phase 1 统一资源网关直接走参数化 SQL。

use sea_orm::{ConnectOptions, Database};
use std::time::Duration;

use crate::{auth::hash_password, cmsdb::CmsDb};

/// 连接：TURSO_URL → Turso 远程；否则本地 SQLite 文件
pub async fn connect() -> CmsDb {
    match std::env::var("TURSO_URL") {
        Ok(_) => CmsDb::connect().await.expect("Turso 连接失败"),
        Err(_) => {
            let url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:./cms.db?mode=rwc".into());
            let mut opts = ConnectOptions::new(url);
            opts.max_connections(5).connect_timeout(Duration::from_secs(30));
            let db = Database::connect(opts).await.expect("数据库连接失败");
            CmsDb::local(db)
        }
    }
}

fn exec(sql: &str) -> sea_orm::Statement {
    sea_orm::Statement::from_string(sea_orm::DatabaseBackend::Sqlite, sql.to_string())
}

/// 启动时执行：建表（幂等）+ 种子（仅空库时）
pub async fn bootstrap(db: &CmsDb) {
    for ddl in [
        "CREATE TABLE IF NOT EXISTS tenants (
            id TEXT PRIMARY KEY, name TEXT NOT NULL, created_at TEXT NOT NULL)",
        "CREATE TABLE IF NOT EXISTS users (
            id TEXT PRIMARY KEY, username TEXT NOT NULL UNIQUE, nickname TEXT NOT NULL,
            email TEXT DEFAULT '', password_hash TEXT NOT NULL, role TEXT NOT NULL, tenant_id TEXT NOT NULL,
            status INTEGER NOT NULL DEFAULT 1, created_at TEXT NOT NULL, updated_at TEXT NOT NULL)",
        "CREATE TABLE IF NOT EXISTS articles (
            id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL,
            title TEXT DEFAULT '', summary TEXT DEFAULT '', content TEXT DEFAULT '',
            status TEXT DEFAULT 'draft', author TEXT DEFAULT '', tags TEXT DEFAULT '[]',
            created_at TEXT NOT NULL, updated_at TEXT NOT NULL)",
        "CREATE TABLE IF NOT EXISTS pages (
            id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL,
            title TEXT DEFAULT '', slug TEXT DEFAULT '', content TEXT DEFAULT '',
            status TEXT DEFAULT 'draft', created_at TEXT NOT NULL, updated_at TEXT NOT NULL)",
        "CREATE TABLE IF NOT EXISTS products (
            id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL,
            name TEXT DEFAULT '', price REAL DEFAULT 0, description TEXT DEFAULT '',
            status TEXT DEFAULT 'draft', created_at TEXT NOT NULL, updated_at TEXT NOT NULL)",
        "CREATE TABLE IF NOT EXISTS media_items (
            id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL,
            name TEXT DEFAULT '', url TEXT DEFAULT '', size INTEGER DEFAULT 0, kind TEXT DEFAULT 'image',
            created_at TEXT NOT NULL, updated_at TEXT NOT NULL)",
        "CREATE INDEX IF NOT EXISTS idx_articles_tenant ON articles(tenant_id, updated_at)",
        "CREATE TABLE IF NOT EXISTS customers (
            id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL,
            name TEXT DEFAULT '', phone TEXT DEFAULT '', source TEXT DEFAULT '',
            tags TEXT DEFAULT '[]', priority TEXT DEFAULT 'normal', note TEXT DEFAULT '',
            last_contact_at TEXT DEFAULT '',
            created_at TEXT NOT NULL, updated_at TEXT NOT NULL)",
        "CREATE TABLE IF NOT EXISTS leads (
            id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL,
            name TEXT DEFAULT '', phone TEXT DEFAULT '', interest TEXT DEFAULT '',
            source TEXT DEFAULT '', status TEXT DEFAULT 'new',
            created_at TEXT NOT NULL, updated_at TEXT NOT NULL)",
        "CREATE TABLE IF NOT EXISTS forms (
            id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL,
            title TEXT DEFAULT '', descr TEXT DEFAULT '', field_count INTEGER DEFAULT 0,
            submissions INTEGER DEFAULT 0, status TEXT DEFAULT 'published',
            created_at TEXT NOT NULL, updated_at TEXT NOT NULL)",
        "CREATE TABLE IF NOT EXISTS form_submissions (
            id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL,
            form_id TEXT DEFAULT '', data TEXT DEFAULT '',
            created_at TEXT NOT NULL)",
        "CREATE TABLE IF NOT EXISTS approvals (
            id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL,
            action TEXT DEFAULT 'publish', target TEXT DEFAULT '',
            requested_by TEXT DEFAULT '', risk TEXT DEFAULT 'low',
            status TEXT DEFAULT 'pending', summary TEXT DEFAULT '',
            decided_at TEXT,
            created_at TEXT NOT NULL, updated_at TEXT NOT NULL)",
        "CREATE TABLE IF NOT EXISTS ai_tasks (
            id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL,
            title TEXT DEFAULT '', capability TEXT DEFAULT '',
            status TEXT DEFAULT 'running', result TEXT,
            created_at TEXT NOT NULL, updated_at TEXT NOT NULL)",
        "CREATE TABLE IF NOT EXISTS workflows (
            id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL,
            name TEXT DEFAULT '', trigger_expr TEXT DEFAULT '',
            event TEXT DEFAULT '',
            steps TEXT DEFAULT '',
            step_count INTEGER DEFAULT 1, enabled INTEGER DEFAULT 1,
            last_run_at TEXT,
            created_at TEXT NOT NULL, updated_at TEXT NOT NULL)",
        "CREATE TABLE IF NOT EXISTS ai_audit_log (
            id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL,
            actor TEXT DEFAULT '', actor_role TEXT DEFAULT '',
            capability TEXT DEFAULT '', decision TEXT DEFAULT '',
            target_id TEXT DEFAULT '', detail TEXT DEFAULT '',
            created_at TEXT NOT NULL)",
        "CREATE TABLE IF NOT EXISTS integrations (
            id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL,
            key TEXT DEFAULT '', name TEXT DEFAULT '', descr TEXT DEFAULT '',
            category TEXT DEFAULT 'seo', connected INTEGER DEFAULT 0,
            created_at TEXT NOT NULL, updated_at TEXT NOT NULL)",
    ] {
        db.execute_statement(exec(ddl)).await.expect("建表失败");
    }

    // 种子：仅当 users 为空
    let n = db
        .query_one_statement(exec("SELECT COUNT(*) AS n FROM users"))
        .await
        .expect("count")
        .map(|r| r.try_get::<i64>("", "n").unwrap_or(0))
        .unwrap_or(0);
    if n > 0 {
        return;
    }
    let now = now_iso();
    let hash = hash_password("demo1234");
    for (id, username, nickname, role) in [
        ("u_owner", "owner", "老板（Owner）", "owner"),
        ("u_editor", "editor", "店员（Editor）", "editor"),
        ("u_viewer", "viewer", "观察者（Viewer）", "viewer"),
    ] {
        db.execute_statement(exec(&format!(
            "INSERT INTO users (id, username, nickname, password_hash, role, tenant_id, status, created_at, updated_at) \
             VALUES ('{id}', '{username}', '{nickname}', '{HASH}', '{role}', 't_demo', 1, '{now}', '{now}')",
            HASH = hash.replace('\'', "''"),
        )))
        .await
        .expect("seed user");
    }
    db.execute_statement(exec(&format!(
        "INSERT INTO tenants (id, name, created_at) VALUES ('t_demo', '桂花栗子烘焙坊', '{now}')"
    )))
    .await
    .expect("seed tenant");

    /// 种子插入：先收集 SQL，最后统一顺序执行（避免闭包持有 await）
    let mut seed_sqls: Vec<String> = Vec::new();
    let ins_row = |seed_sqls: &mut Vec<String>, table: &str, vals: &[(&str, String)]| {
        let mut vals = vals.to_vec();
        // 自动补时间戳（种子行统一写同一时刻）
        if !vals.iter().any(|(c, _)| *c == "created_at") {
            let now = now_iso();
            vals.push(("created_at", now.clone()));
            vals.push(("updated_at", now));
        }
        let cols: Vec<&str> = vals.iter().map(|(c, _)| *c).collect();
        let vs: Vec<String> = vals
            .iter()
            .map(|(_, v)| format!("'{}'", v.replace('\'', "''")))
            .collect();
        seed_sqls.push(format!(
            "INSERT INTO {} ({}) VALUES ({})",
            table,
            cols.join(", "),
            vs.join(", ")
        ));
    };
    macro_rules! seed {
        ($table:expr, $vals:expr) => {
            ins_row(&mut seed_sqls, $table, &$vals.iter().map(|(a, b): &(&str, String)| (*a, b.clone())).collect::<Vec<_>>())
        };
    }

    // 客户 / 线索 / 表单
    for row in [
        ("customers", vec![
            ("id", uuid::Uuid::new_v4().to_string()), ("tenant_id", "t_demo".into()),
            ("name", "王女士".into()), ("phone", "13800000001".into()),
            ("source", "门店".into()), ("tags", "[\"会员\"]".into()),
            ("priority", "high".into()), ("note", "每周六固定买欧包".into()),
            ("last_contact_at", now.clone()),
        ]),
        ("customers", vec![
            ("id", uuid::Uuid::new_v4().to_string()), ("tenant_id", "t_demo".into()),
            ("name", "李先生".into()), ("phone", "13800000002".into()),
            ("source", "美团".into()), ("tags", "[]".into()),
            ("priority", "mid".into()), ("note", "".into()),
            ("last_contact_at", now.clone()),
        ]),
        ("leads", vec![
            ("id", uuid::Uuid::new_v4().to_string()), ("tenant_id", "t_demo".into()),
            ("name", "张女士".into()), ("phone", "13900000001".into()),
            ("interest", "生日蛋糕定制".into()), ("source", "小程序".into()),
            ("status", "new".into()),
        ]),
        ("leads", vec![
            ("id", uuid::Uuid::new_v4().to_string()), ("tenant_id", "t_demo".into()),
            ("name", "刘先生".into()), ("phone", "13900000002".into()),
            ("interest", "企业下午茶团购".into()), ("source", "电话".into()),
            ("status", "following".into()),
        ]),
        ("forms", vec![
            ("id", uuid::Uuid::new_v4().to_string()), ("tenant_id", "t_demo".into()),
            ("title", "蛋糕预订表单".into()), ("descr", "留下联系方式，我们 1 小时内致电确认订单。".into()),
            ("field_count", "5".into()), ("submissions", "23".into()), ("status", "published".into()),
        ]),
        ("forms", vec![
            ("id", uuid::Uuid::new_v4().to_string()), ("tenant_id", "t_demo".into()),
            ("title", "烘焙课报名".into()), ("descr", "每周末两小时，零基础也能做出第一炉面包。".into()),
            ("field_count", "4".into()), ("submissions", "8".into()), ("status", "published".into()),
        ]),
    ] {
        seed!(row.0, row.1);
    }

    // 审批（一条 pending 供 Owner 裁决演示）与 AI 任务
    for row in [
        ("approvals", vec![
            ("id", uuid::Uuid::new_v4().to_string()), ("tenant_id", "t_demo".into()),
            ("action", "publish".into()),
            ("target", "article:中秋礼盒预售文".into()),
            ("requested_by", "AI 助手（店员发起）".into()),
            ("risk", "mid".into()), ("status", "pending".into()),
            ("summary", "发布前需要确认价格信息无误".into()),
        ]),
        ("ai_tasks", vec![
            ("id", uuid::Uuid::new_v4().to_string()), ("tenant_id", "t_demo".into()),
            ("title", "为秋季新品生成 3 条推广文案".into()),
            ("capability", "content.articles.create".into()),
            ("status", "done".into()), ("result", "已生成并存入草稿箱".into()),
        ]),
    ] {
        seed!(row.0, row.1);
    }

    // 自动化流程 / 集成
    for row in [
        ("workflows", vec![
            ("id", uuid::Uuid::new_v4().to_string()), ("tenant_id", "t_demo".into()),
            ("name", "每周经营摘要".into()), ("trigger_expr", "每周一 08:00".into()),
            ("event", "schedule.weekly".into()),
            ("step_count", "3".into()), ("enabled", "1".into()),
        ]),
        ("workflows", vec![
            ("id", uuid::Uuid::new_v4().to_string()), ("tenant_id", "t_demo".into()),
            ("name", "新客户欢迎消息".into()), ("trigger_expr", "客户创建时".into()),
            ("event", "customer.created".into()),
            ("steps", "[{\"type\":\"notify\",\"message\":\"新客户 {name} 已登记，请安排跟进\"},{\"type\":\"task\",\"title\":\"跟进客户 {name}\"}]".into()),
            ("step_count", "2".into()), ("enabled", "1".into()),
        ]),
        ("integrations", vec![
            ("id", uuid::Uuid::new_v4().to_string()), ("tenant_id", "t_demo".into()),
            ("key", "wechat".into()), ("name", "微信服务号".into()),
            ("descr", "模板消息推送".into()), ("category", "message".into()),
            ("connected", "1".into()),
        ]),
        ("integrations", vec![
            ("id", uuid::Uuid::new_v4().to_string()), ("tenant_id", "t_demo".into()),
            ("key", "wecom-bot".into()), ("name", "企业微信群机器人".into()),
            ("descr", "粘贴群机器人 Webhook 地址到 API Key，即可在群里收到审批推送".into()),
            ("category", "message".into()),
            ("connected", "0".into()),
        ]),
        ("integrations", vec![
            ("id", uuid::Uuid::new_v4().to_string()), ("tenant_id", "t_demo".into()),
            ("key", "ga".into()), ("name", "Google Analytics".into()),
            ("descr", "网站流量分析".into()), ("category", "analytics".into()),
            ("oauth_provider", "google".into()),
            ("connected", "0".into()),
        ]),
    ] {
        seed!(row.0, row.1);
    }

    // 页面 / 产品 / 素材
    for row in [
        ("pages", vec![
            ("id", uuid::Uuid::new_v4().to_string()), ("tenant_id", "t_demo".into()),
            ("title", "关于我们".into()), ("slug", "about".into()),
            ("content", "桂花栗子烘焙坊，用当季食材做有温度的点心。".into()),
            ("status", "published".into()),
        ]),
        ("products", vec![
            ("id", uuid::Uuid::new_v4().to_string()), ("tenant_id", "t_demo".into()),
            ("name", "桂花栗子欧包".into()), ("price", "18.5".into()),
            ("description", "秋日限定，每日限量 30 个。".into()),
            ("status", "published".into()),
        ]),
        ("products", vec![
            ("id", uuid::Uuid::new_v4().to_string()), ("tenant_id", "t_demo".into()),
            ("name", "手作可礼盒（6 装）".into()), ("price", "98".into()),
            ("description", "可颂 + 欧包混搭，支持企业定制。".into()),
            ("status", "draft".into()),
        ]),
        ("media_items", vec![
            ("id", uuid::Uuid::new_v4().to_string()), ("tenant_id", "t_demo".into()),
            ("name", "欧包主图.jpg".into()), ("url", "/media/seed-bread.jpg".into()),
            ("size", "204800".into()), ("kind", "image".into()),
        ]),
        ("media_items", vec![
            ("id", uuid::Uuid::new_v4().to_string()), ("tenant_id", "t_demo".into()),
            ("name", "门店环境照.jpg".into()), ("url", "/media/seed-shop.jpg".into()),
            ("size", "3145728".into()), ("kind", "image".into()),
        ]),
    ] {
        seed!(row.0, row.1);
    }

    // 内容种子（与前端 mock 同源的面包店场景）
    for (title, summary, status, author) in [
        ("秋季新品：桂花栗子欧包上市", "板栗与桂花蜜的秋日限定，本周五起门店供应。", "published", "老板"),
        ("周末烘焙体验课报名开启", "周六下午 3 点，手作可颂体验课，限 8 人。", "published", "老板"),
        ("会员日双倍积分预告", "每月 8 号会员日，消费双倍积分。", "draft", "店员"),
    ] {
        db.execute_statement(exec(&format!(
            "INSERT INTO articles (id, tenant_id, title, summary, content, status, author, tags, created_at, updated_at) \
             VALUES ('{uuid}', 't_demo', '{t}', '{s}', '{c}', '{st}', '{a}', '[]', '{now}', '{now}')",
            uuid = uuid::Uuid::new_v4(),
            t = title.replace('\'', "''"),
            s = summary.replace('\'', "''"),
            c = format!("{summary}（正文）").replace('\'', "''"),
            st = status,
            a = author,
        )))
        .await
        .expect("seed article");
    }
    // 兼容旧库：users 增加 must_change_password 列
    let _ = db
        .execute_statement(exec("ALTER TABLE users ADD COLUMN must_change_password INTEGER DEFAULT 0"))
        .await;
    // 兼容旧库：users 增加 email 列
    let _ = db
        .execute_statement(exec("ALTER TABLE users ADD COLUMN email TEXT DEFAULT ''"))
        .await;
    // 兼容旧库：integrations 增加 OAuth 列
    let _ = db
        .execute_statement(exec("ALTER TABLE integrations ADD COLUMN oauth_provider TEXT"))
        .await;
    let _ = db
        .execute_statement(exec("ALTER TABLE integrations ADD COLUMN oauth_client_id TEXT"))
        .await;
    let _ = db
        .execute_statement(exec("ALTER TABLE integrations ADD COLUMN oauth_client_secret TEXT"))
        .await;
    let _ = db
        .execute_statement(exec("ALTER TABLE integrations ADD COLUMN oauth_token TEXT"))
        .await;
    // 兼容旧库：workflows 增加 steps 列
    let _ = db
        .execute_statement(exec("ALTER TABLE workflows ADD COLUMN steps TEXT"))
        .await;
    // 兼容旧库：workflows 增加 event 列
    let _ = db
        .execute_statement(exec("ALTER TABLE workflows ADD COLUMN event TEXT"))
        .await;
    // 兼容旧库：integrations 增加 api_key 列
    let _ = db
        .execute_statement(exec("ALTER TABLE integrations ADD COLUMN api_key TEXT"))
        .await;
    // 兼容旧库：forms 增加 descr 列
    let _ = db
        .execute_statement(exec("ALTER TABLE forms ADD COLUMN descr TEXT"))
        .await;
    // 兼容旧库：approvals 增加 payload 列（幂等迁移）
    let _ = db
        .execute_statement(exec("ALTER TABLE approvals ADD COLUMN payload TEXT"))
        .await;

    // 统一执行种子 SQL
    for sql in &seed_sqls {
        db.execute_statement(exec(sql)).await.expect("seed insert");
    }
}

pub fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
