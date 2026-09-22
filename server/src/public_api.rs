//! 公开内容 API（免认证，只读）：
//! - GET /api/public/articles           已发布文章列表（不含正文，便于列表聚合）
//! - GET /api/public/articles?tag=xxx   按标签过滤（tags 字段包含匹配）
//! - GET /api/public/articles/{id}      单篇详情（含正文；id 或 slug 均可解析）
//! - GET /api/public/tags               标签列表（独立 Tag 管理表）
//!
//! 仅返回 status='published' 且属于当前租户的文章；供公开站点前端 /
//! Jamstack / 第三方应用消费（headless 用法）。响应沿用统一信封
//! {ok:true,data}，与前端 api()/apiList() 约定一致。
//!
//! 部署：新增模块，本地 `cargo build` 后随新二进制生效。

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use crate::cmsdb::Row;
use sea_orm::Value as SqlValue;
use serde_json::{json, Value};
use std::collections::HashMap;

use crate::error::{ok, ApiError, ApiResult};
use crate::site;
use crate::state::AppState;

/// 读 TEXT 列（NULL→空串）
fn s(r: &Row, col: &str) -> String {
    r.try_get::<String>("", col).unwrap_or_default()
}

/// 读可空 TEXT 列
fn so(r: &Row, col: &str) -> Option<String> {
    r.try_get::<Option<String>>("", col).ok().flatten()
}

/// 由托管上传 URL（形如 /uploads/<uuid>.<ext> 或 <PUBLIC_BASE_URL>/uploads/<uuid>.<ext>）
/// 推导响应式 srcset 字符串（与 upload.rs 生成的 480/960/1600 变体命名一致）。
/// 非托管上传（外链 / data URL）返回 None。
fn build_srcset(url: &str) -> Option<String> {
    let fname = url.rsplit('/').next()?;
    let (stem, ext) = fname.rsplit_once('.')?;
    if !matches!(ext.to_ascii_lowercase().as_str(), "jpg" | "jpeg" | "png" | "gif" | "webp") {
        return None;
    }
    let head = &url[..url.len() - fname.len()];
    let widths = [480u32, 960, 1600];
    let parts: Vec<String> = widths.iter().map(|w| format!("{head}{stem}_{w}.{ext} {w}w")).collect();
    Some(parts.join(", "))
}

/// 列表行 → JSON（白名单字段，不暴露正文以外的内部列）
fn row_json(r: &Row, with_content: bool) -> Value {
    let mut v = json!({
        "id": s(r, "id"),
        "title": s(r, "title"),
        "slug": s(r, "slug"),
        "summary": s(r, "summary"),
        "author": s(r, "author"),
        "tags": s(r, "tags"),
        "featured_image": so(r, "featured_image"),
        "published_at": so(r, "published_at"),
        "meta_title": so(r, "meta_title"),
        "meta_description": so(r, "meta_description"),
        "locale": so(r, "locale"),
        // 内容类型：前端据此选版式（文章 vs 问题页）
        "kind": so(r, "kind"),
        "visibility": so(r, "visibility"),
        "featured": r.try_get::<i64>("", "featured").unwrap_or(0) != 0,
        "scheduled_at": so(r, "scheduled_at"),
        "canonical_url": so(r, "canonical_url"),
        "updated_at": s(r, "updated_at"),
    });
    if with_content {
        v["content"] = json!(s(r, "content"));
    }
    // 封面图若为本站托管上传，附带响应式 srcset（供前端 <img srcSet>）
    if let Some(img) = v.get("featured_image").and_then(|x| x.as_str()) {
        if !img.is_empty() {
            if let Some(ss) = build_srcset(img) {
                v["featured_image_srcset"] = json!(ss);
            }
        }
    }
    v
}

/// 付费墙拦截：返回 HTTP 402 + 结构化 `locked` 体（含可见元数据预览，不含正文），
/// 供前端渲染「解锁」区块。`reason` 标明具体门槛：
/// login（需登录会员）/ subscription（需付费订阅）/ points（积分买断）/ invite（邀请会员专享）。
fn locked(reason: &str, message: impl Into<String>, preview: Value, paid_level: i64, price_points: i64) -> ApiResult {
    Ok((
        StatusCode::PAYMENT_REQUIRED,
        Json(json!({
            "ok": false,
            "locked": true,
            "reason": reason,
            "paidLevel": paid_level,
            "pricePoints": price_points,
            "error": message.into(),
            "preview": preview,
        })),
    )
        .into_response())
}

