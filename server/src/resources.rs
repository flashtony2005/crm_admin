//! 统一资源网关：固定白名单表上的 CRUD（Phase 1）。
//!
//! - 表与列**白名单写死**（TD-1 固定表结构，拒绝动态建模）；
//! - 每张表绑定权限前缀，动作 → 权限码：
//!   list/get → `{prefix}.view`、create → `.create`、update → `.update`、
//!   delete → `.delete`；可用 *_perm 覆盖（media 上传、automation 开关、
//!   approvals 裁决等特殊语义动作）；
//! - SQL 列 snake_case ↔ JSON camelCase 由 ColDef 映射；
//! - 数组/对象字段以 JSON 字符串落 TEXT 列，读出时还原。

use axum::{extract::{Path, Query, State}, Json};
use sea_orm::{Statement, Value as SqlValue};
use serde_json::{json, Map, Value};
use uuid::Uuid;

use crate::{
    auth::{ensure, Auth},
    db::now_iso,
    error::{ok, ok_list, ApiError, ApiResult},
    state::AppState,
};

/// 列类型（对齐 DDL）
#[derive(Clone, Copy)]
pub enum Col {
    /// 非空 TEXT；数组/对象自动 JSON 序列化
    Text,
    /// 可空 TEXT（None → JSON null）
    TextNull,
    Real,
    Int,
    /// INTEGER 0/1 ↔ JSON bool
    Bool,
    /// 保密值（如 API Key）：可写，读出恒为 "configured" 占位
    Secret,
}

pub struct ColDef {
    pub sql: &'static str,
    pub json: &'static str,
    pub kind: Col,
}

macro_rules! cols {
    ($(($sql:expr, $json:expr, $kind:expr)),+ $(,)?) => {
        &[$(ColDef { sql: $sql, json: $json, kind: $kind }),+]
    };
}
macro_rules! texts {
    ($($c:expr),+) => { &[$(ColDef { sql: $c, json: $c, kind: Col::Text }),+] };
}

pub struct TableDef {
    pub key: &'static str,
    pub table: &'static str,
    pub perm_prefix: &'static str,
    pub create_perm: Option<&'static str>,
    pub update_perm: Option<&'static str>,
    pub delete_perm: Option<&'static str>,
    pub columns: &'static [ColDef],
}

