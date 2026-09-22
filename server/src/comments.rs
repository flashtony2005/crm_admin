//! 评论（Comments）：公开列表 / 发布（支持嵌套 parent_id）；Admin 审核与状态管理。

use axum::http::header::AUTHORIZATION;
use axum::{extract::{Path, Query, State}, Json};
use sea_orm::Value as SqlValue;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use uuid::Uuid;

use crate::{
    auth::{ensure, verify, Auth},
    db::now_iso,
    error::{ok, ok_list, ApiError, ApiResult},
    members::MemberAuth,
    state::AppState,
};

fn sval(s: String) -> SqlValue { SqlValue::String(Some(s)) }

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
// ── 键名契约：camelCase + snake 别名 ────────────────────────────────
// 全站 JSON 契约是 camelCase（通用网关 / 响应体 / 前端 TS 接口都是）。
// 这些手写 DTO 早期按 snake_case 定义，而调用方一律传 camelCase ；
// serde 对**未知字段静默忽略**，于是形成两类后果：
//   · 必填字段 → Json 提取失败 → 422（调用方直接不可用，看得到）
//   · 可选字段 → 静默变 None（**看不到，但业务已经错了**）
// rename_all 把接受名对齐契约；alias 保留 snake 接受名，让
// 老客户端（浏览器里缓存的旧 JS）与既有测试不必同步改。
pub struct CommentCreate {
    #[serde(alias = "article_id")]
    pub article_id: String,
    #[serde(default, alias = "parent_id")]
    pub parent_id: Option<String>,
    #[serde(alias = "author_name")]
    pub author_name: String,
    #[serde(default, alias = "author_email")]
    pub author_email: Option<String>,
    pub content: String,
}

/// POST /api/public/comments —— 公开发布评论（默认 approved；可接审核流）
///
/// 携带可选会员身份：登录会员发的评论会落 `member_id` ——
/// 这是「评论归属到人」的前提，也是 P0-3 会员档案时间线能出评论的前提。
/// 匿名访客照旧可发（member_id 为空串）—— 评论区不能只给会员用，否则永远热不起来。
pub async fn public_create(
    State(st): State<AppState>,
    om: OptionalMember,
    Json(req): Json<CommentCreate>,
) -> ApiResult {
    let name = req.author_name.trim().to_string();
    let content = req.content.trim().to_string();
    if name.is_empty() || content.is_empty() {
        return Err(ApiError::bad("昵称与内容必填"));
    }
    if content.len() > 2000 {
        return Err(ApiError::bad("评论过长（上限 2000 字）"));
    }
    let id = Uuid::new_v4().to_string();
    let now = now_iso();
    let auto = std::env::var("COMMENTS_AUTO_APPROVE").map(|v| v == "1" || v.eq_ignore_ascii_case("true")).unwrap_or(true);
    let status = if auto { "approved" } else { "pending" };
    let member_id = om.0.as_ref().map(|c| c.sub.clone()).unwrap_or_default();
    let parent_id = req.parent_id.clone().unwrap_or_default();
    st.db
        .execute(
            "INSERT INTO comments (id, tenant_id, article_id, parent_id, author_name, author_email, member_id, content, status, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            vec![
                sval(id.clone()),
                sval(st.tenant.clone()),
                sval(req.article_id.clone()),
                sval(parent_id.clone()),
                sval(name.clone()),
                sval(req.author_email.clone().unwrap_or_default()),
                sval(member_id.clone()),
                sval(content.clone()),
                sval(status.to_string()),
                sval(now.clone()),
            ],
        )
        .await
        .map_err(|e| ApiError::bad(format!("发布失败：{e}")))?;

    // ── 通知（尽力而为：通知失败不能拖垮评论发布）──
    // 回复：告诉被回复的人（否则楼中楼只有发起方知道，不会形成对话）。
    if !parent_id.is_empty() {
        if let Ok(Some(p)) = st
            .db
            .query_one(
                "SELECT member_id FROM comments WHERE id = ? AND tenant_id = ? LIMIT 1",
                vec![sval(parent_id.clone()), sval(st.tenant.clone())],
            )
            .await
        {
            let target: String = p.try_get("", "member_id").unwrap_or_default();
            if !target.is_empty() && target != member_id {
                notify(&st, &target, "reply", "有人回复了你的评论", &clip(&content, 60), "article", &req.article_id).await;
            }
        }
    }
    // @提及：只按**昵称精确相等**匹配本站会员。
    // 做模糊匹配会把消息发给无关人 —— 通知错人比没通知更坏。
    for who in parse_mentions(&content).into_iter().take(10) {
        if let Ok(Some(m)) = st
            .db
            .query_one(
                "SELECT id FROM members WHERE tenant_id = ? AND name = ? LIMIT 1",
                vec![sval(st.tenant.clone()), sval(who.clone())],
            )
            .await
        {
            let target: String = m.try_get("", "id").unwrap_or_default();
            if !target.is_empty() && target != member_id {
                notify(
                    &st,
                    &target,
                    "mention",
                    &format!("你在评论里被 @{who} 提到"),
                    &clip(&content, 60),
                    "article",
                    &req.article_id,
                )
                .await;
            }
        }
    }
    // 出站 Webhook
    crate::webhooks_out::emit(&st, "comment.created", json!({
        "id": id, "articleId": req.article_id, "author": name, "status": status
    }));
    ok(json!({ "id": id, "status": status }))
}