/// 会员门槛查询行：订阅档位 + 有效期 + 邀请来源
struct MemberGate {
    plan: String,
    plan_expires_at: String,
    invited_by: String,
    status: i64,
}

impl MemberGate {
    /// 账号本身可用：status == 1。
    ///
    /// P0-4：付费墙此前只认令牌、不看 status，管理员停用会员后对方凭旧令牌
    /// 仍能解锁付费内容。所有权益判定都必须先过这一关。
    fn active(&self) -> bool {
        self.status == 1
    }

    /// 订阅有效：账号可用 且 plan != 'free' 且（未设到期 或 未到期）
    fn subscribed(&self) -> bool {
        if !self.active() || self.plan == "free" {
            return false;
        }
        if self.plan_expires_at.is_empty() {
            return true;
        }
        match chrono::DateTime::parse_from_rfc3339(&self.plan_expires_at) {
            Ok(exp) => exp.with_timezone(&chrono::Utc) > chrono::Utc::now(),
            // P0-1（安全默认）：此前解析失败**视为不限期**，等于把一条脏数据
            // 变成永久会员。存疑时应当拒绝，而不是放行 —— 到期时间写坏了
            // 是运营问题，白送权益是资损。
            Err(_) => false,
        }
    }
}

async fn member_gate_row(st: &AppState, member_id: &str) -> Result<Option<MemberGate>, ApiError> {
    let r = st
        .db
        .query_one(
            "SELECT plan, plan_expires_at, invited_by, status FROM members WHERE id = ? AND tenant_id = ? LIMIT 1",
            vec![
                SqlValue::String(Some(member_id.to_string())),
                SqlValue::String(Some(st.tenant.clone())),
            ],
        )
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    Ok(r.map(|x| MemberGate {
        plan: x.try_get("", "plan").unwrap_or_else(|_| "free".into()),
        plan_expires_at: x.try_get("", "plan_expires_at").unwrap_or_default(),
        invited_by: x.try_get("", "invited_by").unwrap_or_default(),
        status: x.try_get::<i64>("", "status").unwrap_or(1),
    }))
}

/// 积分买断判定：points_ledger 存在 reason='purchase' AND ref_id=文章id 的流水
async fn owns_article(st: &AppState, member_id: &str, article_id: &str) -> bool {
    st.db
        .query_one(
            "SELECT id FROM points_ledger WHERE tenant_id = ? AND member_id = ? \
             AND reason = 'purchase' AND ref_id = ? LIMIT 1",
            vec![
                SqlValue::String(Some(st.tenant.clone())),
                SqlValue::String(Some(member_id.to_string())),
                SqlValue::String(Some(article_id.to_string())),
            ],
        )
        .await
        .ok()
        .flatten()
        .is_some()
}