pub static TABLES: &[TableDef] = &[
    // ── Content ──
    TableDef {
        key: "articles", table: "articles", perm_prefix: "content.articles",
        create_perm: None, update_perm: None, delete_perm: None,
        columns: cols![
            ("title","title",Col::Text),("slug","slug",Col::Text),
            ("summary","summary",Col::Text),
            ("content","content",Col::Text),("status","status",Col::Text),
            ("author","author",Col::Text),("tags","tags",Col::Text),
            ("featured_image","featuredImage",Col::TextNull),
            ("published_at","publishedAt",Col::TextNull),
            ("meta_title","metaTitle",Col::TextNull),
            ("meta_description","metaDescription",Col::TextNull),
            ("featured","featured",Col::Bool),
            ("scheduled_at","scheduledAt",Col::TextNull),
            ("canonical_url","canonicalUrl",Col::TextNull),
            // 付费墙/可见性（回归修复：曾在此重构中丢失，致后台无法设置售卖分级）
            ("visibility","visibility",Col::Text),
            ("locale","locale",Col::Text),
            ("paid_level","paidLevel",Col::Int),
            ("price_points","pricePoints",Col::Int),
            // 内容类型（迁移 0011）：post = 文章，problem = 问题页。
            // 必须登记进 TableDef —— 历史教训：漏列会让通用网关**静默丢弃**
            // 该字段（articles 的 visibility/paid_level 就这么丢过一次），
            // 表现为「后台保存成功但字段不变」，极难排查。
            ("kind","kind",Col::Text),
        ],
    },
    TableDef {
        key: "pages", table: "pages", perm_prefix: "content.pages",
        create_perm: None, update_perm: None, delete_perm: None,
        columns: texts!("title", "slug", "content", "status"),
    },
    // 独立 Tag 管理：描述/封面/SEO 字段，供内容组织专业化（P3 中优先级）
    TableDef {
        key: "tags", table: "tags", perm_prefix: "content.tags",
        create_perm: None, update_perm: None, delete_perm: None,
        columns: cols![
            ("name","name",Col::Text),
            ("slug","slug",Col::Text),
            ("description","description",Col::Text),
            ("cover_image","coverImage",Col::TextNull),
            ("meta_title","metaTitle",Col::TextNull),
            ("meta_description","metaDescription",Col::TextNull),
        ],
    },
    TableDef {
        key: "products", table: "products", perm_prefix: "content.products",
        create_perm: None, update_perm: None, delete_perm: None,
        columns: cols![("name","name",Col::Text),("price","price",Col::Real),("description","description",Col::Text),("status","status",Col::Text)],
    },
    TableDef {
        key: "media", table: "media_items", perm_prefix: "content.media",
        create_perm: Some("content.media.upload"), update_perm: None,
        delete_perm: Some("content.media.delete"),
        columns: cols![
            ("name","name",Col::Text),
            ("url","url",Col::Text),
            ("size","sizeKb",Col::Int),
            ("kind","type",Col::Text),
            ("thumbnail","thumbnail",Col::TextNull),
            ("large","large",Col::TextNull),
            ("width","width",Col::Int),
            ("height","height",Col::Int),
            ("srcset","srcset",Col::TextNull),
        ],
    },
    // ── Business ──
    TableDef {
        key: "customers", table: "customers", perm_prefix: "business.customers",
        create_perm: None, update_perm: None, delete_perm: None,
        columns: cols![
            ("name","name",Col::Text),("phone","phone",Col::Text),("source","source",Col::Text),
            ("tags","tags",Col::Text),("priority","priority",Col::Text),
            ("note","note",Col::Text),("last_contact_at","lastContactAt",Col::Text),
        ],
    },
    TableDef {
        key: "leads", table: "leads", perm_prefix: "business.leads",
        create_perm: None, update_perm: None, delete_perm: None,
        columns: texts!("name", "phone", "interest", "source", "status"),
    },
    TableDef {
        key: "forms", table: "forms", perm_prefix: "business.forms",
        create_perm: None, update_perm: None, delete_perm: Some("team.roles.manage"),
        columns: cols![
            ("title","title",Col::Text),("field_count","fieldCount",Col::Int),
            ("submissions","submissions",Col::Int),("status","status",Col::Text),
        ],
    },
    // ── AI ──
    TableDef {
        key: "approvals", table: "approvals", perm_prefix: "ai.approvals",
        // 写动作全部收敛到裁决权（创建走 Phase 2 AI 执行器）
        create_perm: Some("ai.approvals.decide"),
        update_perm: Some("ai.approvals.decide"),
        delete_perm: Some("ai.approvals.decide"),
        columns: cols![
            ("action","action",Col::Text),("target","target",Col::Text),
            ("requested_by","requestedBy",Col::Text),("risk","risk",Col::Text),
            ("status","status",Col::Text),("summary","summary",Col::Text),
            ("decided_at","decidedAt",Col::TextNull),
            ("payload","payload",Col::TextNull),
        ],
    },
    TableDef {
        key: "ai-tasks", table: "ai_tasks", perm_prefix: "ai.tasks",
        // 任务由 AI 执行器产生（Phase 2）；当前写动作仅 Owner
        create_perm: Some("ai.approvals.decide"),
        update_perm: Some("ai.approvals.decide"),
        delete_perm: Some("ai.approvals.decide"),
        columns: cols![
            ("title","title",Col::Text),("capability","capability",Col::Text),
            ("status","status",Col::Text),("result","result",Col::TextNull),
        ],
    },
    // ── Automation ──
    TableDef {
        key: "workflows", table: "workflows", perm_prefix: "automation.workflows",
        // 启停是独立权限码（导航锚定同源）
        update_perm: Some("automation.workflows.toggle"),
        create_perm: Some("automation.workflows.toggle"),
        delete_perm: Some("automation.workflows.toggle"),
        columns: cols![
            ("name","name",Col::Text),("trigger_expr","trigger",Col::Text),
            ("event","event",Col::Text),
            ("step_count","stepCount",Col::Int),("enabled","enabled",Col::Bool),
            ("last_run_at","lastRunAt",Col::TextNull),
            ("steps","steps",Col::Text),
        ],
    },
    TableDef {
        key: "integrations", table: "integrations", perm_prefix: "automation.integrations",
        create_perm: Some("automation.integrations.toggle"),
        update_perm: Some("automation.integrations.toggle"),
        delete_perm: Some("automation.integrations.toggle"),
        columns: cols![
            ("key","key",Col::Text),("name","name",Col::Text),("descr","desc",Col::Text),
            ("category","category",Col::Text),("connected","connected",Col::Bool),
            ("oauth_provider","oauthProvider",Col::Text),
            ("oauth_client_id","oauthClientId",Col::Text),
            ("oauth_client_secret","oauthClientSecret",Col::Secret),
            ("oauth_token","oauthToken",Col::Secret),
            ("api_key","apiKey",Col::Secret),
        ],
    },
    // ── P4 商业层资源（Admin CRUD 网关）──
    TableDef {
        key: "members", table: "members", perm_prefix: "content.members",
        create_perm: Some("content.members.create"), update_perm: Some("content.members.update"),
        delete_perm: Some("content.members.delete"),
        columns: cols![
            ("email","email",Col::Text),("name","name",Col::Text),
            ("status","status",Col::Int),("plan","plan",Col::Text),
            ("stripe_customer_id","stripeCustomerId",Col::Text),
            ("invited_by","invitedBy",Col::Text),
        ],
    },
    // 邀请码：额度制（used 由注册核销自动 +1，后台只配 code/quota/有效期/启停）
    TableDef {
        key: "invite_codes", table: "invite_codes", perm_prefix: "content.members",
        create_perm: Some("content.members.create"), update_perm: Some("content.members.update"),
        delete_perm: Some("content.members.delete"),
        columns: cols![
            ("code","code",Col::Text),("owner_member_id","ownerMemberId",Col::Text),
            ("quota","quota",Col::Int),("used","used",Col::Int),
            ("expires_at","expiresAt",Col::TextNull),("enabled","enabled",Col::Bool),
        ],
    },
    // 兑换码：一次性核销（kind: points=充积分 / plan_days=会员天数）；
    // used_by/used_at 由会员兑换时后端写入，后台只管 code/kind/value。
    TableDef {
        key: "redeem_codes", table: "redeem_codes", perm_prefix: "content.members",
        create_perm: Some("content.members.create"), update_perm: Some("content.members.update"),
        delete_perm: Some("content.members.delete"),
        columns: cols![
            ("code","code",Col::Text),("kind","kind",Col::Text),
            ("value","value",Col::Int),("status","status",Col::Text),
            ("used_by","usedBy",Col::Text),("used_at","usedAt",Col::Text),
        ],
    },
    // 订单：人工确认收款的管理视图（列表/备注改写走网关；真正的「确认收款」
    // 发货走专用路由 /api/admin/orders/{id}/confirm，含原子防重与邀请奖励）。
    TableDef {
        key: "orders", table: "orders", perm_prefix: "content.members",
        create_perm: Some("content.members.never"), update_perm: Some("content.members.update"),
        delete_perm: Some("content.members.delete"),
        columns: cols![
            ("order_no","orderNo",Col::Text),("member_id","memberId",Col::Text),
            ("biz_type","bizType",Col::Text),("tier_id","tierId",Col::Text),
            ("points","points",Col::Int),("plan_days","planDays",Col::Int),
            ("amount_cents","amountCents",Col::Int),("channel","channel",Col::Text),
            ("status","status",Col::Text),("ref_no","refNo",Col::Text),
            ("paid_at","paidAt",Col::Text),
        ],
    },
    // 积分流水：审计只读面（发放/扣减一律走后端逻辑或 Owner 手工调账）
    TableDef {
        key: "points_ledger", table: "points_ledger", perm_prefix: "content.points",
        create_perm: Some("content.points.never"), update_perm: Some("content.points.never"),
        delete_perm: Some("content.points.never"),
        columns: cols![
            ("member_id","memberId",Col::Text),("delta","delta",Col::Int),
            ("balance_after","balanceAfter",Col::Int),("reason","reason",Col::Text),
            ("ref_id","refId",Col::Text),("note","note",Col::Text),
        ],
    },
    TableDef {
        key: "comments", table: "comments", perm_prefix: "content.comments",
        create_perm: None, update_perm: Some("content.comments.update"),
        delete_perm: Some("content.comments.delete"),
        columns: cols![
            ("article_id","articleId",Col::Text),("parent_id","parentId",Col::Text),
            ("author_name","authorName",Col::Text),("author_email","authorEmail",Col::Text),
            ("member_id","memberId",Col::Text),("content","content",Col::Text),
            ("status","status",Col::Text),
        ],
    },
    TableDef {
        key: "subscribers", table: "subscribers", perm_prefix: "newsletter.subscribers",
        create_perm: Some("newsletter.subscribers.view"), update_perm: None,
        delete_perm: Some("newsletter.subscribers.view"),
        columns: cols![("email","email",Col::Text),("name","name",Col::Text),("status","status",Col::Text)],
    },
    TableDef {
        key: "tiers", table: "tiers", perm_prefix: "subscriptions.tiers",
        create_perm: Some("subscriptions.tiers.create"), update_perm: Some("subscriptions.tiers.update"),
        delete_perm: Some("subscriptions.tiers.delete"),
        columns: cols![
            ("name","name",Col::Text),("slug","slug",Col::Text),("description","description",Col::Text),
            ("price_monthly","priceMonthly",Col::Real),("price_yearly","priceYearly",Col::Real),
            ("stripe_price_id","stripePriceId",Col::Text),("features","features",Col::Text),
            ("active","active",Col::Bool),
        ],
    },
    TableDef {
        key: "webhooks", table: "webhook_subscriptions", perm_prefix: "webhooks",
        create_perm: Some("webhooks.create"), update_perm: Some("webhooks.update"),
        delete_perm: Some("webhooks.delete"),
        columns: cols![
            ("event","event",Col::Text),("url","url",Col::Text),("secret","secret",Col::Secret),
            ("active","active",Col::Bool),
        ],
    },
    // ── 首页区块：独立 CMS 资源（关于 / 组织 / 实验 / Web3）──
    // 由后台「设置 → 站点外观 → 首页区块」维护；公开站点经 /api/public/sections 消费。
    // body 为 JSON 文本列（数组/对象自动序列化还原）。
    TableDef {
        key: "sections", table: "sections", perm_prefix: "content.sections",
        create_perm: Some("content.sections.create"),
        update_perm: Some("content.sections.update"),
        delete_perm: Some("content.sections.delete"),
        columns: cols![
            ("slug","slug",Col::Text),
            ("title","title",Col::Text),
            ("subtitle","subtitle",Col::TextNull),
            ("body","body",Col::Text),
            ("icon","icon",Col::TextNull),
            ("sort","sort",Col::Int),
        ],
    },
    // ── 导航与页脚链接（site 域）：后台列表/增删改/启停走通用网关。
    //    公开站点经 /api/public/nav 消费（仅 enabled，按 sort，已分组）。
    //    注意：通用 list 按 updated_at DESC 返回，管理页前端按 sort 重排。
    TableDef {
        key: "navlinks", table: "nav_links", perm_prefix: "site.nav",
        create_perm: None, update_perm: None, delete_perm: None,
        columns: cols![
            ("grp","grp",Col::Text),
            ("label","label",Col::Text),
            ("href","href",Col::Text),
            ("target","target",Col::Text),
            ("sort","sort",Col::Int),
            ("enabled","enabled",Col::Bool),
        ],
    },
    // ── 主页置顶文章（site 域）：指定「哪几篇上主页、顺序、置顶」。
    //    公开站点经 /api/public/home-pins?slot=writing 消费（JOIN articles）。
    TableDef {
        key: "home_pins", table: "home_pins", perm_prefix: "site.homePins",
        create_perm: None, update_perm: None, delete_perm: None,
        columns: cols![
            ("slot","slot",Col::Text),
            ("article_id","articleId",Col::Text),
            ("sort","sort",Col::Int),
            ("enabled","enabled",Col::Bool),
        ],
    },
    TableDef {
        key: "dispatches", table: "article_dispatches", perm_prefix: "content.articles",
        create_perm: Some("content.articles.update"), update_perm: Some("content.articles.update"),
        delete_perm: Some("content.articles.update"),
        columns: cols![
            ("article_id","articleId",Col::Text),("channel","channel",Col::Text),
            ("external_url","externalUrl",Col::TextNull),("status","status",Col::Text),
            ("dispatched_at","dispatchedAt",Col::TextNull),("note","note",Col::TextNull),
        ],
    },
    // ── SEO（P0-1：重定向 + 404 监控，对标 Rank Math）──
    // perm_prefix 用新码 content.seo：它不属于内容生产（文章/标签）也不属于商业层，
    // 而是站点的**入口资产治理**。放进 content.* 会让「内容运营」与「URL 资产」混在一个权限口上。
    TableDef {
        key: "redirects", table: "redirects", perm_prefix: "content.seo",
        create_perm: None, update_perm: None, delete_perm: None,
        columns: cols![
            ("from_path","fromPath",Col::Text),("to_path","toPath",Col::Text),
            ("code","code",Col::Int),("note","note",Col::Text),
            ("hits","hits",Col::Int),("enabled","enabled",Col::Bool),
        ],
    },
    // ── Smart Links（P0-4：点链接自动打标签）──
    // 与 redirects 同属 content.seo：两者都是「站点的入口资产」而非内容本身。
    // 放在这里而不是新开 perm_prefix，是因为能用它的人本来就是同一批
    // （做 SEO / 增长的人），多开一个权限码只会让矩阵变长而不增加控制力。
    TableDef {
        key: "links", table: "smart_links", perm_prefix: "content.seo",
        create_perm: None, update_perm: None, delete_perm: None,
        columns: cols![
            ("token","token",Col::Text),("url","url",Col::Text),
            ("label","label",Col::Text),("tags","tags",Col::Text),
            ("enabled","enabled",Col::Bool),("clicks","clicks",Col::Int),
            ("prefetch","prefetch",Col::Int),
            ("last_click_at","lastClickAt",Col::TextNull),
        ],
    },
    TableDef {
        key: "notfound", table: "not_found_log", perm_prefix: "content.seo",
        create_perm: Some("content.seo.create"), update_perm: Some("content.seo.update"),
        delete_perm: Some("content.seo.delete"),
        columns: cols![
            ("path","path",Col::Text),("referer","referer",Col::Text),
            ("ua","ua",Col::Text),("hits","hits",Col::Int),
            ("last_seen","lastSeen",Col::Text),("resolved","resolved",Col::Bool),
        ],
    },

];