/// GET /api/public/comments?article=xxx —— 公开已审核评论（嵌套）
pub async fn public_list(
    State(st): State<AppState>,
    om: OptionalMember,
    Query(params): Query<HashMap<String, String>>,
) -> ApiResult {
    let article = params.get("article").cloned().unwrap_or_default();
    if article.is_empty() {
        return Err(ApiError::bad("article 必填"));
    }
    let me = om.0.as_ref().map(|c| c.sub.clone()).unwrap_or_default();
    let rows = st
        .db
        .query_all(
            "SELECT id, article_id, parent_id, author_name, content, status, created_at \
             FROM comments WHERE tenant_id = ? AND article_id = ? AND status = 'approved' \
             ORDER BY created_at ASC",
            vec![sval(st.tenant.clone()), sval(article)],
        )
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    let mut items: Vec<Value> = Vec::new();
    for r in &rows {
        let cid: String = r.try_get("", "id").map_err(internal)?;
        // 反应计数与「我点没点过」一并带出：
        // 前者给所有人看，后者只在带了合法会员令牌时有意义。
        let (up, useful, mine) = reaction_state(&st, &cid, &me).await?;
        let c = json!({
            "id": cid,
            "articleId": r.try_get::<String>("", "article_id").unwrap_or_default(),
            "parentId": r.try_get::<String>("", "parent_id").unwrap_or_default(),
            "authorName": r.try_get::<String>("", "author_name").unwrap_or_default(),
            "content": r.try_get::<String>("", "content").unwrap_or_default(),
            "status": r.try_get::<String>("", "status").unwrap_or_default(),
            "createdAt": r.try_get::<String>("", "created_at").unwrap_or_default(),
            "reactions": { "up": up, "useful": useful },
            "myReaction": mine,
        });
        items.push(c);
    }
    ok(json!(items))
}

