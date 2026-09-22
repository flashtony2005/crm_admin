//! 数据库连接 + 版本化迁移（TD-1：SeaORM 固定表，无本体动态建模）+ 种子数据。
//!
//! 表结构是**编译期固定的 DDL**；Phase 2 引入复杂查询时再为各表补
//! SeaORM entity 结构体，Phase 1 统一资源网关直接走参数化 SQL。
//!
//! Schema 演进规则：建表/补列一律走 `_migrations` 记录的版本化迁移
//! （见 `MIGRATIONS`），**已发布的 version 只追加、不修改**；
//! 种子数据保持幂等（判空即插），与迁移解耦。

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

/// ── 版本化迁移 ────────────────────────────────────────────────────────────
struct Migration {
    version: &'static str,
    name: &'static str,
    /// true：单条 SQL 失败被忽略（旧库补列——列已存在属预期）；false：失败即 panic
    lenient: bool,
    sqls: &'static [&'static str],
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: "0001_base_schema",
        name: "base tables",
        lenient: false,
        sqls: &[
        "CREATE TABLE IF NOT EXISTS tenants (
            id TEXT PRIMARY KEY, name TEXT NOT NULL, created_at TEXT NOT NULL)",
        "CREATE TABLE IF NOT EXISTS users (
            id TEXT PRIMARY KEY, username TEXT NOT NULL UNIQUE, nickname TEXT NOT NULL,
            email TEXT DEFAULT '', password_hash TEXT NOT NULL, role TEXT NOT NULL, tenant_id TEXT NOT NULL,
            status INTEGER NOT NULL DEFAULT 1, must_change_password INTEGER DEFAULT 0,
            created_at TEXT NOT NULL, updated_at TEXT NOT NULL)",
        "CREATE TABLE IF NOT EXISTS articles (
            id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL,
            title TEXT DEFAULT '', summary TEXT DEFAULT '', content TEXT DEFAULT '',
            status TEXT DEFAULT 'draft', author TEXT DEFAULT '', tags TEXT DEFAULT '[]',
            slug TEXT DEFAULT '', featured_image TEXT, published_at TEXT,
            meta_title TEXT, meta_description TEXT, featured INTEGER DEFAULT 0,
            scheduled_at TEXT DEFAULT '', canonical_url TEXT DEFAULT '',
            visibility TEXT DEFAULT 'public', locale TEXT DEFAULT 'zh',
            views INTEGER DEFAULT 0,
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
            srcset TEXT,
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
            payload TEXT, decided_at TEXT,
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
            api_key TEXT DEFAULT '',
            oauth_provider TEXT, oauth_client_id TEXT, oauth_client_secret TEXT, oauth_token TEXT,
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
        ]},
    // 旧库补列：服务存量库升级；新库列已在 0001 建齐，这些 ALTER 会因
    // "duplicate column name" 失败——lenient=true 忽略，保持幂等。
    // 注意：必须先于任何种子执行（历史版本把补列放在 users 判空 return
    // 之后，导致存量库补列永不生效、空库种子引用缺列直接 panic）。
    Migration {
        version: "0002_compat_columns",
        name: "legacy column backfill",
        lenient: true,
        sqls: &[
            "ALTER TABLE users ADD COLUMN must_change_password INTEGER DEFAULT 0",
            "ALTER TABLE users ADD COLUMN email TEXT DEFAULT ''",
            "ALTER TABLE integrations ADD COLUMN oauth_provider TEXT",
            "ALTER TABLE integrations ADD COLUMN oauth_client_id TEXT",
            "ALTER TABLE integrations ADD COLUMN oauth_client_secret TEXT",
            "ALTER TABLE integrations ADD COLUMN oauth_token TEXT",
            "ALTER TABLE media_items ADD COLUMN thumbnail TEXT",
            "ALTER TABLE media_items ADD COLUMN large TEXT",
            "ALTER TABLE media_items ADD COLUMN width INTEGER DEFAULT 0",
            "ALTER TABLE media_items ADD COLUMN height INTEGER DEFAULT 0",
            "ALTER TABLE workflows ADD COLUMN steps TEXT",
            "ALTER TABLE workflows ADD COLUMN event TEXT",
            "ALTER TABLE integrations ADD COLUMN api_key TEXT",
            "ALTER TABLE forms ADD COLUMN descr TEXT",
            "ALTER TABLE approvals ADD COLUMN payload TEXT",
            "ALTER TABLE articles ADD COLUMN visibility TEXT DEFAULT 'public'",
            "ALTER TABLE articles ADD COLUMN locale TEXT DEFAULT 'zh'",
            "ALTER TABLE articles ADD COLUMN slug TEXT DEFAULT ''",
            "ALTER TABLE articles ADD COLUMN featured_image TEXT",
            "ALTER TABLE articles ADD COLUMN published_at TEXT",
            "ALTER TABLE articles ADD COLUMN meta_title TEXT",
            "ALTER TABLE articles ADD COLUMN meta_description TEXT",
            "ALTER TABLE articles ADD COLUMN featured INTEGER DEFAULT 0",
            "ALTER TABLE articles ADD COLUMN scheduled_at TEXT DEFAULT ''",
            "ALTER TABLE articles ADD COLUMN canonical_url TEXT DEFAULT ''",
            "ALTER TABLE articles ADD COLUMN views INTEGER DEFAULT 0",
            "ALTER TABLE media_items ADD COLUMN srcset TEXT",
            "ALTER TABLE members ADD COLUMN stripe_customer_id TEXT DEFAULT ''",
        ]},
    // ── Domain Model V1：Agent-first 领域模型（12 张核心表）────────────────
    // 原则：Content 是业务事实，Site 是呈现载体；Template 决定怎么组合，
    // Theme 决定怎么呈现；Channel 是分发出口而非站点部件。
    // 与旧表（articles / pages / products / site_settings …）并存、互不干扰，
    // 旧表不在本轮重构范围。全部 IF NOT EXISTS，保证可重复执行。
    Migration {
        version: "0003_domain_model_v1",
        name: "domain model v1 (12 tables)",
        lenient: false,
        sqls: &[
        // ── Site：呈现载体。只存身份 / 域名 / 语言 / 时区 / 设置，
        //    绝不内联 template_json / theme_json / content_json；
        //    呈现绑定一律走 site_templates / site_themes 关联表。
        "CREATE TABLE IF NOT EXISTS sites (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            slug TEXT NOT NULL UNIQUE,
            domain TEXT,
            description TEXT,
            default_locale TEXT NOT NULL DEFAULT 'en-US',
            timezone TEXT NOT NULL DEFAULT 'UTC',
            status TEXT NOT NULL DEFAULT 'draft',
            settings_json TEXT NOT NULL DEFAULT '{}',
            metadata_json TEXT NOT NULL DEFAULT '{}',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL)",
        "CREATE INDEX IF NOT EXISTS idx_sites_domain ON sites(domain)",
        "CREATE INDEX IF NOT EXISTS idx_sites_status ON sites(status)",
        // ── Channel：分发出口。type ∈ website / x / wechat / telegram /
        //    newsletter / youtube / tiktok / xiaohongshu / api / custom。
        //    注意：Channel 表示「在哪里分发」，不是「网站的一部分」。
        //    UNIQUE(provider, external_id) 防同一外部渠道重复登记；
        //    SQLite 下 NULL 不参与唯一性判定，纯本地渠道不受影响。
        "CREATE TABLE IF NOT EXISTS channels (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            type TEXT NOT NULL,
            provider TEXT,
            external_id TEXT,
            url TEXT,
            config_json TEXT NOT NULL DEFAULT '{}',
            metadata_json TEXT NOT NULL DEFAULT '{}',
            status TEXT NOT NULL DEFAULT 'active',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL)",
        "CREATE INDEX IF NOT EXISTS idx_channels_type ON channels(type)",
        "CREATE INDEX IF NOT EXISTS idx_channels_status ON channels(status)",
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_channels_provider_external
            ON channels(provider, external_id)",
        // ── Content：最核心的一张。V1 不为每种类型建业务表，统一由 type 区分
        //    （profile / article / organization / project / product /
        //     experiment / link / media / event / custom）；
        //    类型专属字段全部进 data_json，以后扩展新类型无需改库。
        "CREATE TABLE IF NOT EXISTS contents (
            id TEXT PRIMARY KEY,
            type TEXT NOT NULL,
            slug TEXT,
            title TEXT,
            summary TEXT,
            status TEXT NOT NULL DEFAULT 'draft',
            locale TEXT NOT NULL DEFAULT 'en-US',
            data_json TEXT NOT NULL DEFAULT '{}',
            metadata_json TEXT NOT NULL DEFAULT '{}',
            author_id TEXT,
            version INTEGER NOT NULL DEFAULT 1,
            published_at TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL)",
        "CREATE INDEX IF NOT EXISTS idx_contents_type ON contents(type)",
        "CREATE INDEX IF NOT EXISTS idx_contents_status ON contents(status)",
        "CREATE INDEX IF NOT EXISTS idx_contents_slug ON contents(slug)",
        "CREATE INDEX IF NOT EXISTS idx_contents_locale ON contents(locale)",
        "CREATE INDEX IF NOT EXISTS idx_contents_published ON contents(published_at)",
        // ── Context：Agent 的语义上下文，不等价于传统 Tag。
        //    自身可携带 audience / intent / industry / region 等语义，
        //    供 Analyst 做交叉查询而非简单 tag 匹配；parent_id 支持层级。
        "CREATE TABLE IF NOT EXISTS contexts (
            id TEXT PRIMARY KEY,
            type TEXT NOT NULL,
            name TEXT NOT NULL,
            slug TEXT NOT NULL UNIQUE,
            description TEXT,
            data_json TEXT NOT NULL DEFAULT '{}',
            metadata_json TEXT NOT NULL DEFAULT '{}',
            parent_id TEXT,
            status TEXT NOT NULL DEFAULT 'active',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL)",
        "CREATE INDEX IF NOT EXISTS idx_contexts_type ON contexts(type)",
        "CREATE INDEX IF NOT EXISTS idx_contexts_parent ON contexts(parent_id)",
        "CREATE INDEX IF NOT EXISTS idx_contexts_status ON contexts(status)",
        // ── Content ↔ Context：多对多 + 角色 / 权重。
        //    role ∈ primary / secondary / audience / topic / intent
        "CREATE TABLE IF NOT EXISTS content_contexts (
            content_id TEXT NOT NULL,
            context_id TEXT NOT NULL,
            role TEXT NOT NULL DEFAULT 'primary',
            weight REAL NOT NULL DEFAULT 1.0,
            created_at TEXT NOT NULL,
            PRIMARY KEY (content_id, context_id),
            FOREIGN KEY (content_id) REFERENCES contents(id) ON DELETE CASCADE,
            FOREIGN KEY (context_id) REFERENCES contexts(id) ON DELETE CASCADE)",
        "CREATE INDEX IF NOT EXISTS idx_content_contexts_context
            ON content_contexts(context_id)",
        // ── Content ↔ Channel：同一 Content 可进入多个渠道
        //    （Article A → Website + X + Newsletter），
        //    是后续 Producer / Executor 分发链路的关键基础。
        "CREATE TABLE IF NOT EXISTS content_channels (
            content_id TEXT NOT NULL,
            channel_id TEXT NOT NULL,
            external_id TEXT,
            external_url TEXT,
            status TEXT NOT NULL DEFAULT 'active',
            published_at TEXT,
            metadata_json TEXT NOT NULL DEFAULT '{}',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            PRIMARY KEY (content_id, channel_id),
            FOREIGN KEY (content_id) REFERENCES contents(id) ON DELETE CASCADE,
            FOREIGN KEY (channel_id) REFERENCES channels(id) ON DELETE CASCADE)",
        "CREATE INDEX IF NOT EXISTS idx_content_channels_channel
            ON content_channels(channel_id)",
        "CREATE INDEX IF NOT EXISTS idx_content_channels_status
            ON content_channels(status)",
        // ── Template：决定「怎么组合」。type ∈ site / home / article /
        //    listing / project / product / landing / custom
        "CREATE TABLE IF NOT EXISTS templates (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            slug TEXT NOT NULL UNIQUE,
            type TEXT NOT NULL,
            description TEXT,
            status TEXT NOT NULL DEFAULT 'draft',
            metadata_json TEXT NOT NULL DEFAULT '{}',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL)",
        "CREATE INDEX IF NOT EXISTS idx_templates_type ON templates(type)",
        "CREATE INDEX IF NOT EXISTS idx_templates_status ON templates(status)",
        // ── TemplateVersion：真正的页面结构（definition_json，schema
        //    admin.template.v1）。V1 刻意不建 components / component_versions
        //    / component_props / component_bindings 等表——否则等于重新发明
        //    Webflow / Elementor。section + component + binding + props
        //    直接声明在 definition_json 内即可；等真出现「多模板共用组件 /
        //    组件市场 / 第三方组件」需求再抽 Component Registry。
        "CREATE TABLE IF NOT EXISTS template_versions (
            id TEXT PRIMARY KEY,
            template_id TEXT NOT NULL,
            version INTEGER NOT NULL,
            definition_json TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'draft',
            created_at TEXT NOT NULL,
            UNIQUE(template_id, version),
            FOREIGN KEY (template_id) REFERENCES templates(id) ON DELETE CASCADE)",
        "CREATE INDEX IF NOT EXISTS idx_template_versions_template
            ON template_versions(template_id)",
        // ── Site ↔ Template：路由到模板的绑定（/ → PersonalBrandHome）。
        "CREATE TABLE IF NOT EXISTS site_templates (
            site_id TEXT NOT NULL,
            template_id TEXT NOT NULL,
            route TEXT NOT NULL,
            is_default INTEGER NOT NULL DEFAULT 0,
            config_json TEXT NOT NULL DEFAULT '{}',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            PRIMARY KEY (site_id, template_id, route),
            FOREIGN KEY (site_id) REFERENCES sites(id) ON DELETE CASCADE,
            FOREIGN KEY (template_id) REFERENCES templates(id) ON DELETE CASCADE)",
        "CREATE INDEX IF NOT EXISTS idx_site_templates_route
            ON site_templates(site_id, route)",
        // ── Theme：决定「怎么呈现」。这里只放元数据，
        //    真正的 tokens / components 在 theme_versions。
        "CREATE TABLE IF NOT EXISTS themes (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            slug TEXT NOT NULL UNIQUE,
            description TEXT,
            status TEXT NOT NULL DEFAULT 'draft',
            metadata_json TEXT NOT NULL DEFAULT '{}',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL)",
        "CREATE INDEX IF NOT EXISTS idx_themes_status ON themes(status)",
        // ── ThemeVersion：tokens_json（color / typography / spacing /
        //    layout / radius）+ components_json，供外部 Renderer 消费。
        "CREATE TABLE IF NOT EXISTS theme_versions (
            id TEXT PRIMARY KEY,
            theme_id TEXT NOT NULL,
            version INTEGER NOT NULL,
            tokens_json TEXT NOT NULL,
            components_json TEXT NOT NULL DEFAULT '{}',
            status TEXT NOT NULL DEFAULT 'draft',
            created_at TEXT NOT NULL,
            UNIQUE(theme_id, version),
            FOREIGN KEY (theme_id) REFERENCES themes(id) ON DELETE CASCADE)",
        "CREATE INDEX IF NOT EXISTS idx_theme_versions_theme
            ON theme_versions(theme_id)",
        // ── Site ↔ Theme：一个站点可挂多套主题，is_default 标记生效者。
        "CREATE TABLE IF NOT EXISTS site_themes (
            site_id TEXT NOT NULL,
            theme_id TEXT NOT NULL,
            is_default INTEGER NOT NULL DEFAULT 0,
            config_json TEXT NOT NULL DEFAULT '{}',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            PRIMARY KEY (site_id, theme_id),
            FOREIGN KEY (site_id) REFERENCES sites(id) ON DELETE CASCADE,
            FOREIGN KEY (theme_id) REFERENCES themes(id) ON DELETE CASCADE)",
        "CREATE INDEX IF NOT EXISTS idx_site_themes_default
            ON site_themes(site_id, is_default)",
        ]},
    // ── 创作者社区 P0：邀请制注册 + 内容售卖分级 ────────────────────────────
    // invite_codes：邀请码（额度制，好友凭码注册后 used+1 核销）。
    // members.invited_by：注册来源（邀请人 member id；非空 = 邀请加入）。
    // articles.paid_level：售卖分级 0 公开 / 1 订阅会员 / 2 积分买断 / 3 邀请专享，
    //   优先于旧 visibility 字段；price_points 为积分买断单价（P1 积分商城消费）。
    // lenient=true：旧库重放时 ALTER 重复列失败属预期，静默忽略保幂等。
    Migration {
        version: "0004_community_paywall",
        name: "invite codes + article paywall levels",
        lenient: true,
        sqls: &[
        "CREATE TABLE IF NOT EXISTS invite_codes (
            id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL,
            code TEXT NOT NULL, owner_member_id TEXT DEFAULT '',
            quota INTEGER DEFAULT 1, used INTEGER DEFAULT 0,
            expires_at TEXT DEFAULT '', enabled INTEGER DEFAULT 1,
            created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
            UNIQUE(tenant_id, code))",
        "CREATE INDEX IF NOT EXISTS idx_invite_codes_owner ON invite_codes(tenant_id, owner_member_id)",
        "ALTER TABLE members ADD COLUMN invited_by TEXT DEFAULT ''",
        "ALTER TABLE articles ADD COLUMN paid_level INTEGER DEFAULT 0",
        "ALTER TABLE articles ADD COLUMN price_points INTEGER DEFAULT 0",
        ]},
    // ── 创作者社区 P1：积分 / 订单（人工确认收款）/ 兑码 ─────────────────────
    // points_ledger：积分流水（唯一事实源）。余额 = SUM(delta)；balance_after
    //   为写入时快照，便于对账。reason ∈ signin/purchase/redeem/recharge/
    //   invite_reward/adjust；ref_id：purchase=文章id、invite_reward=新会员id、
    //   recharge/redeem=订单号或兑码id。
    // orders：收款单一入口。channel 预留 manual/code/wechat（P1 仅人工确认）；
    //   status pending→paid（确认收款时原子 UPDATE ... WHERE status='pending'
    //   防重复发货）；biz_type points_recharge 充积分 / plan 开通订阅。
    // redeem_codes：一次性兑码（kind points=充积分 / plan_days=会员天数）。
    // members.plan_expires_at：订阅到期时间（空=不限期）；付费墙判定
    //   plan != 'free' 且（未设过期 或 未到期）。
    Migration {
        version: "0005_points_orders",
        name: "points ledger + orders + redeem codes",
        lenient: true,
        sqls: &[
        "CREATE TABLE IF NOT EXISTS points_ledger (
            id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL,
            member_id TEXT NOT NULL, delta INTEGER NOT NULL,
            balance_after INTEGER NOT NULL DEFAULT 0,
            reason TEXT NOT NULL DEFAULT 'adjust',
            ref_id TEXT DEFAULT '', note TEXT DEFAULT '',
            created_at TEXT NOT NULL)",
        "CREATE INDEX IF NOT EXISTS idx_points_ledger_member ON points_ledger(tenant_id, member_id, created_at)",
        "CREATE TABLE IF NOT EXISTS orders (
            id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL,
            order_no TEXT NOT NULL, member_id TEXT NOT NULL,
            biz_type TEXT NOT NULL DEFAULT 'points_recharge',
            tier_id TEXT DEFAULT '', points INTEGER DEFAULT 0, plan_days INTEGER DEFAULT 0,
            amount_cents INTEGER DEFAULT 0, channel TEXT DEFAULT 'manual',
            status TEXT DEFAULT 'pending', ref_no TEXT DEFAULT '',
            paid_at TEXT DEFAULT '', created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
            UNIQUE(tenant_id, order_no))",
        "CREATE INDEX IF NOT EXISTS idx_orders_member ON orders(tenant_id, member_id, created_at)",
        "CREATE TABLE IF NOT EXISTS redeem_codes (
            id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL,
            code TEXT NOT NULL, kind TEXT NOT NULL DEFAULT 'points',
            value INTEGER NOT NULL DEFAULT 0,
            status TEXT NOT NULL DEFAULT 'unused',
            used_by TEXT DEFAULT '', used_at TEXT DEFAULT '',
            created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
            UNIQUE(tenant_id, code))",
        "ALTER TABLE members ADD COLUMN plan_expires_at TEXT DEFAULT ''",
        ]},

    // ── 0006_hardening（F1/F2 配套）────────────────────────────────
    // 1) 会员邮箱唯一：先清理历史重复（保留同邮箱中 id 最小者），再建部分唯一索引，
    //    空邮箱不参与（大量会员未填邮箱，纳入会误伤）。
    // 2) 社区表补索引：订单按状态/渠道筛选、积分流水按会员/原因查重、兑码核销。
    // 3) order_fulfill_log：发货 outbox。fulfill_order 的每个动作先占位再执行，
    //    中途失败留痕可重试，杜绝「已扣款未发货」。
    Migration {
        version: "0006_hardening",
        name: "0006_hardening",
        lenient: true,
        sqls: &[
        "DELETE FROM members WHERE email <> '' AND id NOT IN (\
            SELECT MIN(id) FROM members WHERE email <> '' GROUP BY tenant_id, email)",
        "CREATE UNIQUE INDEX IF NOT EXISTS uq_members_email ON members(tenant_id, email) WHERE email <> ''",
        "CREATE INDEX IF NOT EXISTS idx_orders_status ON orders(tenant_id, status, created_at)",
        "CREATE INDEX IF NOT EXISTS idx_orders_channel ON orders(tenant_id, channel, status)",
        "CREATE INDEX IF NOT EXISTS idx_ledger_member ON points_ledger(tenant_id, member_id, created_at)",
        "CREATE INDEX IF NOT EXISTS idx_ledger_reason ON points_ledger(tenant_id, reason, ref_id)",
        "CREATE INDEX IF NOT EXISTS idx_invite_codes_code ON invite_codes(tenant_id, code)",
        "CREATE INDEX IF NOT EXISTS idx_redeem_codes_status ON redeem_codes(tenant_id, status)",
        "CREATE INDEX IF NOT EXISTS idx_members_plan_expiry ON members(tenant_id, plan_expires_at)",
        "CREATE INDEX IF NOT EXISTS idx_members_invited_by ON members(tenant_id, invited_by)",
        "CREATE TABLE IF NOT EXISTS order_fulfill_log (
            id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL,
            order_id TEXT NOT NULL, order_no TEXT NOT NULL,
            stage_key TEXT NOT NULL, stage TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'pending',
            attempts INTEGER NOT NULL DEFAULT 0,
            last_error TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
            UNIQUE(tenant_id, order_id, stage_key))",
        "CREATE INDEX IF NOT EXISTS idx_fulfill_pending ON order_fulfill_log(tenant_id, status)",
        // F4：users.token_version 用于作废旧 token（改密 / 管理员踢下线）
        "ALTER TABLE users ADD COLUMN token_version INTEGER NOT NULL DEFAULT 1",
        ],
    },
    // ── 0007_process_state（G2 配套）──────────────────────────────
    // 1) login_locks：登录失败计数/锁定落库（原为进程内 Mutex，重启即清、多实例不共享）。
    // 2) webhook_deliveries.updated_at：投递重试的退避基准（每次尝试刷新）。
    Migration {
        version: "0007_process_state",
        name: "0007_process_state",
        lenient: true,
        sqls: &[
        "CREATE TABLE IF NOT EXISTS login_locks (\
            username TEXT PRIMARY KEY, fails INTEGER NOT NULL DEFAULT 0, \
            locked_until TEXT NOT NULL DEFAULT '', updated_at TEXT NOT NULL)",
        "ALTER TABLE webhook_deliveries ADD COLUMN updated_at TEXT DEFAULT ''",
        ],
    },
    // ── 0008_audit（P2 配套）────────────────────────────────────
    // audit_log：/api 写操作审计（谁、何时、对哪个路径、结果状态与耗时）。
    Migration {
        version: "0008_audit",
        name: "0008_audit",
        lenient: true,
        sqls: &[
        "CREATE TABLE IF NOT EXISTS audit_log (\
            id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL, \
            user_id TEXT DEFAULT '', username TEXT DEFAULT '', \
            method TEXT DEFAULT '', path TEXT DEFAULT '', \
            status INTEGER DEFAULT 0, duration_ms INTEGER DEFAULT 0, \
            request_id TEXT DEFAULT '', created_at TEXT NOT NULL)",
        "CREATE INDEX IF NOT EXISTS idx_audit_created ON audit_log(tenant_id, created_at)",
        ],
    },
    // ── 0009_article_dispatches（域模型收敛：一稿多投下沉）────────────
    // article_dispatches：一篇文章分发到多个渠道的记录（原域模型 content_channels 的下沉版）。
    // channel 为渠道标识文本（如 website / x / wechat），不做外键——渠道定义随域模型一并移除。
    Migration {
        version: "0009_article_dispatches",
        name: "0009_article_dispatches",
        lenient: true,
        sqls: &[
        "CREATE TABLE IF NOT EXISTS article_dispatches (\
            id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL, \
            article_id TEXT NOT NULL, channel TEXT NOT NULL DEFAULT '', \
            external_url TEXT DEFAULT '', status TEXT NOT NULL DEFAULT 'active', \
            dispatched_at TEXT DEFAULT '', note TEXT DEFAULT '', \
            created_at TEXT NOT NULL, updated_at TEXT NOT NULL)",
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_dispatch_unique ON article_dispatches(tenant_id, article_id, channel)",
        "CREATE INDEX IF NOT EXISTS idx_dispatch_article ON article_dispatches(tenant_id, article_id)",
        ],
    },
    // ── 0010_webhook_events（P0-3 配套：入站 Webhook 事件落库）────────
    // 入站事件的单一事实源：既做 event_id 去重（Stripe 会重投），
    // 也做失败重放与日后对账的依据。此前事件**处理完即丢**，
    // 所以既无法重放，也无从知道"Stripe 说投过但库里没变化"。
    // status: received（已入库待处理）/ processed / pending_apply（缺订阅状态机，待阶段 1）/
    //         ignored（未识别类型）/ failed（处理失败，Stripe 会重投）。
    Migration {
        version: "0010_webhook_events",
        name: "0010_webhook_events",
        lenient: true,
        sqls: &[
        "CREATE TABLE IF NOT EXISTS webhook_events (\
            id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL, \
            provider TEXT NOT NULL DEFAULT '', event_id TEXT NOT NULL DEFAULT '', \
            type TEXT NOT NULL DEFAULT '', payload TEXT NOT NULL DEFAULT '', \
            status TEXT NOT NULL DEFAULT 'received', attempts INTEGER NOT NULL DEFAULT 0, \
            last_error TEXT DEFAULT '', applies_at TEXT DEFAULT '', \
            created_at TEXT NOT NULL, updated_at TEXT NOT NULL)",
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_webhook_event_unique ON webhook_events(tenant_id, provider, event_id)",
        "CREATE INDEX IF NOT EXISTS idx_webhook_event_status ON webhook_events(tenant_id, status, created_at)",
        ],
    },
    // ── 0011_article_kind（段 3：问题页面体系的承载列）──────────────────
    // 问题页不是「同一种文章的另一种排版」，而是**另一种内容意图**：一个页面
    // 只回答一个问题。不给类型判据，就只能靠标签名做 LIKE 子串匹配
    // （`articles?tag=` 现在正是这么干的）—— 名字一改就断链，且 20+ 条问题页
    // 混在文章流里，无法被 sitemap / 列表 / 导航分别对待。
    // 默认 'post' 保证既有行语义不变（零回填）。
    Migration {
        version: "0011_article_kind",
        name: "0011_article_kind",
        lenient: true,
        sqls: &[
        "ALTER TABLE articles ADD COLUMN kind TEXT NOT NULL DEFAULT 'post'",
        "CREATE INDEX IF NOT EXISTS idx_articles_kind ON articles(tenant_id, kind, status)",
        ],
    },
    // ── 0012_seo_redirects（P0-1：重定向表 + 404 监控）────────────────
    // 对标 Rank Math 免费版的重定向管理。为什么现在是刚需：迁移 0011 刚加了
    // kind 列（文章能在 /post/ 与 /problems/ 之间迁移），问题页正在铺开，
    // sitemap 刚从 7 条修到 12 条 —— **URL 面本身在快速变化**。
    // 没有重定向表，改一次 slug 就是一次不报警、查不到的资产流失。
    // not_found_log 的 path 建唯一索引：同一条死链只累加 hits，
    // 避免后台列表被同一条链接刷满（采用方向由前端主动上报，量会大）。
    Migration {
        version: "0012_seo_redirects",
        name: "0012_seo_redirects",
        lenient: true,
        sqls: &[
        "CREATE TABLE IF NOT EXISTS redirects (
            id TEXT PRIMARY KEY,
            tenant_id TEXT NOT NULL DEFAULT 't_demo',
            from_path TEXT NOT NULL,
            to_path TEXT NOT NULL,
            code INTEGER NOT NULL DEFAULT 301,
            note TEXT NOT NULL DEFAULT '',
            hits INTEGER NOT NULL DEFAULT 0,
            enabled INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL DEFAULT '',
            updated_at TEXT NOT NULL DEFAULT '')",
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_redirect_unique ON redirects(tenant_id, from_path)",
        "CREATE INDEX IF NOT EXISTS idx_redirect_enabled ON redirects(tenant_id, enabled)",
        "CREATE TABLE IF NOT EXISTS not_found_log (
            id TEXT PRIMARY KEY,
            tenant_id TEXT NOT NULL DEFAULT 't_demo',
            path TEXT NOT NULL,
            referer TEXT NOT NULL DEFAULT '',
            ua TEXT NOT NULL DEFAULT '',
            hits INTEGER NOT NULL DEFAULT 0,
            last_seen TEXT NOT NULL DEFAULT '',
            resolved INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT '',
            updated_at TEXT NOT NULL DEFAULT '')",
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_nf_unique ON not_found_log(tenant_id, path)",
        "CREATE INDEX IF NOT EXISTS idx_nf_resolved ON not_found_log(tenant_id, resolved, hits)",
        ],
    },
    // ── 0013_community_min（P0-2：社区最小闭环 —— 反应 / 举报 / 通知）────
    // 对标 FluentCommunity 的最小可用集：反应 + 楼中楼 + @提及 + 举报。
    // 楼中楼的 parent_id 列已在 0004 就位（写入/读取都已支持），本次补的是
    // 「下半场」：没有反应就没有「读者对读者」，没有通知就不会有人回来看。
    //
    // comment_reactions / comment_reports 的唯一索引都带 member_id：
    // 防重复投票与防重复举报是**库层保证**，不靠应用层先查后写
    // （后者在并发下必然漏。且该项目已有「唯一冲突被 Turso 吞成 Ok(0)」的前车之鉴）。
    Migration {
        version: "0013_community_min",
        name: "0013_community_min",
        lenient: true,
        sqls: &[
        "CREATE TABLE IF NOT EXISTS comment_reactions (
            id TEXT PRIMARY KEY,
            tenant_id TEXT NOT NULL DEFAULT 't_demo',
            comment_id TEXT NOT NULL,
            member_id TEXT NOT NULL,
            kind TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT '',
            updated_at TEXT NOT NULL DEFAULT '')",
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_creaction_unique ON comment_reactions(tenant_id, comment_id, member_id, kind)",
        "CREATE INDEX IF NOT EXISTS idx_creaction_comment ON comment_reactions(tenant_id, comment_id)",
        "CREATE TABLE IF NOT EXISTS comment_reports (
            id TEXT PRIMARY KEY,
            tenant_id TEXT NOT NULL DEFAULT 't_demo',
            comment_id TEXT NOT NULL,
            member_id TEXT NOT NULL,
            reason TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL DEFAULT '',
            updated_at TEXT NOT NULL DEFAULT '')",
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_creport_unique ON comment_reports(tenant_id, comment_id, member_id)",
        "CREATE INDEX IF NOT EXISTS idx_creport_comment ON comment_reports(tenant_id, comment_id)",
        "CREATE TABLE IF NOT EXISTS notifications (
            id TEXT PRIMARY KEY,
            tenant_id TEXT NOT NULL DEFAULT 't_demo',
            member_id TEXT NOT NULL,
            kind TEXT NOT NULL,
            title TEXT NOT NULL DEFAULT '',
            body TEXT NOT NULL DEFAULT '',
            ref_kind TEXT NOT NULL DEFAULT '',
            ref_id TEXT NOT NULL DEFAULT '',
            read_at TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL DEFAULT '',
            updated_at TEXT NOT NULL DEFAULT '')",
        "CREATE INDEX IF NOT EXISTS idx_notif_member ON notifications(tenant_id, member_id, created_at)",
        ],
    },
    Migration {
        // P0-4：Smart Links。三张表各司其职 ——
        // smart_links 是「我要发出去的链接」，link_clicks 是「谁点过」，
        // member_tags 是「点过之后这个人被打上了什么」。
        // 计数与明细分开：列表页只读 smart_links 的两个计数器，
        // 明细表可以随体积增长清理而不影响任何前台数字。
        version: "0014_smart_links",
        name: "0014_smart_links",
        lenient: true,
        sqls: &[
        "CREATE TABLE IF NOT EXISTS smart_links (
            id TEXT PRIMARY KEY,
            tenant_id TEXT NOT NULL DEFAULT 't_demo',
            token TEXT NOT NULL,
            url TEXT NOT NULL DEFAULT '',
            label TEXT NOT NULL DEFAULT '',
            tags TEXT NOT NULL DEFAULT '',
            enabled INTEGER NOT NULL DEFAULT 1,
            clicks INTEGER NOT NULL DEFAULT 0,
            prefetch INTEGER NOT NULL DEFAULT 0,
            last_click_at TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL DEFAULT '',
            updated_at TEXT NOT NULL DEFAULT '')",
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_smartlink_token ON smart_links(tenant_id, token)",
        "CREATE TABLE IF NOT EXISTS link_clicks (
            id TEXT PRIMARY KEY,
            tenant_id TEXT NOT NULL DEFAULT 't_demo',
            link_id TEXT NOT NULL,
            member_id TEXT NOT NULL DEFAULT '',
            referer TEXT NOT NULL DEFAULT '',
            ua TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL DEFAULT '',
            updated_at TEXT NOT NULL DEFAULT '')",
        "CREATE INDEX IF NOT EXISTS idx_linkclick_link ON link_clicks(tenant_id, link_id, created_at)",
        "CREATE TABLE IF NOT EXISTS member_tags (
            id TEXT PRIMARY KEY,
            tenant_id TEXT NOT NULL DEFAULT 't_demo',
            member_id TEXT NOT NULL,
            tag TEXT NOT NULL,
            source TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL DEFAULT '',
            updated_at TEXT NOT NULL DEFAULT '')",
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_membertag_unique ON member_tags(tenant_id, member_id, tag)",
        ],
    },
    // ── 0015_attribution（分析归因：把匿名访客与注册会员串成一条线）────────
    // 「哪篇文章带来几个会员」在数据上一直不可回答 —— 缺的不是事件量，
    // 而是**身份连续性**：events 有 type 有 ref_id，却没有任何匿名访客标识；
    // members 是注册之后的身份。两者之间断链，于是内容与转化的因果关系
    // 只能靠感觉。补一个 visitor_id（前端 localStorage 里的持久 uuid），
    // 这条线就通了，而且**零新增事实表**。
    //
    // 为什么不另建一张 visits 表：events 本身就是统一事件日志，
    // 浏览器埋点与后台统计都从它读。同一事实存两处，迟早对不上 ——
    // 而且「浏览」与「点赞/购买」的先后顺序恰恰是归因最需要的，
    // 分表存反而让时序拼接变难。
    //
    // 为什么要把归因结果**固化**到 members 上，而不是每次实时反查 events：
    // 1) 归因只该算一次 —— 首次触达是既成事实，不该随事件保留策略/清理而变；
    // 2) 事件量大后实时 JOIN 代价高，而会员表天生就是「归因结果」的归属地；
    // 3) 事件表若将来做归档，历史归因不会一起丢。
    Migration {
        version: "0015_attribution",
        name: "0015_attribution",
        lenient: true,
        sqls: &[
        "ALTER TABLE events ADD COLUMN visitor_id TEXT NOT NULL DEFAULT ''",
        "CREATE INDEX IF NOT EXISTS idx_events_visitor ON events(tenant_id, visitor_id, type, created_at)",
        "ALTER TABLE members ADD COLUMN visitor_id TEXT NOT NULL DEFAULT ''",
        "ALTER TABLE members ADD COLUMN first_touch_article_id TEXT NOT NULL DEFAULT ''",
        "ALTER TABLE members ADD COLUMN first_touch_at TEXT NOT NULL DEFAULT ''",
        "CREATE INDEX IF NOT EXISTS idx_members_visitor ON members(tenant_id, visitor_id)",
        "CREATE INDEX IF NOT EXISTS idx_members_first_touch ON members(tenant_id, first_touch_article_id)",
        ],
    },
];