fn def(key: &str) -> Result<&'static TableDef, ApiError> {
    TABLES
        .iter()
        .find(|t| t.key == key)
        .ok_or_else(|| ApiError::not_found(format!("未知资源：{key}")))
}


fn sval(s: String) -> SqlValue {
    SqlValue::String(Some(s))
}

/// JSON 值 → SQL 参数（数组/对象序列化为 JSON 字符串）
fn to_param(v: &Value, col: Col) -> SqlValue {
    match (v, col) {
        (Value::Null, _) => SqlValue::String(None),
        (_, Col::Real) => SqlValue::Double(Some(v.as_f64().unwrap_or(0.0))),
        (_, Col::Int) => SqlValue::BigInt(Some(v.as_i64().unwrap_or(0))),
        (_, Col::Bool) => SqlValue::BigInt(Some(i64::from(v.as_bool().unwrap_or(false)))),
        (Value::String(s), _) => sval(s.clone()),
        (Value::Bool(b), _) => SqlValue::BigInt(Some(if *b { 1 } else { 0 })),
        (Value::Number(n), _) => SqlValue::Double(Some(n.as_f64().unwrap_or(0.0))),
        (other, _) => sval(other.to_string()),
    }
}

/// TEXT 列值 → JSON（尝试还原数组/对象，失败则原样字符串）
fn from_text(s: String) -> Value {
    let t = s.trim();
    if (t.starts_with('[') && t.ends_with(']')) || (t.starts_with('{') && t.ends_with('}')) {
        if let Ok(v) = serde_json::from_str::<Value>(t) {
            return v;
        }
    }
    Value::String(s)
}