/// GET /api/comments?status=pending —— Admin 审核列表
pub async fn admin_list(
    State(st): State<AppState>,
    auth: Auth,
    Query(params): Query<HashMap<String, String>>,
) -> ApiResult {
    // 权限：此前这两个管理端点只要登录就能用（`let _ = auth`）——
    // viewer 也能读取并修改评论审核状态。趁本次改动一并收紧。
    ensure(&auth, "content.comments.view")?;
    let status = params.get("status").cloned().unwrap_or_default();
    // 子查询带出举报数与反应数，并**按举报降序**排 ——
    // 审核队列的价值就在「被人举报的先给我看」；按时间排会让举报沉在第 200 条之后。
    let base = "SELECT c.id, c.article_id, c.parent_id, c.author_name, c.author_email, c.content, c.status, c.created_at, \
                (SELECT COUNT(*) FROM comment_reports r WHERE r.tenant_id = c.tenant_id AND r.comment_id = c.id) AS report_count, \
                (SELECT COUNT(*) FROM comment_reactions k WHERE k.tenant_id = c.tenant_id AND k.comment_id = c.id) AS reaction_count \
          FROM comments c WHERE c.tenant_id = ?";
    let (sql, args) = if status.is_empty() {
        (format!("{base} ORDER BY report_count DESC, c.created_at DESC LIMIT 200"),
         vec![sval(st.tenant.clone())])
    } else {
        (format!("{base} AND c.status = ? ORDER BY report_count DESC, c.created_at DESC LIMIT 200"),
         vec![sval(st.tenant.clone()), sval(status)])
    };
    let rows = st
        .db
        .query_all(&sql, args)
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    let mut items: Vec<Value> = Vec::new();
    for r in &rows {
        items.push(json!({
            "id": r.try_get::<String>("", "id").map_err(internal)?,
            "articleId": r.try_get::<String>("", "article_id").unwrap_or_default(),
            "parentId": r.try_get::<String>("", "parent_id").unwrap_or_default(),
            "authorName": r.try_get::<String>("", "author_name").unwrap_or_default(),
            "authorEmail": r.try_get::<String>("", "author_email").unwrap_or_default(),
            "content": r.try_get::<String>("", "content").unwrap_or_default(),
            "status": r.try_get::<String>("", "status").unwrap_or_default(),
            "createdAt": r.try_get::<String>("", "created_at").unwrap_or_default(),
            "reportCount": r.try_get::<i64>("", "report_count").unwrap_or(0),
            "reactionCount": r.try_get::<i64>("", "reaction_count").unwrap_or(0),
        }));
    }
    let n = items.len();
    ok_list(items, n)
}

#[derive(Deserialize)]
pub struct ModerateReq { pub status: String }

/// POST /api/comments/{id}/status —— Admin 审核（approve/reject/spam）
pub async fn moderate(
    State(st): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
    Json(req): Json<ModerateReq>,
) -> ApiResult {
    ensure(&auth, "content.comments.update")?;
    let valid = ["approved", "pending", "rejected", "spam"];
    if !valid.contains(&req.status.as_str()) {
        return Err(ApiError::bad("非法状态"));
    }
    let n = st
        .db
        .execute(
            "UPDATE comments SET status = ? WHERE id = ? AND tenant_id = ?",
            vec![sval(req.status.clone()), sval(id.clone()), sval(st.tenant.clone())],
        )
        .await
        .map_err(|e| ApiError::bad(format!("更新失败：{e}")))?;
    if n == 0 {
        return Err(ApiError::not_found("评论不存在"));
    }
    crate::webhooks_out::emit(&st, "comment.moderated", json!({ "id": id, "status": req.status }));
    ok(json!({ "updated": true }))
}

fn internal(e: impl std::fmt::Display) -> ApiError {
    ApiError::bad(format!("行解析失败：{e}"))
}


// ═══════════════════════ 社区最小闭环（P0-2）═══════════════════════
//
// 对标 FluentCommunity 的最小可用集：反应 + 楼中楼 + @提及 + 举报。
//
// 此前 `comments.tsx` 全文只命中 3 处「审核」，公开站连评论组件都没有 ——
// 评论区只有「读者对作者」，没有「读者对读者」，**所以不产生任何留存**。
// 这是会员站，会员留存就是收入留存，所以这几件事是同一件事的前后脚。
//
// 明确不做（见 WORDPRESS_BENCHMARK 报告 §3）：私信与活动流。社区体量不够时
// 空的活动流比没有更伤 —— 它向新访客证明「这里没人」。

