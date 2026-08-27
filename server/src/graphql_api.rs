//! GraphQL API（只读）：暴露文章 / 标签 / 评论 / 套餐 / 会员计数。
//! 经 async-graphql + axum 集成；POST /graphql（JSON）与 GET /graphql（?query=）均可；
//! GET /graphiql 提供交互式调试台。

use async_graphql::{
    Context, EmptyMutation, EmptySubscription, Object, Result as GqlResult, Schema, SimpleObject,
};
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::{extract::State, response::Html};
use sea_orm::Value as SqlValue;

use crate::{state::AppState};

fn sval(s: String) -> SqlValue { SqlValue::String(Some(s)) }

#[derive(SimpleObject)]
pub struct GqlArticle {
    pub id: String,
    pub title: String,
    pub slug: String,
    pub summary: String,
    pub author: String,
    pub tags: String,
    pub published_at: Option<String>,
}

#[derive(SimpleObject)]
pub struct GqlTag {
    pub id: String,
    pub name: String,
    pub slug: String,
}

#[derive(SimpleObject)]
pub struct GqlComment {
    pub id: String,
    pub article_id: String,
    pub author_name: String,
    pub content: String,
    pub created_at: String,
}

#[derive(SimpleObject)]
pub struct GqlTier {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub price_monthly: f64,
}

pub struct Query;

#[Object]
impl Query {
    async fn articles(&self, ctx: &Context<'_>, limit: Option<i32>) -> GqlResult<Vec<GqlArticle>> {
        let st = ctx.data_unchecked::<AppState>();
        let lim = limit.unwrap_or(50).clamp(1, 200) as i64;
        let rows = st
            .db
            .query_all(
                "SELECT id, title, slug, summary, author, tags, published_at FROM articles \
                 WHERE tenant_id = ? AND status = 'published' ORDER BY COALESCE(published_at, updated_at) DESC LIMIT ?",
                vec![sval(st.tenant.clone()), SqlValue::BigInt(Some(lim))],
            )
            .await
            .map_err(|e| async_graphql::Error::new(format!("查询失败：{e}")))?;
        Ok(rows
            .iter()
            .map(|r| GqlArticle {
                id: r.try_get("", "id").unwrap_or_default(),
                title: r.try_get("", "title").unwrap_or_default(),
                slug: r.try_get("", "slug").unwrap_or_default(),
                summary: r.try_get("", "summary").unwrap_or_default(),
                author: r.try_get("", "author").unwrap_or_default(),
                tags: r.try_get("", "tags").unwrap_or_default(),
                published_at: r.try_get::<Option<String>>("", "published_at").ok().flatten(),
            })
            .collect())
    }

    async fn article(&self, ctx: &Context<'_>, id: String) -> GqlResult<Option<GqlArticle>> {
        let st = ctx.data_unchecked::<AppState>();
        let rows = st
            .db
            .query_all(
                "SELECT id, title, slug, summary, author, tags, published_at FROM articles \
                 WHERE tenant_id = ? AND status = 'published' AND (id = ? OR slug = ?) LIMIT 1",
                vec![sval(st.tenant.clone()), sval(id.clone()), sval(id)],
            )
            .await
            .map_err(|e| async_graphql::Error::new(format!("查询失败：{e}")))?;
        Ok(rows.first().map(|r| GqlArticle {
            id: r.try_get("", "id").unwrap_or_default(),
            title: r.try_get("", "title").unwrap_or_default(),
            slug: r.try_get("", "slug").unwrap_or_default(),
            summary: r.try_get("", "summary").unwrap_or_default(),
            author: r.try_get("", "author").unwrap_or_default(),
            tags: r.try_get("", "tags").unwrap_or_default(),
            published_at: r.try_get::<Option<String>>("", "published_at").ok().flatten(),
        }))
    }

    async fn tags(&self, ctx: &Context<'_>) -> GqlResult<Vec<GqlTag>> {
        let st = ctx.data_unchecked::<AppState>();
        let rows = st
            .db
            .query_all("SELECT id, name, slug FROM tags WHERE tenant_id = ? ORDER BY name LIMIT 200",
                vec![sval(st.tenant.clone())])
            .await
            .map_err(|e| async_graphql::Error::new(format!("查询失败：{e}")))?;
        Ok(rows
            .iter()
            .map(|r| GqlTag {
                id: r.try_get("", "id").unwrap_or_default(),
                name: r.try_get("", "name").unwrap_or_default(),
                slug: r.try_get("", "slug").unwrap_or_default(),
            })
            .collect())
    }

    async fn comments(&self, ctx: &Context<'_>, article_id: String) -> GqlResult<Vec<GqlComment>> {
        let st = ctx.data_unchecked::<AppState>();
        let rows = st
            .db
            .query_all(
                "SELECT id, article_id, author_name, content, created_at FROM comments \
                 WHERE tenant_id = ? AND article_id = ? AND status = 'approved' ORDER BY created_at ASC",
                vec![sval(st.tenant.clone()), sval(article_id)],
            )
            .await
            .map_err(|e| async_graphql::Error::new(format!("查询失败：{e}")))?;
        Ok(rows
            .iter()
            .map(|r| GqlComment {
                id: r.try_get("", "id").unwrap_or_default(),
                article_id: r.try_get("", "article_id").unwrap_or_default(),
                author_name: r.try_get("", "author_name").unwrap_or_default(),
                content: r.try_get("", "content").unwrap_or_default(),
                created_at: r.try_get("", "created_at").unwrap_or_default(),
            })
            .collect())
    }

    async fn tiers(&self, ctx: &Context<'_>) -> GqlResult<Vec<GqlTier>> {
        let st = ctx.data_unchecked::<AppState>();
        let rows = st
            .db
            .query_all("SELECT id, name, slug, price_monthly FROM tiers WHERE tenant_id = ? AND active = 1",
                vec![sval(st.tenant.clone())])
            .await
            .map_err(|e| async_graphql::Error::new(format!("查询失败：{e}")))?;
        Ok(rows
            .iter()
            .map(|r| GqlTier {
                id: r.try_get("", "id").unwrap_or_default(),
                name: r.try_get("", "name").unwrap_or_default(),
                slug: r.try_get("", "slug").unwrap_or_default(),
                price_monthly: r.try_get::<f64>("", "price_monthly").unwrap_or(0.0),
            })
            .collect())
    }

    async fn members_count(&self, ctx: &Context<'_>) -> GqlResult<i64> {
        let st = ctx.data_unchecked::<AppState>();
        let row = st
            .db
            .query_one("SELECT COUNT(*) AS n FROM members WHERE tenant_id = ?", vec![sval(st.tenant.clone())])
            .await
            .map_err(|e| async_graphql::Error::new(format!("查询失败：{e}")))?;
        Ok(row.and_then(|r| r.try_get::<i64>("", "n").ok()).unwrap_or(0))
    }
}

pub type CmsSchema = Schema<Query, EmptyMutation, EmptySubscription>;

pub fn build_schema(st: AppState) -> CmsSchema {
    Schema::build(Query, EmptyMutation, EmptySubscription)
        .data(st)
        .finish()
}

/// POST /graphql 与 GET /graphql?query= —— 统一入口（State 中构建 schema）
pub async fn graphql_handler(
    State(st): State<AppState>,
    req: GraphQLRequest,
) -> GraphQLResponse {
    let schema = build_schema(st);
    schema.execute(req.into_inner()).await.into()
}

/// GET /graphiql —— 交互式调试台
pub async fn graphiql() -> Html<String> {
    Html(async_graphql::http::GraphiQLSource::build().endpoint("/graphql").finish())
}