async fn fetch_row(
    st: &AppState,
    d: &TableDef,
    id: &str,
) -> Result<Option<Map<String, Value>>, ApiError> {
    let cols = d.columns.iter().map(|c| c.sql.to_string()).collect::<Vec<_>>().join(", ");
    let sql = format!(
        "SELECT id, tenant_id, {}, created_at, updated_at FROM {} WHERE id = ? AND tenant_id = ? LIMIT 1",
        cols, d.table
    );
    let res = st
        .db
        .query_one_statement(Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Sqlite,
            sql,
            vec![sval(id.into()), sval(st.tenant.clone())],
        ))
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    match res {
        None => Ok(None),
        Some(r) => {
            let v = row_to_json(d, &r)?;
            Ok(v.as_object().cloned())
        }
    }
}

fn row_to_json(d: &TableDef, r: &crate::cmsdb::Row) -> Result<Value, ApiError> {
    fn internal(e: impl std::fmt::Display) -> ApiError {
        ApiError::bad(format!("行解析失败：{e}"))
    }
    let mut m = Map::new();
    m.insert("id".into(), Value::String(r.try_get("", "id").map_err(internal)?));
    for c in d.columns {
        let v = match c.kind {
            Col::Text => from_text(r.try_get::<String>("", c.sql).unwrap_or_default()),
            // NULL → 键省略（与前端 optional 字段语义一致）
            Col::TextNull => match r.try_get::<Option<String>>("", c.sql) {
                Ok(Some(raw)) => from_text(raw),
                _ => continue,
            },
            Col::Real => Value::from(r.try_get::<f64>("", c.sql).unwrap_or(0.0)),
            Col::Int => Value::from(r.try_get::<i64>("", c.sql).unwrap_or(0)),
            Col::Bool => Value::Bool(r.try_get::<i64>("", c.sql).unwrap_or(0) != 0),
            // 保密列不回显明文
            Col::Secret => match r.try_get::<Option<String>>("", c.sql) {
                Ok(Some(raw)) if !raw.is_empty() => Value::String("configured".into()),
                _ => continue,
            },
        };
        m.insert(c.json.to_string(), v);
    }
    m.insert(
        "createdAt".into(),
        Value::String(r.try_get("", "created_at").map_err(internal)?),
    );
    m.insert(
        "updatedAt".into(),
        Value::String(r.try_get("", "updated_at").map_err(internal)?),
    );
    Ok(Value::Object(m))
}