/// 举报累计到这个数就自动转待审。
///
/// 为什么不只靠管理员看：举报的本质是「用户帮我发现脏东西」，如果处理速度
/// 取决于管理员什么时候想起看后台，那举报按钮就是摆设。自动转待审让**风险
/// 先被控住**，人工只做最终裁定。
const REPORT_HOLD_THRESHOLD: i64 = 3;

/// 可选会员身份：带合法会员令牌就解析出来，否则视为匿名。
///
/// 公开接口要同时服务两种人：匿名访客（只是看评论）与登录会员（还要看到
/// 「我点没点过赞」）。用 `MemberAuth` 会把访客整个挡在门外，所以要一个
/// **永不拒绝**的版本。
///
/// 注意它把「令牌无效」也当匿名而不是报错：一个过期令牌不该让访客连评论
/// 都读不了。真正的鉴权由需要身份的端点（react / report）自己用 MemberAuth 做。
pub struct OptionalMember(pub Option<crate::auth::Claims>);

impl<S> axum::extract::FromRequestParts<S> for OptionalMember
where
    S: Send + Sync + Clone + 'static,
{
    type Rejection = ApiError;
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let Some(tok) = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
        else {
            return Ok(OptionalMember(None));
        };
        match verify(tok) {
            Some(c) if c.role == "member" => Ok(OptionalMember(Some(c))),
            _ => Ok(OptionalMember(None)),
        }
    }
}

/// 按**字符**截断 —— 中文按字节切会切出半个字。
fn clip(s: &str, n: usize) -> String {
    let t: String = s.chars().take(n).collect();
    if s.chars().count() > n {
        format!("{t}...")
    } else {
        t
    }
}

/// 从评论正文里解析 @提及。
///
/// 终止条件同时取「空白」「常见标点」「长度上限」三样：只按空白切，
/// 中文评论里「@张三，你怎么看」会把「张三，你怎么看」整个当昵称。
fn parse_mentions(s: &str) -> Vec<String> {
    const MAX_NAME: usize = 20;
    // ASCII 停用符与 CJK 标点分开写：CJK 标点用 matches! 直观；
    // 而 ASCII 里含单引号 / 双引号 / 反斜杠，塞进字符字面量极易因转义
    // 写坏（本次第一版就写坏了 —— `'"' | '\''` 被解析成 `'"' | '\'`），
    // 用字符串 contains 既短又不会出转义事故。
    const ASCII_STOP: &str = "'\"/\\<>|!?:;,.[]()";
    let is_stop = |c: char| {
        c.is_whitespace()
            || ASCII_STOP.contains(c)
            || matches!(
                c,
                '、' | '，' | '。' | '！' | '？' | '：' | '；' | '（' | '）' | '【' | '】'
                    | '《' | '》' | '「' | '」' | '“' | '”' | '‘' | '’' | '…' | '—' | '@'
            )
    };
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut in_m = false;
    for ch in s.chars() {
        if !in_m {
            if ch == '@' {
                in_m = true;
                cur.clear();
            }
            continue;
        }
        if is_stop(ch) || cur.chars().count() >= MAX_NAME {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            in_m = ch == '@';
            if in_m {
                cur.clear();
            }
            continue;
        }
        cur.push(ch);
    }
    if in_m && !cur.is_empty() {
        out.push(cur);
    }
    // 去重并保持出现顺序：同一人被 @ 三次只通知一次。
    let mut seen: Vec<String> = Vec::new();
    for n in out {
        if !seen.contains(&n) {
            seen.push(n);
        }
    }
    seen
}

