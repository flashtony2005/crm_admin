//! Content API —— 业务事实的读写与「关系维度」的挂载。
//!
//! 设计要点：
//! 1. **列表不 N+1**：先查 contents，再对这批 id 各发一条联结查询，
//!    在内存里拼装 contexts / channels。
//! 2. **Context / Channel 不是独立页面**：它们作为 Content 的**两个维度**
//!    内嵌在响应里（前者带 role/weight，后者带渠道侧发布状态），
//!    前端据此渲染筛选器与渠道徽章。
//! 3. **data_json 的类型化由前端 schema registry 负责**，
//!    本层只保证「写入什么对象、读出还是什么对象」，
//!    不解释字段含义——否则服务端就会变成第二套内容模型。

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::{
    api::{
        b_js, b_str, int, js, must_get, now, ok, ok_empty, ok_id, ok_list, opt, real, s, so, to_json_text,
        txt,
    },
    auth::{ensure, Auth},
    error::{ApiError, ApiResult},
    state::AppState,
};

// ── 查询参数 ──────────────────────────────────────────────────────────────

#[derive(Deserialize, Default)]
pub struct ContentQuery {
    /// 内容类型：article / profile / organization / project / product …
    pub r#type: Option<String>,
    /// **Context  slug**（不是 tag 名）——Analyst 交叉查询入口
    pub context: Option<String>,
    /// Channel slug：看「哪些内容进了这个渠道」
    pub channel: Option<String>,
    pub status: Option<String>,
    pub locale: Option<String>,
    /// 标题 / 摘要模糊搜索
    pub q: Option<String>,
    pub limit: Option<i64>,
}

// ── 行 → JSON ─────────────────────────────────────────────────────────────

fn content_json(r: &crate::cmsdb::Row) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("id".into(), txt(r, "id").into());
    m.insert("type".into(), txt(r, "type").into());
    m.insert("slug".into(), opt(r, "slug").into());
    m.insert("title".into(), opt(r, "title").into());
    m.insert("summary".into(), opt(r, "summary").into());
    m.insert("status".into(), txt(r, "status").into());
    m.insert("locale".into(), txt(r, "locale").into());
    m.insert("data".into(), js(r, "data_json"));
    m.insert("metadata".into(), js(r, "metadata_json"));
    m.insert("authorId".into(), opt(r, "author_id").into());
    m.insert("version".into(), int(r, "version").into());
    m.insert("publishedAt".into(), opt(r, "published_at").into());
    m.insert("createdAt".into(), txt(r, "created_at").into());
    m.insert("updatedAt".into(), txt(r, "updated_at").into());
    m
}

/// 批量拉 Context 维度：content_id → [{id,name,slug,type,role,weight}]
async fn contexts_by_content(
    st: &AppState,
    ids: &[String],
) -> Result<std::collections::HashMap<String, Vec<Value>>, ApiError> {
    let mut out: std::collections::HashMap<String, Vec<Value>> = std::collections::HashMap::new();
    if ids.is_empty() {
        return Ok(out);
    }
    let ph = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT cc.content_id AS content_id, cc.role AS role, cc.weight AS weight,
                x.id AS id, x.name AS name, x.slug AS slug, x.type AS type
           FROM content_contexts cc
           JOIN contexts x ON x.id = cc.context_id
          WHERE cc.content_id IN ({ph})
          ORDER BY cc.weight DESC, x.name ASC"
    );
    let rows = st
        .db
        .query_all(&sql, ids.iter().map(|i| s(i.clone())).collect())
        .await
        .map_err(crate::api::db_err("查询 Context 失败"))?;
    for r in rows {
        let cid = txt(&r, "content_id");
        out.entry(cid).or_default().push(json!({
            "id": txt(&r, "id"),
            "name": txt(&r, "name"),
            "slug": txt(&r, "slug"),
            "type": txt(&r, "type"),
            "role": txt(&r, "role"),
            "weight": real(&r, "weight"),
        }));
    }
    Ok(out)
}