// ── Handlers ──

/// GET /api/{table}?col=value（白名单列等值过滤，供徽标/页签使用）
pub async fn list(
    State(st): State<AppState>,
    auth: Auth,
    Path(key): Path<String>,
    Query(filters): Query<std::collections::HashMap<String, String>>,
) -> ApiResult {
    let d = def(&key)?;
    ensure(&auth, &format!("{}.view", d.perm_prefix))?;
    // 分页参数从过滤字典剥离（page 1 起；pageSize 上限 200）
    let mut filters = filters;
    let page = filters.remove("page").and_then(|v| v.parse::<u64>().ok());
    let page_size = filters.remove("pageSize").and_then(|v| v.parse::<u64>().ok());

    let mut where_clauses = vec!["tenant_id = ?".to_string()];
    let mut vals: Vec<SqlValue> = vec![sval(st.tenant.clone())];
    for (k, v) in &filters {
        if let Some(c) = d.columns.iter().find(|c| c.json == *k) {
            where_clauses.push(format!("{} = ?", c.sql));
            vals.push(to_param(&Value::String(v.clone()), c.kind));
        }
    }
    let cols = d.columns.iter().map(|c| c.sql.to_string()).collect::<Vec<_>>().join(", ");
    let where_sql = where_clauses.join(" AND ");

    // 真实总数（与过滤条件一致）；未分页时 total == items.len()，向后兼容
    let count_sql = format!("SELECT COUNT(*) AS cnt FROM {} WHERE {}", d.table, where_sql);
    let cnt_row = st
        .db
        .query_one_statement(Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Sqlite,
            count_sql,
            vals.clone(),
        ))
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    let total: i64 = cnt_row
        .and_then(|r| r.try_get::<i64>("", "cnt").ok())
        .unwrap_or(0);

    // 未传分页参数时保持全量返回（兼容客户端搜索/过滤模式）
    let (limit_sql, extra_vals) = match (page, page_size) {
        (Some(p), Some(sz)) => {
            let sz = sz.clamp(1, 200).max(1) as i64;
            let p = p.max(1) as i64;
            (
                " LIMIT ? OFFSET ?".to_string(),
                vec![
                    to_param(&Value::from(sz), Col::Int),
                    to_param(&Value::from((p - 1) * sz), Col::Int),
                ],
            )
        }
        _ => (String::new(), Vec::new()),
    };

    let sql = format!(
        "SELECT id, tenant_id, {}, created_at, updated_at FROM {} WHERE {} ORDER BY updated_at DESC{}",
        cols, d.table, where_sql, limit_sql
    );
    vals.extend(extra_vals);
    let rows = st
        .db
        .query_all_statement(Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Sqlite,
            sql,
            vals,
        ))
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    let items: Vec<Value> = rows.iter().map(|r| row_to_json(d, r)).collect::<Result<_, _>>()?;
    ok_list(items, total as usize)
}