/// 迁移执行器：确保 `_migrations` 记录表存在 → 逐版本判重 → 执行 → 记录。
/// 已应用版本重启时直接跳过；新增迁移只需在 `MIGRATIONS` 末尾追加。
async fn run_migrations(db: &CmsDb) {
    db.execute_statement(exec(
        "CREATE TABLE IF NOT EXISTS _migrations (
            version TEXT PRIMARY KEY, name TEXT DEFAULT '', applied_at TEXT NOT NULL)",
    ))
    .await
    .expect("建迁移记录表失败");

    for m in MIGRATIONS {
        let applied = db
            .query_one_statement(exec(&format!(
                "SELECT version FROM _migrations WHERE version = '{}'",
                m.version
            )))
            .await
            .ok()
            .flatten()
            .is_some();
        if applied {
            continue;
        }
        for sql in m.sqls {
            if m.lenient {
                // P2：只吞「已存在/重复」类预期错误（幂等重放）；其他失败大声告警，
                // 避免「版本已记录、列实际缺失」的结构漂移被静默吞掉。
                if let Err(e) = db.execute_statement(exec(sql)).await {
                    let msg = format!("{e}");
                    let expected =
                        msg.contains("already exists") || msg.contains("duplicate column");
                    if !expected {
                        eprintln!(
                            "[db] 迁移 {} 非预期失败（已跳过该条，请人工核对）：{msg}\n  sql: {}",
                            m.version, sql
                        );
                    }
                }
            } else {
                db.execute_statement(exec(sql)).await.expect("迁移执行失败");
            }
        }
        db.execute_statement(exec(&format!(
            "INSERT INTO _migrations (version, name, applied_at) VALUES ('{}', '{}', '{}')",
            m.version,
            m.name,
            now_iso()
        )))
        .await
        .expect("记录迁移版本失败");
        eprintln!("[db] migration applied: {} ({})", m.version, m.name);
    }
}