/// 批量拉 Channel 维度：content_id → [{id,name,type,status,externalUrl,publishedAt}]
async fn channels_by_content(
    st: &AppState,
    ids: &[String],
) -> Result<std::collections::HashMap<String, Vec<Value>>, ApiError> {
    let mut out: std::collections::HashMap<String, Vec<Value>> = std::collections::HashMap::new();
    if ids.is_empty() {
        return Ok(out);
    }
    let ph = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT cc.content_id AS content_id, cc.status AS status,
                cc.external_id AS external_id, cc.external_url AS external_url,
                cc.published_at AS published_at,
                c.id AS id, c.name AS name, c.type AS type
           FROM content_channels cc
           JOIN channels c ON c.id = cc.channel_id
          WHERE cc.content_id IN ({ph})
          ORDER BY c.name ASC"
    );
    let rows = st
        .db
        .query_all(&sql, ids.iter().map(|i| s(i.clone())).collect())
        .await
        .map_err(crate::api::db_err("查询渠道分发失败"))?;
    for r in rows {
        let cid = txt(&r, "content_id");
        out.entry(cid).or_default().push(json!({
            "id": txt(&r, "id"),
            "name": txt(&r, "name"),
            "type": txt(&r, "type"),
            "status": txt(&r, "status"),
            "externalId": opt(&r, "external_id"),
            "externalUrl": opt(&r, "external_url"),
            "publishedAt": opt(&r, "published_at"),
        }));
    }
    Ok(out)
}

// ── Handlers ──────────────────────────────────────────────────────────────

/// GET /api/v1/content —— 支持按 Context / Channel 交叉过滤
pub async fn list(
    State(st): State<AppState>,
    auth: Auth,
    Query(q): Query<ContentQuery>,
) -> ApiResult {
    ensure(&auth, "domain.content.view")?;

    let mut wheres: Vec<String> = vec!["1=1".into()];
    let mut vals: Vec<sea_orm::Value> = vec![];

    if let Some(t) = &q.r#type {
        wheres.push("type = ?".into());
        vals.push(s(t.clone()));
    }
    if let Some(stt) = &q.status {
        wheres.push("status = ?".into());
        vals.push(s(stt.clone()));
    }
    if let Some(loc) = &q.locale {
        wheres.push("locale = ?".into());
        vals.push(s(loc.clone()));
    }
    // Context 交叉过滤：按 slug 命中语义上下文（Analyst 的核心查询形态）
    if let Some(ctx) = &q.context {
        wheres.push(
            "id IN (SELECT cc.content_id FROM content_contexts cc
                     JOIN contexts x ON x.id = cc.context_id WHERE x.slug = ?)"
                .into(),
        );
        vals.push(s(ctx.clone()));
    }
    if let Some(ch) = &q.channel {
        wheres.push(
            "id IN (SELECT cc.content_id FROM content_channels cc
                     JOIN channels c ON c.id = cc.channel_id WHERE c.slug = ?)"
                .into(),
        );
        vals.push(s(ch.clone()));
    }
    if let Some(kw) = &q.q {
        if !kw.trim().is_empty() {
            wheres.push("(title LIKE ? OR summary LIKE ?)".into());
            let like = format!("%{kw}%");
            vals.push(s(like.clone()));
            vals.push(s(like));
        }
    }

    let limit = q.limit.unwrap_or(200).clamp(1, 500);
    let sql = format!(
        "SELECT * FROM contents WHERE {} ORDER BY updated_at DESC LIMIT {limit}",
        wheres.join(" AND ")
    );
    let rows = st
        .db
        .query_all(&sql, vals)
        .await
        .map_err(crate::api::db_err("查询内容失败"))?;

    let ids: Vec<String> = rows.iter().map(|r| txt(r, "id")).collect();
    let ctxs = contexts_by_content(&st, &ids).await?;
    let chs = channels_by_content(&st, &ids).await?;

    let mut items = Vec::with_capacity(rows.len());
    for r in &rows {
        let id = txt(r, "id");
        let mut m = content_json(r);
        m.insert(
            "contexts".into(),
            ctxs.get(&id).cloned().unwrap_or_default().into(),
        );
        m.insert(
            "channels".into(),
            chs.get(&id).cloned().unwrap_or_default().into(),
        );
        items.push(Value::Object(m));
    }
    // 列表被 LIMIT 截断时 total 仍反映本次返回量；前端不以它做总库统计
    let total = items.len();
    ok_list(items, total)
}