/// GET /api/public/articles —— 已发布文章列表（不含正文；可选 ?tag= / ?locale= / ?featured=1 / ?limit= 过滤）
pub async fn articles(
    State(st): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> ApiResult {
    let items = articles_value(&st, &params).await?;
    ok(items)
}

/// 已发布文章列表的纯数据实现（单个端点与 `/api/public/home` 聚合共用）
async fn articles_value(
    st: &AppState,
    params: &HashMap<String, String>,
) -> Result<Value, ApiError> {
    let tag = params.get("tag").map(|s| s.trim()).filter(|s| !s.is_empty());
    let locale = params.get("locale").map(|s| s.trim()).filter(|s| !s.is_empty() && *s != "all");
    let author = params.get("author").map(|s| s.trim()).filter(|s| !s.is_empty());
    // ?kind=problem 只看问题页；不传则不过滤，文章流行为完全不变。
    let kind = params.get("kind").map(|s| s.trim()).filter(|s| !s.is_empty());
    let featured_only = params.get("featured").map(|s| s == "1").unwrap_or(false);
    // ?limit= 控制返回条数（默认 100，上限 200，供首页「显示条数」配置消费）
    let limit = params
        .get("limit")
        .and_then(|s| s.trim().parse::<i64>().ok())
        .map(|n| n.clamp(1, 200))
        .unwrap_or(100);
    let mut sql = String::from(
        "SELECT id, title, slug, summary, author, tags, featured_image, \
         published_at, meta_title, meta_description, locale, visibility, featured, scheduled_at, \
         canonical_url, paid_level, price_points, kind, updated_at FROM articles \
         WHERE tenant_id = ? AND status = 'published'",
    );
    let mut args = vec![SqlValue::String(Some(st.tenant.clone()))];
    if let Some(t) = tag {
        sql.push_str(" AND tags LIKE ?");
        args.push(SqlValue::String(Some(format!("%{}%", t))));
    }
    if let Some(l) = locale {
        sql.push_str(" AND locale = ?");
        args.push(SqlValue::String(Some(l.to_string())));
    }
    if let Some(a) = author {
        sql.push_str(" AND author = ?");
        args.push(SqlValue::String(Some(a.to_string())));
    }
    if featured_only {
        sql.push_str(" AND featured = 1");
    }
    if let Some(k) = kind {
        // COALESCE：极旧库还没有 kind 列时按 post 处理，不因缺列把文章全漏掉
        sql.push_str(" AND COALESCE(kind, 'post') = ?");
        args.push(SqlValue::String(Some(k.to_string())));
    }
    sql.push_str(" ORDER BY featured DESC, COALESCE(published_at, updated_at) DESC LIMIT ?");
    args.push(SqlValue::BigInt(Some(limit)));
    let rows = st
        .db
        .query_all(&sql, args)
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    let items: Vec<Value> = rows.iter().map(|r| row_json(r, false)).collect();
    Ok(json!(items))
}

/// GET /api/public/articles/{id} —— 单篇详情（含正文；id 或 slug 均可解析）
///
/// 付费墙判定（paid_level 优先于旧 visibility 字段）：
/// - paid_level 0：回落 visibility —— public 全文；members 需会员登录；paid 需付费订阅
/// - paid_level 1（订阅会员）：member.plan != 'free' 放行
/// - paid_level 2（积分买断）：P0 结构就位，P1 积分商城上线后接购买记录
/// - paid_level 3（邀请专享）：member.invited_by 非空（凭邀请码注册的会员）放行
/// 管理端角色（owner/editor/viewer）始终可预览。
pub async fn article_detail(
    State(st): State<AppState>,
    headers: HeaderMap,
    Path(key): Path<String>,
) -> ApiResult {
    let sql = "SELECT id, title, slug, summary, author, tags, featured_image, \
               published_at, meta_title, meta_description, updated_at, content, visibility, locale, \
               featured, scheduled_at, canonical_url, paid_level, price_points, kind FROM articles \
               WHERE tenant_id = ? AND status = 'published' AND (id = ? OR slug = ?) LIMIT 1";
    let rows = st
        .db
        .query_all(
            sql,
            vec![
                SqlValue::String(Some(st.tenant.clone())),
                SqlValue::String(Some(key.clone())),
                SqlValue::String(Some(key)),
            ],
        )
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    let Some(r) = rows.first() else {
        return Err(ApiError::not_found("文章不存在或未发布"));
    };
    let visibility: String = r.try_get("", "visibility").unwrap_or_else(|_| "public".into());
    let paid_level: i64 = r.try_get::<i64>("", "paid_level").unwrap_or(0);
    let price_points: i64 = r.try_get::<i64>("", "price_points").unwrap_or(0);

    // 解析可选 Bearer 令牌（管理员 JWT 与会员 JWT 同钥但 role 不同）
    let token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    let claims = token.and_then(|t| crate::auth::verify(t));
    // 管理端角色直接放行（作者预览自有内容）
    let is_staff = claims
        .as_ref()
        .map(|c| matches!(c.role.as_str(), "owner" | "editor" | "viewer"))
        .unwrap_or(false);

    if !is_staff {
        // 门槛归一：(Some(reason) 表示需要拦截判定)
        let gate: Option<(&str, String)> = if paid_level > 0 {
            Some(match paid_level {
                1 => ("subscription", "该内容需要付费会员订阅才能解锁".into()),
                2 => ("points", "该内容需积分解锁（积分商城即将开放）".into()),
                3 => ("invite", "该内容仅限邀请加入的会员访问".into()),
                _ => ("login", "该内容需要会员登录后访问".into()),
            })
        } else if visibility == "paid" {
            Some(("subscription", "该内容需要付费会员订阅才能解锁".into()))
        } else if visibility == "members" {
            Some(("login", "该内容需要会员登录后访问".into()))
        } else {
            None
        };

        if let Some((reason, message)) = gate {
            // 仅 role=member 的令牌参与权益判定
            let member = match claims.as_ref() {
                Some(c) if c.role == "member" => member_gate_row(&st, &c.sub).await?,
                _ => None,
            };
            let entitled = match reason {
                "subscription" => member.as_ref().map(|m| m.subscribed()).unwrap_or(false),
                // 积分买断：points_ledger 有该文购买流水即放行（购买走
                // POST /api/public/members/purchase-article，原子扣减）
                "points" => match (&claims, member.as_ref()) {
                    (Some(c), _) if c.role == "member" => {
                        let aid = r.try_get::<String>("", "id").unwrap_or_default();
                        owns_article(&st, &c.sub, &aid).await
                    }
                    _ => false,
                },
                "invite" => member
                    .as_ref()
                    .map(|m| m.active() && !m.invited_by.is_empty())
                    .unwrap_or(false),
                // 兜底分支（login）：停用账号不算"已登录会员"
                _ => member.as_ref().map(|m| m.active()).unwrap_or(false),
            };
            if !entitled {
                return locked(reason, message, row_json(r, false), paid_level, price_points);
            }
        }
    }
    ok(row_json(r, true))
}

/// GET /api/public/sections —— 首页区块（免认证，供公开站点消费），按 sort 升序
///
/// 返回每个区块的 `{ id, slug, title, subtitle, icon, body, sort, updatedAt }`；
/// `body` 是区块的结构化内容（JSON 对象，已从 TEXT 还原）。供 coucouya 等
/// 公开站点按 slug（about / org / lab / web3）合并进首页内容。
pub async fn sections(State(st): State<AppState>) -> ApiResult {
    let items = sections_value(&st).await?;
    ok(items)
}

/// 首页区块的纯数据实现（单个端点与 `/api/public/home` 聚合共用）
async fn sections_value(st: &AppState) -> Result<Value, ApiError> {
    let sql = "SELECT id, slug, title, subtitle, body, icon, sort, updated_at \
               FROM sections WHERE tenant_id = ? ORDER BY sort ASC, updated_at ASC";
    let rows = st
        .db
        .query_all(sql, vec![SqlValue::String(Some(st.tenant.clone()))])
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            let body_raw = s(r, "body");
            let body: Value = serde_json::from_str(&body_raw)
                .unwrap_or_else(|_| Value::Object(serde_json::Map::new()));
            json!({
                "id": s(r, "id"),
                "slug": s(r, "slug"),
                "title": s(r, "title"),
                "subtitle": so(r, "subtitle"),
                "icon": so(r, "icon"),
                "body": body,
                "sort": r.try_get::<i64>("", "sort").unwrap_or(0),
                "updatedAt": s(r, "updated_at"),
            })
        })
        .collect();
    Ok(json!(items))
}

