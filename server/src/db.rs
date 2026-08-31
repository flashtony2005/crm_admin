//! 数据库连接 + 固定表结构（TD-1：SeaORM 固定表，无本体动态建模）+ 种子数据。
//!
//! 表结构是**编译期固定的 DDL**；Phase 2 引入复杂查询时再为各表补
//! SeaORM entity 结构体，Phase 1 统一资源网关直接走参数化 SQL。

use sea_orm::{ConnectOptions, Database, Statement};
use sea_orm::Value as SqlValue;
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
            thumbnail TEXT, large TEXT, width INTEGER DEFAULT 0, height INTEGER DEFAULT 0,
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
        // ── P4 商业层：会员 / 评论 / 订阅 / 邮件 / 出站 Webhook ──
        "CREATE TABLE IF NOT EXISTS members (
            id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL,
            email TEXT NOT NULL UNIQUE, name TEXT DEFAULT '',
            password_hash TEXT NOT NULL, status INTEGER DEFAULT 1,
            plan TEXT DEFAULT 'free', stripe_customer_id TEXT DEFAULT '',
            created_at TEXT NOT NULL, updated_at TEXT NOT NULL)",
        "CREATE TABLE IF NOT EXISTS comments (
            id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL,
            article_id TEXT DEFAULT '', parent_id TEXT DEFAULT '',
            author_name TEXT DEFAULT '', author_email TEXT DEFAULT '',
            member_id TEXT DEFAULT '', content TEXT DEFAULT '',
            status TEXT DEFAULT 'approved', created_at TEXT NOT NULL)",
        "CREATE TABLE IF NOT EXISTS subscribers (
            id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL,
            email TEXT NOT NULL UNIQUE, name TEXT DEFAULT '', status TEXT DEFAULT 'active',
            created_at TEXT NOT NULL)",
        "CREATE TABLE IF NOT EXISTS tiers (
            id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL,
            name TEXT DEFAULT '', slug TEXT DEFAULT '', description TEXT DEFAULT '',
            price_monthly REAL DEFAULT 0, price_yearly REAL DEFAULT 0,
            stripe_price_id TEXT DEFAULT '', features TEXT DEFAULT '[]', active INTEGER DEFAULT 1,
            created_at TEXT NOT NULL, updated_at TEXT NOT NULL)",
        "CREATE TABLE IF NOT EXISTS webhook_subscriptions (
            id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL,
            event TEXT DEFAULT '', url TEXT DEFAULT '', secret TEXT DEFAULT '',
            active INTEGER DEFAULT 1, created_at TEXT NOT NULL, updated_at TEXT NOT NULL)",
        "CREATE TABLE IF NOT EXISTS webhook_deliveries (
            id TEXT PRIMARY KEY, sub_id TEXT DEFAULT '', tenant_id TEXT NOT NULL,
            event TEXT DEFAULT '', payload TEXT DEFAULT '', status TEXT DEFAULT 'pending',
            attempts INTEGER DEFAULT 0, last_error TEXT DEFAULT '', created_at TEXT NOT NULL)",
        // ── 站点级设置（主题 / 模板 / 品牌）：KV 表，发布者在后台统一设定 ──
        "CREATE TABLE IF NOT EXISTS site_settings (
            key TEXT PRIMARY KEY, value TEXT NOT NULL DEFAULT '',
            tenant_id TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL)",
        // ── 首页区块：独立 CMS 资源表（关于 / 组织 / 实验 / Web3 四个内容块）──
        // 由后台「设置 → 站点外观 → 首页区块」维护，公开站点经 /api/public/sections 消费。
        // body 存区块结构化内容（JSON 文本），读时还原为对象。
        "CREATE TABLE IF NOT EXISTS sections (
            id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL,
            slug TEXT NOT NULL, title TEXT DEFAULT '',
            subtitle TEXT DEFAULT '', body TEXT DEFAULT '{}',
            icon TEXT DEFAULT '', sort INTEGER DEFAULT 0,
            created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
            UNIQUE(tenant_id, slug))",
        // ── 事件生态（统一事件日志：阅读/注册/评论/订阅等，统计看板的数据源）──
        "CREATE TABLE IF NOT EXISTS events (
            id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL,
            type TEXT NOT NULL, ref_id TEXT DEFAULT '', ref_key TEXT DEFAULT '',
            payload TEXT DEFAULT '', created_at TEXT NOT NULL)",
        "CREATE INDEX IF NOT EXISTS idx_events_tenant_type ON events(tenant_id, type, created_at)",
        // ── 导航与页脚链接：独立管理表（取代 site_settings.home JSON 里整块编辑的
        //    nav.links / footer.links）。grp 分 nav | footer 两组，可独立增删改/
        //    排序/启停/新窗口；UNIQUE(tenant_id, grp, href) 防同组重复目标地址。
        "CREATE TABLE IF NOT EXISTS nav_links (
            id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL,
            grp TEXT NOT NULL DEFAULT 'nav',
            label TEXT NOT NULL DEFAULT '', href TEXT NOT NULL DEFAULT '',
            target TEXT DEFAULT '',
            sort INTEGER DEFAULT 0, enabled INTEGER DEFAULT 1,
            created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
            UNIQUE(tenant_id, grp, href))",
        "CREATE INDEX IF NOT EXISTS idx_nav_links_grp ON nav_links(tenant_id, grp, sort)",
        // ── 主页置顶文章：精确编排「哪几篇上主页、顺序、置顶」
        //    slot 区分展示位（writing / series），enabled=0 临时下架不动排序。
        "CREATE TABLE IF NOT EXISTS home_pins (
            id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL,
            slot TEXT NOT NULL DEFAULT 'writing',
            article_id TEXT NOT NULL,
            sort INTEGER DEFAULT 0, enabled INTEGER DEFAULT 1,
            created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
            UNIQUE(tenant_id, slot, article_id))",
    ] {
        db.execute_statement(exec(ddl)).await.expect("建表失败");
    }

    // 站点级设置（主题 / 模板 / 品牌）：空库时写入单租户 t_demo 默认值
    let site_n = db
        .query_one_statement(exec(
            "SELECT COUNT(*) AS n FROM site_settings WHERE tenant_id = 't_demo'",
        ))
        .await
        .ok()
        .flatten()
        .map(|r| r.try_get::<i64>("", "n").unwrap_or(0))
        .unwrap_or(0);
    if site_n == 0 {
        let now = now_iso();
        for (k, v) in [
            ("theme", "paper"),
            ("template", "default"),
            ("site_title", "LightPress"),
            ("site_tagline", "专注内容的现代发布平台"),
        ] {
            let _ = db
                .execute_statement(Statement::from_sql_and_values(
                    sea_orm::DatabaseBackend::Sqlite,
                    "INSERT INTO site_settings (key, value, tenant_id, created_at, updated_at) \
                     VALUES (?, ?, 't_demo', ?, ?)",
                    vec![
                        SqlValue::String(Some(k.to_string())),
                        SqlValue::String(Some(v.to_string())),
                        SqlValue::String(Some(now.clone())),
                        SqlValue::String(Some(now.clone())),
                    ],
                ))
                .await;
        }
    }

    // 首页区块（4 个默认内容块：关于 / 组织 / 实验 / Web3）。
    // 与 users 是否已有数据无关：只要本租户尚无 sections 就补种子，
    // 保证老库升级后也能在后台「设置 → 站点外观 → 首页区块」里看到并编辑。
    {
        let sec_n = db
            .query_one_statement(exec(
                "SELECT COUNT(*) AS n FROM sections WHERE tenant_id = 't_demo'",
            ))
            .await
            .ok()
            .flatten()
            .map(|r| r.try_get::<i64>("", "n").unwrap_or(0))
            .unwrap_or(0);
        if sec_n == 0 {
            let now = now_iso();
            let about = r#"{"kicker":"KOSX.ai 社群增长操盘手","tagline":"Community Growth & Learning in Public","hero":{"title":"把连接，变成看得见的机会。","bio":"我是可可鸭，一名在 X 上持续实战的内容创作者。我围绕 AI 工具、Web3 与跨境金融，把复杂信息拆成可以直接使用的方法：单帖最高 120 万浏览，起号一周接到首单商业合作，一个月累计达到 20M+ 流量。\n\n同时，我是 KOSX.ai 社群增长操盘手，也是这个 AI 创造者网络的「第一位实习小子」。我负责 KOSX 的社群运营与增长：运营 300+ 人的「搞钱研究所」创作者社群；从 0 策划并主持「上桌！KOSX」系列 X Space——首期 8 位实战派嘉宾、2.5 小时全程满麦；同时组织线下城市面基局，把线上连接变成真实的握手。\n\n我相信：个人 IP 是这个时代的上桌券，而社群是让同频的人互相看见的引力场。","ctaLabel":"查看我的 X 实战主页","ctaHref":"https://x.com/KeKeYa88"}}"#.to_string();
            let org = r#"{"affiliation":{"eyebrow":"Current Affiliation / 当前组织","name":"KOSX.ai","logo":"https://coucouya.com/assets/kosx-logo-official.png","desc":"KOSX.ai 是面向 AI-native Builders 的创造者网络：从社群发现人才，以真实项目组织协作，并让成果连接到商业世界。","ctaLabel":"访问 KOSX.ai","ctaHref":"https://kosx.ai/"},"pillars":{"eyebrow":"KOSX.ai 创造者网络 / 把线上连接变成真实的见面","title":"把 AI 时代的创造者，组织成真正能做成事的网络。","items":[{"no":"01","title":"AI 带来新的能力"},{"no":"02","title":"人才带来创造力与判断"},{"no":"03","title":"商业让价值真正落地"},{"no":"04","title":"资本与资源让成果持续生长"}]}}"#.to_string();
            let lab = r#"{"writing":{"eyebrow":"Selected Writing / 代表文章","title":"在 X 上的实战手册","desc":"这里记录我在 AI 工具、Web3 与跨境金融领域的实战研究：从支付、外币卡到海外手机卡与 U 卡，把复杂门槛拆成可以直接使用的步骤。","ctaLabel":"了解更多我的内容","ctaHref":"https://x.com/KeKeYa88"},"series":{"eyebrow":"From 0 to 1 / 持续实验","title":"从 0 到 1","desc":"如何用 AI 工作流创造个人 IP","items":[{"no":"01","title":"治愈图文","desc":"用轻松温柔的图文，记录生活、工具与成长。","href":"https://mp.weixin.qq.com/mp/profile_ext?action=home&__biz=MzcwNzM3NjY2Mg%3D%3D#wechat_redirect"},{"no":"02","title":"书评","desc":"把阅读、思考与创作者视角放进一支支短内容里。","href":"https://weixin.qq.com/sph/AAmgiDIFxb"},{"no":"03","title":"建设中","desc":"小红书和抖音个人 IP 正在建设中。","coming":true}]}}"#.to_string();
            let web3 = r#"{"web3":{"title":"探索 Web3","desc":"把我正在使用与合作的 Web3 产品入口集中在这里。先看清产品类型、复制邀请码，再前往官方页面。","disclaimer":"以下链接包含邀请关系；你通过链接注册时，我可能获得平台奖励，但不会增加你的注册费用。","items":[{"name":"Bitget Wallet","desc":"Bitget Wallet 支持多链资产管理、交易与 Web3 应用探索；私钥由你自己掌握，请离线保管助记词。","inviteCode":"JtTDj8Se","ctaLabel":"前往官方注册链接","ctaHref":"https://web3.bitget.com/share/25AasN?inviteCode=JtTDj8Se"}]}}"#.to_string();
            let rows: Vec<(&str, &str, &str, String, i64)> = vec![
                ("about", "关于", "", about, 1),
                ("org", "组织", "", org, 2),
                ("lab", "实验", "", lab, 3),
                ("web3", "Web3", "", web3, 4),
            ];
            for (slug, title, subtitle, body, sort) in rows {
                db.execute_statement(Statement::from_sql_and_values(
                    sea_orm::DatabaseBackend::Sqlite,
                    "INSERT INTO sections (id, tenant_id, slug, title, subtitle, body, sort, created_at, updated_at) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    vec![
                        SqlValue::String(Some(uuid::Uuid::new_v4().to_string())),
                        SqlValue::String(Some("t_demo".into())),
                        SqlValue::String(Some(slug.into())),
                        SqlValue::String(Some(title.into())),
                        SqlValue::String(Some(subtitle.into())),
                        SqlValue::String(Some(body)),
                        SqlValue::BigInt(Some(sort)),
                        SqlValue::String(Some(now.clone())),
                        SqlValue::String(Some(now.clone())),
                    ],
                ))
                .await
                .expect("seed section");
            }
        }
    }

    // ── nav_links 一次性迁移：老库 site_settings.home 里嵌套的 nav.links /
    //    footer.links 导入新表（判空幂等：表非空即视为已迁移，老库升级后
    //    首次启动自动搬一次；公开站点 /api/public/nav 与后台列表均读表）。
    {
        let n = db
            .query_one_statement(exec("SELECT COUNT(*) AS n FROM nav_links WHERE tenant_id = 't_demo'"))
            .await
            .ok()
            .flatten()
            .map(|r| r.try_get::<i64>("", "n").unwrap_or(0))
            .unwrap_or(0);
        if n == 0 {
            let home_row = db
                .query_one_statement(exec(
                    "SELECT value FROM site_settings WHERE tenant_id = 't_demo' AND key = 'home'",
                ))
                .await
                .ok()
                .flatten();
            let home_json: Option<serde_json::Value> = home_row
                .and_then(|r| r.try_get::<String>("", "value").ok())
                .and_then(|v| serde_json::from_str(&v).ok());
            let now = now_iso();
            let mut inserts: Vec<Vec<SqlValue>> = Vec::new();
            // 迁移来源形如 { "nav": {"links":[{label,href,target?}...]}, "footer": {...} }
            if let Some(obj) = home_json.and_then(|v| v.as_object().cloned()) {
                for (grp, slot) in [("nav", "nav"), ("footer", "footer")] {
                    if let Some(links) = obj.get(slot).and_then(|x| x.get("links")).and_then(|x| x.as_array()) {
                        for (i, l) in links.iter().enumerate() {
                            let label = l.get("label").and_then(|x| x.as_str()).unwrap_or("").to_string();
                            let href = l.get("href").and_then(|x| x.as_str()).unwrap_or("#").to_string();
                            if label.is_empty() && href == "#" {
                                continue;
                            }
                            let target = l.get("target").and_then(|x| x.as_str()).unwrap_or("").to_string();
                            inserts.push(vec![
                                SqlValue::String(Some(uuid::Uuid::new_v4().to_string())),
                                SqlValue::String(Some("t_demo".into())),
                                SqlValue::String(Some(grp.into())),
                                SqlValue::String(Some(label)),
                                SqlValue::String(Some(href)),
                                SqlValue::String(Some(target)),
                                SqlValue::BigInt(Some(i as i64)),
                                SqlValue::BigInt(Some(1)),
                                SqlValue::String(Some(now.clone())),
                                SqlValue::String(Some(now.clone())),
                            ]);
                        }
                    }
                }
            }
            for v in inserts {
                let _ = db
                    .execute_statement(Statement::from_sql_and_values(
                        sea_orm::DatabaseBackend::Sqlite,
                        "INSERT OR IGNORE INTO nav_links \
                         (id, tenant_id, grp, label, href, target, sort, enabled, created_at, updated_at) \
                         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                        v,
                    ))
                    .await;
            }
        }
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
    let article_seed: Vec<(&str, &str, &str, &str, &str)> = vec![
        ("秋季新品：桂花栗子欧包上市", "板栗与桂花蜜的秋日限定，本周五起门店供应。", "published", "老板", "autumn-chestnut-bread"),
        ("周末烘焙体验课报名开启", "周六下午 3 点，手作可颂体验课，限 8 人。", "published", "老板", "weekend-baking-class"),
        ("会员日双倍积分预告", "每月 8 号会员日，消费双倍积分。", "draft", "店员", "member-day-double-points"),
    ];
    let daily_views: [u32; 14] = [4, 6, 3, 8, 5, 2, 7, 4, 6, 3, 5, 9, 4, 7];
    for (title, summary, status, author, slug) in &article_seed {
        let aid = uuid::Uuid::new_v4().to_string();
        db.execute_statement(exec(&format!(
            "INSERT INTO articles (id, tenant_id, title, summary, content, status, author, slug, tags, created_at, updated_at) \
             VALUES ('{aid}', 't_demo', '{t}', '{s}', '{c}', '{st}', '{a}', '{slug}', '[]', '{now}', '{now}')",
            aid = aid,
            t = title.replace('\'', "''"),
            s = summary.replace('\'', "''"),
            c = format!("{summary}（正文）").replace('\'', "''"),
            st = status,
            a = author,
            slug = slug.replace('\'', "''"),
            now = now,
        )))
        .await
        .expect("seed article");
        // 已发布文章：补一批阅读事件（最近 14 天），让统计看板有演示数据
        if *status == "published" {
            for d in 0..14usize {
                let day = (chrono::Utc::now() - chrono::Duration::days(d as i64))
                    .format("%Y-%m-%d")
                    .to_string();
                let ts = format!("{day}T10:00:00.000Z");
                for _ in 0..daily_views[d] {
                    db.execute_statement(exec(&format!(
                        "INSERT INTO events (id, tenant_id, type, ref_id, ref_key, created_at) \
                         VALUES ('{eid}', 't_demo', 'article_view', '{aid}', '{slug}', '{ts}')",
                        eid = uuid::Uuid::new_v4().to_string(),
                        aid = aid,
                        slug = slug.replace('\'', "''"),
                        ts = ts,
                    )))
                    .await
                    .expect("seed event");
                }
            }
        }
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
    // 兼容旧库：media_items 增加缩略图/大图/尺寸字段（图片处理 P3）
    let _ = db
        .execute_statement(exec("ALTER TABLE media_items ADD COLUMN thumbnail TEXT"))
        .await;
    let _ = db
        .execute_statement(exec("ALTER TABLE media_items ADD COLUMN large TEXT"))
        .await;
    let _ = db
        .execute_statement(exec("ALTER TABLE media_items ADD COLUMN width INTEGER DEFAULT 0"))
        .await;
    let _ = db
        .execute_statement(exec("ALTER TABLE media_items ADD COLUMN height INTEGER DEFAULT 0"))
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
    // 兼容旧库：articles 增加可见性（会员/付费门槛）与 locale（多语言）
    let _ = db
        .execute_statement(exec("ALTER TABLE articles ADD COLUMN visibility TEXT DEFAULT 'public'"))
        .await;
    let _ = db
        .execute_statement(exec("ALTER TABLE articles ADD COLUMN locale TEXT DEFAULT 'zh'"))
        .await;
    // 兼容旧库：articles 补齐内容列（slug/封面/发布时间/SEO）与新增功能列
    // 早期版本已建这些列，新库此处补齐；ADD COLUMN 失败被忽略，幂等安全
    let _ = db.execute_statement(exec("ALTER TABLE articles ADD COLUMN slug TEXT DEFAULT ''")).await;
    let _ = db.execute_statement(exec("ALTER TABLE articles ADD COLUMN featured_image TEXT")).await;
    let _ = db.execute_statement(exec("ALTER TABLE articles ADD COLUMN published_at TEXT")).await;
    let _ = db.execute_statement(exec("ALTER TABLE articles ADD COLUMN meta_title TEXT")).await;
    let _ = db.execute_statement(exec("ALTER TABLE articles ADD COLUMN meta_description TEXT")).await;
    let _ = db.execute_statement(exec("ALTER TABLE articles ADD COLUMN featured INTEGER DEFAULT 0")).await;
    let _ = db.execute_statement(exec("ALTER TABLE articles ADD COLUMN scheduled_at TEXT DEFAULT ''")).await;
    let _ = db.execute_statement(exec("ALTER TABLE articles ADD COLUMN canonical_url TEXT DEFAULT ''")).await;
    // 兼容旧库：articles 增加阅读量缓存计数（事件生态 / 统计看板）
    let _ = db.execute_statement(exec("ALTER TABLE articles ADD COLUMN views INTEGER DEFAULT 0")).await;
    // 兼容旧库：media_items 增加响应式图 srcset
    let _ = db.execute_statement(exec("ALTER TABLE media_items ADD COLUMN srcset TEXT")).await;
    // 兼容旧库：members 增加 stripe 客户字段（幂等）
    let _ = db
        .execute_statement(exec("ALTER TABLE members ADD COLUMN stripe_customer_id TEXT DEFAULT ''"))
        .await;

    // 统一执行种子 SQL
    for sql in &seed_sqls {
        db.execute_statement(exec(sql)).await.expect("seed insert");
    }
}

pub fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