/// GET /api/v1/content/:id
pub async fn get_one(State(st): State<AppState>, auth: Auth, Path(id): Path<String>) -> ApiResult {
    ensure(&auth, "domain.content.view")?;
    let r = must_get(&st, "contents", &id).await?;
    let cid = txt(&r, "id");
    let mut m = content_json(&r);
    let ctxs = contexts_by_content(&st, &[cid.clone()]).await?;
    let chs = channels_by_content(&st, &[cid.clone()]).await?;
    m.insert(
        "contexts".into(),
        ctxs.get(&cid).cloned().unwrap_or_default().into(),
    );
    m.insert(
        "channels".into(),
        chs.get(&cid).cloned().unwrap_or_default().into(),
    );
    ok(Value::Object(m))
}

/// POST /api/v1/content
pub async fn create(State(st): State<AppState>, auth: Auth, Json(body): Json<Value>) -> ApiResult {
    ensure(&auth, "domain.content.create")?;
    let ctype = b_str(&body, "type").ok_or_else(|| ApiError::bad("缺少 type"))?;
    if ctype.trim().is_empty() {
        return Err(ApiError::bad("type 不能为空"));
    }
    let id = uuid::Uuid::new_v4().to_string();
    let ts = now();
    let status = b_str(&body, "status").unwrap_or_else(|| "draft".into());
    // 只有 published 状态才落 published_at，避免「草稿也有发布时间」
    let published_at = if status == "published" { Some(ts.clone()) } else { None };

    st.db
        .execute(
            "INSERT INTO contents
             (id, type, slug, title, summary, status, locale, data_json, metadata_json,
              author_id, version, published_at, created_at, updated_at)
             VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
            vec![
                s(id.clone()),
                s(ctype),
                so(b_str(&body, "slug").filter(|v| !v.is_empty())),
                so(b_str(&body, "title")),
                so(b_str(&body, "summary")),
                s(status),
                s(b_str(&body, "locale").unwrap_or_else(|| "en-US".into())),
                s(to_json_text(b_js(&body, "data").as_ref())),
                s(to_json_text(b_js(&body, "metadata").as_ref())),
                so(b_str(&body, "authorId")),
                sea_orm::Value::BigInt(Some(1)),
                so(published_at),
                s(ts.clone()),
                s(ts),
            ],
        )
        .await
        .map_err(crate::api::db_err("创建内容失败"))?;

    ok_id(id)
}