/// GET /api/public/tags —— 标签列表（独立 Tag 管理表，含文章计数）
pub async fn tags(State(st): State<AppState>) -> ApiResult {
    let sql = "SELECT t.id, t.name, t.slug, t.description, t.cover_image, \
               (SELECT COUNT(*) FROM articles a WHERE a.tenant_id = t.tenant_id \
                AND a.status = 'published' AND a.tags LIKE '%' || t.name || '%') AS post_count \
               FROM tags t WHERE t.tenant_id = ? ORDER BY t.name COLLATE NOCASE LIMIT 200";
    let rows = st
        .db
        .query_all(sql, vec![SqlValue::String(Some(st.tenant.clone()))])
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id": s(r, "id"),
                "name": s(r, "name"),
                "slug": s(r, "slug"),
                "description": s(r, "description"),
                "cover_image": so(r, "cover_image"),
                "post_count": r.try_get::<i64>("", "post_count").unwrap_or(0),
            })
        })
        .collect();
    ok(json!(items))
}

/// GET /api/public/nav —— 导航与页脚链接（免认证）
///
/// 返回 `{ nav: [...], footer: [...] }`，仅 enabled=1、按 (grp, sort) 升序。
/// 数据源 nav_links 表（后台「设置 → 站点外观 → 导航与页脚链接」维护）；
/// 公开站点 SiteNav/SiteFooter 优先读此接口，读不到再回落内置默认。
pub async fn nav(State(st): State<AppState>) -> ApiResult {
    let v = nav_value(&st).await?;
    ok(v)
}

