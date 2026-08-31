//! Domain Model V1 · SeaORM Entity 层（12 张核心表）
//!
//! 对应 `db::MIGRATIONS` 里的 `0003_domain_model_v1`。
//!
//! 设计约束（与迁移 DDL 严格对齐，改表必须先改迁移）：
//! - 所有时间戳为 `TEXT`（ISO-8601 字符串），故 entity 侧用 `String`，
//!   与既有 `articles` / `sections` 等表的写法保持一致。
//! - JSON 列（`*_json`）在 entity 侧是**原始字符串**，不在此层反序列化；
//!   解析与校验属于 Domain Service 的职责，避免把 schema 知识泄漏进持久层。
//! - Content 的类型专属字段一律在 `data_json`，V1 不为每种类型建表。
//!
//! 关系只定义「直接可表达」的一对多 / 多对一。
//! 跨联结表的多对多（Content ↔ Context / Content ↔ Channel、
//! Site ↔ Template / Site ↔ Theme）由 Domain Service 显式两跳查询，
//! 不在此层用 `Related::via` 隐式展开——联结表还带 role / weight /
//! route / is_default 等自己的业务字段，隐式展开会把它们藏起来。

pub mod channel;
pub mod content;
pub mod content_channel;
pub mod content_context;
pub mod context;
pub mod site;
pub mod site_template;
pub mod site_theme;
pub mod template;
pub mod template_version;
pub mod theme;
pub mod theme_version;

/// Content 类型（V1 白名单）。扩展新类型只改这里 + 文档，不动数据库。
pub const CONTENT_TYPES: &[&str] = &[
    "profile",
    "article",
    "organization",
    "project",
    "product",
    "experiment",
    "link",
    "media",
    "event",
    "custom",
];

/// Channel 类型（V1 白名单）。表示「在哪里分发」，不是「网站的一部分」。
pub const CHANNEL_TYPES: &[&str] = &[
    "website",
    "x",
    "wechat",
    "telegram",
    "newsletter",
    "youtube",
    "tiktok",
    "xiaohongshu",
    "api",
    "custom",
];

/// Context 关系角色。Context 不等价于 Tag——role 表达的是语义关系。
pub const CONTEXT_ROLES: &[&str] = &["primary", "secondary", "audience", "topic", "intent"];

/// Template 类型。决定「怎么组合」。
pub const TEMPLATE_TYPES: &[&str] = &[
    "site",
    "home",
    "article",
    "listing",
    "project",
    "product",
    "landing",
    "custom",
];