/// GET /api/{table}/{id}
pub async fn get_one(
    State(st): State<AppState>,
    auth: Auth,
    Path((key, id)): Path<(String, String)>,
) -> ApiResult {
    let d = def(&key)?;
    ensure(&auth, &format!("{}.view", d.perm_prefix))?;
    let row = fetch_row(&st, d, &id).await?;
    match row {
        Some(m) => ok(Value::Object(m)),
        None => Err(ApiError::not_found("记录不存在")),
    }
}

/// POST /api/{table}
pub async fn create(
    State(st): State<AppState>,
    auth: Auth,
    Path(key): Path<String>,
    Json(body): Json<Value>,
) -> ApiResult {
    let d = def(&key)?;
    ensure(&auth, d.create_perm.unwrap_or(&format!("{}.create", d.perm_prefix)))?;
    let id = Uuid::new_v4().to_string();
    let now = now_iso();

    let mut cols: Vec<&str> = vec!["id", "tenant_id"];
    let mut vals: Vec<SqlValue> = vec![
        sval(id.clone()),
        sval(auth.0.tenant.clone()),
    ];
    for c in d.columns {
        if let Some(v) = body.get(c.json) {
            if !v.is_null() {
                cols.push(c.sql);
                vals.push(to_param(v, c.kind));
            }
        }
    }
    cols.push("created_at");
    vals.push(sval(now.clone()));
    cols.push("updated_at");
    vals.push(sval(now));

    let placeholders = vec!["?"; cols.len()].join(", ");
    let sql = format!(
        "INSERT INTO {} ({}) VALUES ({})",
        d.table,
        cols.join(", "),
        placeholders
    );
    st.db
        .execute_statement(Statement::from_sql_and_values(sea_orm::DatabaseBackend::Sqlite, sql, vals))
        .await
        .map_err(|e| ApiError::bad(format!("创建失败：{e}")))?;

    match fetch_row(&st, d, &id).await? {
        Some(m) => ok(Value::Object(m)),
        None => Err(ApiError::bad("创建后读取失败")),
    }
}

