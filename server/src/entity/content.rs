//! Content —— 业务事实，整个领域模型的核心。
//!
//! V1 **不为每种内容类型建业务表**，统一由 `type` 区分；
//! 类型专属字段全部放 `data_json`，因此后续新增内容类型无需改数据库。
//!
//! 固定字段（type / slug / title / summary / status / locale /
//! author_id / version / published_at）走列，因为它们要参与索引与排序。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "contents")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: String,
    /// profile | article | organization | project | product |
    /// experiment | link | media | event | custom
    pub r#type: String,
    /// 语义化 URL 片段。允许 NULL（如 profile 这类单例内容）
    pub slug: Option<String>,
    pub title: Option<String>,
    pub summary: Option<String>,
    /// draft | review | published | archived
    pub status: String,
    pub locale: String,
    /// 类型专属数据：article 的 body/cover、product 的 category/affiliate_url 等
    pub data_json: String,
    pub metadata_json: String,
    /// 指向 users.id；Agent 生产的内容记录实际生产者
    pub author_id: Option<String>,
    /// 乐观/追溯用的版本号，每次内容变更 +1
    pub version: i32,
    pub published_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::content_context::Entity")]
    ContentContext,
    #[sea_orm(has_many = "super::content_channel::Entity")]
    ContentChannel,
}

impl Related<super::content_context::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ContentContext.def()
    }
}

impl Related<super::content_channel::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ContentChannel.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