/// PATCH /api/v1/content/:id —— 只更新请求体里出现的字段
pub async fn update(
    State(st): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> ApiResult {
    ensure(&auth, "domain.content.update")?;
    let cur = must_get(&st, "contents", &id).await?;

    let mut sets: Vec<String> = vec![];
    let mut vals: Vec<sea_orm::Value> = vec![];

    macro_rules! set_txt {
        ($col:expr, $key:expr) => {
            if let Some(v) = b_str(&body, $key) {
                sets.push(format!("{} = ?", $col));
                vals.push(s(v));
            }
        };
    }
    macro_rules! set_opt {
        ($col:expr, $key:expr) => {
            if let Some(v) = b_js(&body, $key) {
                if v.is_null() {
                    sets.push(format!("{} = NULL", $col));
                } else if let Some(t) = v.as_str() {
                    sets.push(format!("{} = ?", $col));
                    vals.push(s(t.to_string()));
                }
            }
        };
    }
    macro_rules! set_js {
        ($col:expr, $key:expr) => {
            if let Some(v) = b_js(&body, $key) {
                sets.push(format!("{} = ?", $col));
                vals.push(s(to_json_text(Some(&v))));
            }
        };
    }

    set_txt!("type", "type");
    set_txt!("status", "status");
    set_txt!("locale", "locale");
    set_opt!("slug", "slug");
    set_opt!("title", "title");
    set_opt!("summary", "summary");
    set_opt!("author_id", "authorId");
    set_js!("data_json", "data");
    set_js!("metadata_json", "metadata");

    // 状态从非 published 迁到 published → 补发布时间；迁出则清空
    if let Some(new_status) = b_str(&body, "status") {
        let old_status = txt(&cur, "status");
        if new_status == "published" && old_status != "published" {
            sets.push("published_at = ?".into());
            vals.push(s(now()));
        } else if new_status != "published" {
            sets.push("published_at = NULL".into());
        }
    }

    if sets.is_empty() {
        return ok_id(id);
    }

    // 每次实质变更自增版本号，为后续「版本对比 / 回滚」留口子
    sets.push("version = version + 1".into());
    sets.push("updated_at = ?".into());
    vals.push(s(now()));
    vals.push(s(id.clone()));

    let sql = format!("UPDATE contents SET {} WHERE id = ?", sets.join(", "));
    st.db
        .execute(&sql, vals)
        .await
        .map_err(crate::api::db_err("更新内容失败"))?;

    ok_id(id)
}

/// DELETE /api/v1/content/:id（联结记录由外键 ON DELETE CASCADE 清理）
pub async fn remove(State(st): State<AppState>, auth: Auth, Path(id): Path<String>) -> ApiResult {
    ensure(&auth, "domain.content.delete")?;
    must_get(&st, "contents", &id).await?;
    st.db
        .execute("DELETE FROM contents WHERE id = ?", vec![s(id)])
        .await
        .map_err(crate::api::db_err("删除内容失败"))?;
    ok_empty()
}

// ── Content ↔ Context ─────────────────────────────────────────────────────

/// GET /api/v1/content/:id/contexts
pub async fn list_contexts(
    State(st): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
) -> ApiResult {
    ensure(&auth, "domain.content.view")?;
    let rows = st
        .db
        .query_all(
            "SELECT x.id AS id, x.name AS name, x.slug AS slug, x.type AS type,
                    cc.role AS role, cc.weight AS weight, cc.created_at AS created_at
               FROM content_contexts cc
               JOIN contexts x ON x.id = cc.context_id
              WHERE cc.content_id = ?
              ORDER BY cc.weight DESC, x.name ASC",
            vec![s(id)],
        )
        .await
        .map_err(crate::api::db_err("查询 Content 上下文失败"))?;
    let items = rows
        .iter()
        .map(|r| {
            json!({
                "id": txt(r, "id"), "name": txt(r, "name"), "slug": txt(r, "slug"),
                "type": txt(r, "type"), "role": txt(r, "role"),
                "weight": real(r, "weight"), "createdAt": txt(r, "created_at"),
            })
        })
        .collect::<Vec<_>>();
    let total = items.len();
    ok_list(items, total)
}

#[derive(Deserialize)]
pub struct AttachContextReq {
    pub context_id: String,
    pub role: Option<String>,
    pub weight: Option<f64>,
}

/// POST /api/v1/content/:id/contexts —— 挂载语义上下文（带角色与权重）
pub async fn attach_context(
    State(st): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
    Json(body): Json<AttachContextReq>,
) -> ApiResult {
    ensure(&auth, "domain.content.update")?;
    must_get(&st, "contents", &id).await?;
    must_get(&st, "contexts", &body.context_id).await?;

    let role = body.role.unwrap_or_else(|| "primary".into());
    if !crate::entity::CONTEXT_ROLES.contains(&role.as_str()) {
        return Err(ApiError::bad(format!(
            "role 必须是 {} 之一",
            crate::entity::CONTEXT_ROLES.join(" / ")
        )));
    }
    // 权重越界静默收敛，而不是报错打断挂载
    let weight = body.weight.unwrap_or(1.0).clamp(0.0, 10.0);

    // 复合主键 → UPSERT：重复挂载等价于更新角色与权重
    st.db
        .execute(
            "INSERT INTO content_contexts (content_id, context_id, role, weight, created_at)
             VALUES (?,?,?,?,?)
             ON CONFLICT(content_id, context_id)
             DO UPDATE SET role = excluded.role, weight = excluded.weight",
            vec![
                s(id.clone()),
                s(body.context_id.clone()),
                s(role),
                sea_orm::Value::Double(Some(weight)),
                s(now()),
            ],
        )
        .await
        .map_err(crate::api::db_err("挂载上下文失败"))?;

    ok_id(id)
}

/// DELETE /api/v1/content/:id/contexts/:context_id
pub async fn detach_context(
    State(st): State<AppState>,
    auth: Auth,
    Path((id, context_id)): Path<(String, String)>,
) -> ApiResult {
    ensure(&auth, "domain.content.update")?;
    st.db
        .execute(
            "DELETE FROM content_contexts WHERE content_id = ? AND context_id = ?",
            vec![s(id), s(context_id)],
        )
        .await
        .map_err(crate::api::db_err("解除上下文挂载失败"))?;
    ok_empty()
}

// ── Content ↔ Channel ─────────────────────────────────────────────────────

/// GET /api/v1/content/:id/channels
pub async fn list_channels(
    State(st): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
) -> ApiResult {
    ensure(&auth, "domain.content.view")?;
    let rows = st
        .db
        .query_all(
            "SELECT c.id AS id, c.name AS name, c.type AS type, c.slug AS slug,
                    cc.status AS status, cc.external_id AS external_id,
                    cc.external_url AS external_url, cc.published_at AS published_at
               FROM content_channels cc
               JOIN channels c ON c.id = cc.channel_id
              WHERE cc.content_id = ?
              ORDER BY c.name ASC",
            vec![s(id)],
        )
        .await
        .map_err(crate::api::db_err("查询分发状态失败"))?;
    let items = rows
        .iter()
        .map(|r| {
            json!({
                "id": txt(r, "id"), "name": txt(r, "name"), "type": txt(r, "type"),
                "slug": txt(r, "slug"), "status": txt(r, "status"),
                "externalId": opt(r, "external_id"), "externalUrl": opt(r, "external_url"),
                "publishedAt": opt(r, "published_at"),
            })
        })
        .collect::<Vec<_>>();
    let total = items.len();
    ok_list(items, total)
}

/// POST /api/v1/content/:id/channels/:channel_id/publish
///
/// **注意**：这里只登记「已分发」这一事实，不真正调用外部 API。
/// 真实推送应走 `Capability → Policy → Approval → Executor`
/// （见 ai.rs），而不是在 HTTP handler 里直接发请求。
pub async fn publish_to_channel(
    State(st): State<AppState>,
    auth: Auth,
    Path((id, channel_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> ApiResult {
    ensure(&auth, "domain.content.update")?;
    let content = must_get(&st, "contents", &id).await?;
    must_get(&st, "channels", &channel_id).await?;

    // 草稿不允许直接分发——先发布内容，再谈渠道
    if txt(&content, "status") != "published" {
        return Err(ApiError::bad("内容尚未发布，不能分发到渠道"));
    }

    st.db
        .execute(
            "INSERT INTO content_channels
             (content_id, channel_id, external_id, external_url, status, published_at, metadata_json, created_at, updated_at)
             VALUES (?,?,?,?,?,?,?,?,?)
             ON CONFLICT(content_id, channel_id)
             DO UPDATE SET status = excluded.status,
                           external_id = excluded.external_id,
                           external_url = excluded.external_url,
                           published_at = excluded.published_at,
                           updated_at = excluded.updated_at",
            vec![
                s(id.clone()),
                s(channel_id.clone()),
                so(b_str(&body, "externalId")),
                so(b_str(&body, "externalUrl")),
                s(b_str(&body, "status").unwrap_or_else(|| "active".into())),
                s(now()),
                s(to_json_text(b_js(&body, "metadata").as_ref())),
                s(now()),
                s(now()),
            ],
        )
        .await
        .map_err(crate::api::db_err("登记分发失败"))?;

    ok_id(channel_id)
}

/// DELETE /api/v1/content/:id/channels/:channel_id —— 撤回分发
pub async fn unpublish_from_channel(
    State(st): State<AppState>,
    auth: Auth,
    Path((id, channel_id)): Path<(String, String)>,
) -> ApiResult {
    ensure(&auth, "domain.content.update")?;
    st.db
        .execute(
            "DELETE FROM content_channels WHERE content_id = ? AND channel_id = ?",
            vec![s(id), s(channel_id)],
        )
        .await
        .map_err(crate::api::db_err("撤回分发失败"))?;
    ok_empty()
}