/// PUT /api/{table}/{id}
pub async fn update(
    State(st): State<AppState>,
    auth: Auth,
    Path((key, id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> ApiResult {
    let d = def(&key)?;
    ensure(&auth, d.update_perm.unwrap_or(&format!("{}.update", d.perm_prefix)))?;

    let mut sets: Vec<String> = Vec::new();
    let mut vals: Vec<SqlValue> = Vec::new();
    for c in d.columns {
        if let Some(v) = body.get(c.json) {
            // F3 修复：Secret 列读出时恒为 "configured" 占位（不泄露真实密钥），
            // 若原样回写，后台编辑 integrations / webhooks 等记录保存后，
            // 真实密钥会被覆盖成字符串 "configured" —— 集成静默失效且不可逆
            // （原密钥已无法读回，只能重新录入）。此处跳过未改动的占位值。
            if matches!(c.kind, Col::Secret) && v.as_str() == Some("configured") {
                continue;
            }
            if !v.is_null() || matches!(c.kind, Col::TextNull) {
                sets.push(format!("{} = ?", c.sql));
                vals.push(to_param(v, c.kind));
            }
        }
    }
    if sets.is_empty() {
        return Err(ApiError::bad("没有可更新的字段"));
    }
    sets.push("updated_at = ?".to_string());
    vals.push(sval(now_iso()));
    vals.push(sval(id.clone()));
    vals.push(sval(auth.0.tenant.clone()));

    let sql = format!(
        "UPDATE {} SET {} WHERE id = ? AND tenant_id = ?",
        d.table,
        sets.join(", ")
    );
    st.db
        .execute_statement(Statement::from_sql_and_values(sea_orm::DatabaseBackend::Sqlite, sql, vals))
        .await
        .map_err(|e| ApiError::bad(format!("更新失败：{e}")))?;

    match fetch_row(&st, d, &id).await? {
        Some(m) => ok(Value::Object(m)),
        None => Err(ApiError::not_found("记录不存在")),
    }
}

/// DELETE /api/{table}/{id}
pub async fn remove(
    State(st): State<AppState>,
    auth: Auth,
    Path((key, id)): Path<(String, String)>,
) -> ApiResult {
    let d = def(&key)?;
    ensure(&auth, d.delete_perm.unwrap_or(&format!("{}.delete", d.perm_prefix)))?;
    st.db
        .execute_statement(Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Sqlite,
            format!("DELETE FROM {} WHERE id = ? AND tenant_id = ?", d.table),
            vec![sval(id.clone()), sval(auth.0.tenant.clone())],
        ))
        .await
        .map_err(|e| ApiError::bad(format!("删除失败：{e}")))?;
    ok(json!({ "deleted": id }))
}

/// POST /api/approvals/{id}/decide —— 审批裁决（领域动作，非泛型 CRUD）
/// 权限：ai.approvals.decide（Owner）。这是「AI 是执行者，不是超级管理员」的落点。
pub async fn decide(
    State(st): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> ApiResult {
    ensure(&auth, "ai.approvals.decide")?;
    let status = body.get("status").and_then(|v| v.as_str()).unwrap_or("");
    if !matches!(status, "approved" | "rejected") {
        return Err(ApiError::bad("status 必须为 approved | rejected"));
    }
    let now = now_iso();
    let res = st
        .db
        .execute_statement(Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Sqlite,
            "UPDATE approvals SET status = ?, decided_at = ?, updated_at = ? \
             WHERE id = ? AND tenant_id = ? AND status = 'pending'",
            vec![
                sval(status.into()),
                sval(now.clone()),
                sval(now.clone()),
                sval(id.clone()),
                sval(auth.0.tenant.clone()),
            ],
        ))
        .await
        .map_err(|e| ApiError::bad(format!("裁决失败：{e}")))?;
    if res == 0 {
        // 幂等保护：不存在 / 已裁决（重复裁决不产生第二次效果）
        return Err(ApiError::not_found("审批不存在或已裁决"));
    }
    // 批准 → 执行携带的 AI 动作（G4 链路最后一环）
    // 通用化：以「裁决者」身份重放 payload 中的 capability + input，走
    // ai::run_capability 同一条 Policy → Action → Audit 链路（owner 持有
    // 全部权限码 → 直接执行；同时执行本身也写入审计，闭环更完整）。
    // 审批通过后重放 payload 里的 capability（域模型能力已随 B 方案移除，仅存文章 4 能力）。
    if status == "approved" {
        if let Some(Some(m)) = fetch_row(&st, def("approvals")?, &id).await?.as_ref().map(|m| Some(m.clone())) {
            // payload 列经 from_text 已解析为 JSON 对象（{...} 开头）；
            // 兜底兼容仍是字符串的场景。解析出 {capability, input} 后重放。
            let cap: Option<Value> = m.get("payload").cloned().and_then(|p| match p {
                Value::Object(_) => Some(p),
                Value::String(s) => serde_json::from_str::<Value>(&s).ok(),
                _ => None,
            });
            if let Some(cap) = cap {
                let capability = cap.get("capability").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let input = cap.get("input").cloned().unwrap_or(json!({}));
                if !capability.is_empty() {
                    let _ = crate::ai::run_capability(&st, &auth, &capability, &input).await;
                }
            }
        }
    }
    // G4 审计闭环：裁决动作本身留痕（approved / rejected）
    {
        let sql = "INSERT INTO ai_audit_log (id, tenant_id, actor, actor_role, capability, decision, target_id, detail, created_at) \
                   VALUES (?, ?, ?, ?, 'ai.approvals.decide', ?, ?, ?, ?)";
        let vals = vec![
            sval(uuid::Uuid::new_v4().to_string()),
            sval(st.tenant.clone()),
            sval(auth.0.username.clone()),
            sval(auth.0.role.clone()),
            sval(status.into()),
            sval(id.clone()),
            sval(format!("审批裁决：{}", status)),
            sval(now_iso()),
        ];
        let db = st.db.clone();
        tokio::spawn(async move {
            let _ = db.execute_statement(sea_orm::Statement::from_sql_and_values(sea_orm::DatabaseBackend::Sqlite, sql, vals)).await;
        });
    }
    // B1 推送：把裁决结果告知发起人
    crate::notify::approval_decided(&st, status, "审批请求");

    match fetch_row(&st, def("approvals")?, &id).await? {
        Some(m) => ok(Value::Object(m)),
        None => Err(ApiError::bad("裁决后读取失败")),
    }
}