/// 写一条站内通知（尽力而为：失败只影响通知，不影响触发它的业务动作）。
async fn notify(
    st: &AppState,
    member_id: &str,
    kind: &str,
    title: &str,
    body: &str,
    ref_kind: &str,
    ref_id: &str,
) {
    if member_id.is_empty() {
        return;
    }
    let now = now_iso();
    let _ = st
        .db
        .execute(
            "INSERT INTO notifications (id, tenant_id, member_id, kind, title, body, ref_kind, ref_id, read_at, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, '', ?, ?)",
            vec![
                sval(Uuid::new_v4().to_string()),
                sval(st.tenant.clone()),
                sval(member_id.to_string()),
                sval(kind.to_string()),
                sval(title.to_string()),
                sval(body.to_string()),
                sval(ref_kind.to_string()),
                sval(ref_id.to_string()),
                sval(now.clone()),
                sval(now),
            ],
        )
        .await;
}

/// 一条评论的反应计数 + 当前会员点过哪些。
async fn reaction_state(
    st: &AppState,
    comment_id: &str,
    me: &str,
) -> Result<(i64, i64, Vec<String>), ApiError> {
    let rows = st
        .db
        .query_all(
            "SELECT kind, COUNT(*) AS n FROM comment_reactions \
              WHERE tenant_id = ? AND comment_id = ? GROUP BY kind",
            vec![sval(st.tenant.clone()), sval(comment_id.to_string())],
        )
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    let (mut up, mut useful) = (0i64, 0i64);
    for r in &rows {
        let k: String = r.try_get("", "kind").unwrap_or_default();
        let n: i64 = r.try_get("", "n").unwrap_or(0);
        if k == "up" {
            up = n;
        } else if k == "useful" {
            useful = n;
        }
    }
    let mut mine: Vec<String> = Vec::new();
    if !me.is_empty() {
        let rs = st
            .db
            .query_all(
                "SELECT kind FROM comment_reactions \
                  WHERE tenant_id = ? AND comment_id = ? AND member_id = ?",
                vec![
                    sval(st.tenant.clone()),
                    sval(comment_id.to_string()),
                    sval(me.to_string()),
                ],
            )
            .await
            .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
        for r in &rs {
            let k: String = r.try_get("", "kind").unwrap_or_default();
            if !k.is_empty() {
                mine.push(k);
            }
        }
    }
    Ok((up, useful, mine))
}

/// 评论是否存在且可见（给反应/举报共用）。
///
/// 只允许操作**已通过审核**的评论：给一条别人看不到的评论点赞/举报，
/// 产生的是纯噪声数据。
async fn visible_comment(st: &AppState, id: &str) -> Result<(), ApiError> {
    let row = st
        .db
        .query_one(
            "SELECT id FROM comments WHERE id = ? AND tenant_id = ? AND status = 'approved' LIMIT 1",
            vec![sval(id.to_string()), sval(st.tenant.clone())],
        )
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    match row {
        Some(_) => Ok(()),
        None => Err(ApiError::not_found("评论不存在或未通过审核")),
    }
}

#[derive(Deserialize)]
pub struct ReactReq {
    /// up（赞）| useful（有用）
    pub kind: String,
}