/// 导航 / 页脚链接的纯数据实现（单个端点与 `/api/public/home` 聚合共用）
async fn nav_value(st: &AppState) -> Result<Value, ApiError> {
    let sql = "SELECT id, grp, label, href, target, sort FROM nav_links \
               WHERE tenant_id = ? AND enabled = 1 \
               ORDER BY grp ASC, sort ASC, created_at ASC";
    let rows = st
        .db
        .query_all(sql, vec![SqlValue::String(Some(st.tenant.clone()))])
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    let mut nav: Vec<Value> = Vec::new();
    let mut footer: Vec<Value> = Vec::new();
    for r in &rows {
        let item = json!({
            "id": s(r, "id"),
            "label": s(r, "label"),
            "href": s(r, "href"),
            "target": s(r, "target"),
        });
        if s(r, "grp") == "footer" {
            footer.push(item);
        } else {
            nav.push(item);
        }
    }
    Ok(json!({ "nav": nav, "footer": footer }))
}

/// GET /api/public/home-pins?slot=writing —— 主页置顶文章（免认证，JOIN articles）
///
/// 仅返回 enabled=1 且文章已发布（status='published'）的固定项，按 sort 升序。
/// 公开站点优先用 pins 精确编排；未配置时回落 featured 过滤。
pub async fn home_pins(State(st): State<AppState>, Query(params): Query<HashMap<String, String>>) -> ApiResult {
    let slot = params.get("slot").map(|s| s.trim()).filter(|s| !s.is_empty()).unwrap_or("writing");
    let items = home_pins_value(&st, slot).await?;
    ok(items)
}

/// 主页置顶的纯数据实现（单个端点与 `/api/public/home` 聚合共用）
async fn home_pins_value(st: &AppState, slot: &str) -> Result<Value, ApiError> {
    let sql = "SELECT p.sort AS pin_sort, a.id, a.title, a.slug, a.summary, a.featured_image, \
               a.published_at, a.tags, a.updated_at \
               FROM home_pins p JOIN articles a ON a.id = p.article_id \
               WHERE p.tenant_id = ? AND p.slot = ? AND p.enabled = 1 \
                 AND a.tenant_id = p.tenant_id AND a.status = 'published' \
               ORDER BY p.sort ASC, a.updated_at DESC";
    let rows = st
        .db
        .query_all(
            sql,
            vec![
                SqlValue::String(Some(st.tenant.clone())),
                SqlValue::String(Some(slot.to_string())),
            ],
        )
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "pinSort": r.try_get::<i64>("", "pin_sort").unwrap_or(0),
                "id": s(r, "id"),
                "title": s(r, "title"),
                "slug": s(r, "slug"),
                "summary": s(r, "summary"),
                "featured_image": so(r, "featured_image"),
                "published_at": so(r, "published_at"),
                "tags": s(r, "tags"),
                "updated_at": s(r, "updated_at"),
            })
        })
        .collect();
    Ok(json!(items))
}

/// GET /api/public/home —— 主页首屏聚合（免认证）
///
/// 一次返回主页渲染所需的全部区块，替代前端并发打 5 个端点：
/// `{ site, sections, nav, footer, pins, articles }`。
/// 任一子块查询失败时**该块单独降级**（null / 空数组），不拖垮整页 —— 主页前端
/// 本就有「失败即回落内置默认内容」的设计，两者叠加后可用性更稳。
pub async fn home(State(st): State<AppState>) -> ApiResult {
    let site = site::site_value(&st).await.unwrap_or(Value::Null);
    let sections = sections_value(&st).await.unwrap_or_else(|_| json!([]));
    let nav = nav_value(&st).await.unwrap_or(Value::Null);
    let pins = home_pins_value(&st, "writing")
        .await
        .unwrap_or_else(|_| json!([]));
    let mut params = HashMap::<String, String>::new();
    params.insert("featured".to_string(), "1".to_string());
    let articles = articles_value(&st, &params).await.unwrap_or_else(|_| json!([]));
    let (nav_items, footer_items) = match &nav {
        Value::Object(o) => (
            o.get("nav").cloned().unwrap_or_else(|| json!([])),
            o.get("footer").cloned().unwrap_or_else(|| json!([])),
        ),
        _ => (json!([]), json!([])),
    };
    ok(json!({
        "site": site,
        "sections": sections,
        "nav": nav_items,
        "footer": footer_items,
        "pins": pins,
        "articles": articles,
    }))
}