/// 启动时执行：版本化迁移（建表/补列）+ 幂等种子。
pub async fn bootstrap(db: &CmsDb) {
    run_migrations(db).await;

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
            let about = r#"{"kicker":"Demo 创作者","tagline":"Build in Public","hero":{"title":"把内容，变成可持续的事业。","bio":"这是一份演示用的创作者简介：在此介绍你的创作方向、代表成绩与正在经营的业务。安装后可在后台「设置 → 站点外观 → 首页区块」中替换为你的真实内容。\n\n支持多段文本：讲清楚你是谁、做什么、为什么值得关注。","ctaLabel":"查看我的主页","ctaHref":""}}"#.to_string();
            let org = r#"{"affiliation":{"eyebrow":"Current Affiliation / 当前组织","name":"Demo Network","logo":"","desc":"在此填写你当前所属的组织或网络：它是什么、你扮演的角色、以及它如何与你的创作产生协同。","ctaLabel":"访问组织主页","ctaHref":""},"pillars":{"eyebrow":"为什么关注我 / 把关注变成价值","title":"四个维度展示你的独特价值。","items":[{"no":"01","title":"专业能力"},{"no":"02","title":"内容产出"},{"no":"03","title":"社群连接"},{"no":"04","title":"商业转化"}]}}"#.to_string();
            let lab = r#"{"writing":{"eyebrow":"Selected Writing / 代表文章","title":"我的实战手册","desc":"在此汇总你的代表作品与系列文章：选一个你最有发言权的领域，把复杂门槛拆成可以直接使用的步骤。","ctaLabel":"了解更多我的内容","ctaHref":""},"series":{"eyebrow":"From 0 to 1 / 持续实验","title":"从 0 到 1","desc":"记录你正在进行的实验与项目","items":[{"no":"01","title":"实验一","desc":"正在进行的第一个项目简述。","coming":true},{"no":"02","title":"实验二","desc":"正在进行的第二个项目简述。","coming":true},{"no":"03","title":"建设中","desc":"更多实验筹备中。","coming":true}]}}"#.to_string();
            let web3 = r#"{"web3":{"title":"探索 Web3","desc":"把你正在使用与合作的 Web3 产品入口集中在这里。先看清产品类型、复制邀请码，再前往官方页面。","disclaimer":"以下链接包含邀请关系；访客通过链接注册时，你可能获得平台奖励，但不会增加其注册费用。","items":[{"name":"Demo Wallet","desc":"示例产品：描述该钱包或平台的核心能力与适用人群；提醒用户自行离线保管助记词。","inviteCode":"DEMO1234","ctaLabel":"前往官方页面","ctaHref":""}]}}"#.to_string();
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
    // （旧库补列已收拢进迁移 0002_compat_columns，见 MIGRATIONS；
    //   历史上这段 ALTER 位于 users 判空 return 之后，存量库永远执行不到，
    //   空库则因种子先于补列执行而 panic——已由迁移前置修复。）

    // 统一执行种子 SQL
    for sql in &seed_sqls {
        db.execute_statement(exec(sql)).await.expect("seed insert");
    }
}

pub fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