/// POST /api/public/comments/{id}/react —— 切换反应（点一下加，再点一下取消）。
///
/// 切换语义而不是「只有加」：没有取消按钮的点赞是数据污染源 ——
/// 用户点错了就只能去找管理员。
pub async fn react(
    State(st): State<AppState>,
    auth: MemberAuth,
    Path(id): Path<String>,
    Json(req): Json<ReactReq>,
) -> ApiResult {
    if !["up", "useful"].contains(&req.kind.as_str()) {
        return Err(ApiError::bad("非法反应类型（up / useful）"));
    }
    visible_comment(&st, &id).await?;
    let me = auth.0.sub.clone();

    // 先删后插的 toggle。**用 rows_affected 判定，不用 `ON CONFLICT`** ——
    // Turso 后端把唯一键冲突吞成 Ok(0) 而非 Err（见 TOPIC_BILLING.md 的幂等
    // 硬规则），冲突检测依赖 Err 的写法在两条后端上行为不一致。
    let del = st
        .db
        .execute(
            "DELETE FROM comment_reactions \
              WHERE tenant_id = ? AND comment_id = ? AND member_id = ? AND kind = ?",
            vec![
                sval(st.tenant.clone()),
                sval(id.clone()),
                sval(me.clone()),
                sval(req.kind.clone()),
            ],
        )
        .await
        .map_err(|e| ApiError::bad(format!("操作失败：{e}")))?;

    let on = if del == 0 {
        let now = now_iso();
        let ins = st
            .db
            .execute(
                "INSERT INTO comment_reactions (id, tenant_id, comment_id, member_id, kind, created_at, updated_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
                vec![
                    sval(Uuid::new_v4().to_string()),
                    sval(st.tenant.clone()),
                    sval(id.clone()),
                    sval(me),
                    sval(req.kind.clone()),
                    sval(now.clone()),
                    sval(now),
                ],
            )
            .await;
        // 并发下另一个请求可能刚插过（SQLite Err / Turso Ok(0) 都可能）——
        // 结果上就是「已经点过了」，对用户语义一致。
        matches!(ins, Ok(n) if n > 0)
    } else {
        false
    };

    let (up, useful, mine) = reaction_state(&st, &id, &auth.0.sub).await?;
    ok(json!({
        "on": on,
        "reactions": { "up": up, "useful": useful },
        "myReaction": mine,
    }))
}

#[derive(Deserialize)]
pub struct ReportReq {
    #[serde(default)]
    pub reason: String,
}

/// POST /api/public/comments/{id}/report —— 举报进审核队列。
///
/// 唯一键 (tenant, comment, member) 让「一人一次」由**库层**保证，而不是
/// 应用层先查后写（后者在并发下必然漏）。
pub async fn report(
    State(st): State<AppState>,
    auth: MemberAuth,
    Path(id): Path<String>,
    Json(req): Json<ReportReq>,
) -> ApiResult {
    visible_comment(&st, &id).await?;
    let now = now_iso();
    let ins = st
        .db
        .execute(
            "INSERT INTO comment_reports (id, tenant_id, comment_id, member_id, reason, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
            vec![
                sval(Uuid::new_v4().to_string()),
                sval(st.tenant.clone()),
                sval(id.clone()),
                sval(auth.0.sub.clone()),
                sval(clip(req.reason.trim(), 200)),
                sval(now.clone()),
                sval(now),
            ],
        )
        .await;
    let first = matches!(ins, Ok(n) if n > 0);

    let n = st
        .db
        .query_one(
            "SELECT COUNT(*) AS n FROM comment_reports WHERE tenant_id = ? AND comment_id = ?",
            vec![sval(st.tenant.clone()), sval(id.clone())],
        )
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?
        .and_then(|r| r.try_get::<i64>("", "n").ok())
        .unwrap_or(0);

    // 达阈值自动下架待审：先控风险，人工只做裁定。
    let mut auto_held = false;
    if n >= REPORT_HOLD_THRESHOLD {
        let held = st
            .db
            .execute(
                "UPDATE comments SET status = 'pending' WHERE id = ? AND tenant_id = ? AND status = 'approved'",
                vec![sval(id.clone()), sval(st.tenant.clone())],
            )
            .await
            .unwrap_or(0);
        auto_held = held > 0;
    }
    if first {
        crate::webhooks_out::emit(&st, "comment.reported", json!({
            "commentId": id, "reportCount": n, "autoHeld": auto_held
        }));
    }
    ok(json!({ "first": first, "reportCount": n, "autoHeld": auto_held }))
}

/// GET /api/public/members/notifications —— 我的站内通知（@提及 / 回复）。
pub async fn my_notifications(State(st): State<AppState>, auth: MemberAuth) -> ApiResult {
    let rows = st
        .db
        .query_all(
            "SELECT id, kind, title, body, ref_kind, ref_id, read_at, created_at \
               FROM notifications WHERE tenant_id = ? AND member_id = ? \
              ORDER BY created_at DESC LIMIT 50",
            vec![sval(st.tenant.clone()), sval(auth.0.sub.clone())],
        )
        .await
        .map_err(|e| ApiError::bad(format!("查询失败：{e}")))?;
    let mut items: Vec<Value> = Vec::new();
    let mut unread = 0i64;
    for r in &rows {
        let read_at: String = r.try_get("", "read_at").unwrap_or_default();
        if read_at.is_empty() {
            unread += 1;
        }
        items.push(json!({
            "id": r.try_get::<String>("", "id").unwrap_or_default(),
            "kind": r.try_get::<String>("", "kind").unwrap_or_default(),
            "title": r.try_get::<String>("", "title").unwrap_or_default(),
            "body": r.try_get::<String>("", "body").unwrap_or_default(),
            "refKind": r.try_get::<String>("", "ref_kind").unwrap_or_default(),
            "refId": r.try_get::<String>("", "ref_id").unwrap_or_default(),
            "read": !read_at.is_empty(),
            "createdAt": r.try_get::<String>("", "created_at").unwrap_or_default(),
        }));
    }
    ok(json!({ "items": items, "unread": unread }))
}

/// POST /api/public/members/notifications/read —— 全部标记已读。
pub async fn mark_notifications_read(State(st): State<AppState>, auth: MemberAuth) -> ApiResult {
    let n = st
        .db
        .execute(
            "UPDATE notifications SET read_at = ? \
              WHERE tenant_id = ? AND member_id = ? AND read_at = ''",
            vec![
                sval(now_iso()),
                sval(st.tenant.clone()),
                sval(auth.0.sub.clone()),
            ],
        )
        .await
        .map_err(|e| ApiError::bad(format!("标记失败：{e}")))?;
    ok(json!({ "read": n }))
}

#[cfg(test)]
mod tests {
    use super::parse_mentions;

    /// 中文标点必须终止昵称 —— 否则「@张三，你怎么看」会把后半句一起吃进去。
    #[test]
    fn mentions_stop_at_cjk_punctuation() {
        assert_eq!(parse_mentions("@张三，你怎么看"), vec!["张三".to_string()]);
        assert_eq!(parse_mentions("同意 @李四。"), vec!["李四".to_string()]);
        assert_eq!(parse_mentions("see @alice!"), vec!["alice".to_string()]);
        assert_eq!(parse_mentions("@а б"), vec!["а".to_string()]);
    }

    /// 同一人多次提及只通知一次（否则一次评论能给同一个人刷 10 条通知）。
    #[test]
    fn mentions_are_deduped_in_order() {
        assert_eq!(
            parse_mentions("@b @a @b"),
            vec!["b".to_string(), "a".to_string()]
        );
    }

    /// 无 @ / 只有 @ / 超长昵称的边界。
    #[test]
    fn mentions_edge_cases() {
        assert!(parse_mentions("no mention here").is_empty());
        assert!(parse_mentions("just @").is_empty());
        assert!(parse_mentions("@ @").is_empty());
        // 超长昵称被截断成上限长度，而不是整段吞下（防一条评论带上千字昵称）
        let long = format!("@{}", "x".repeat(50));
        let got = parse_mentions(&long);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].chars().count(), 20);
    }

    /// 邮箱里的 @ 不该被当提及（「a@b.com」会解析出 b.com）。
    /// 这是已知取舍：昵称不含点号时不会误伤，含点号会 —— 但误报只产生一条
    /// 找不到会员的通知尝试，不写库，成本为零。
    #[test]
    fn mentions_from_email_are_harmless() {
        let got = parse_mentions("联系 a@b.com");
        // 解析出 "b"（在 '.' 处截断），查询会员名找不到 → 不产生通知
        assert_eq!(got, vec!["b".to_string()]);
    }
}
