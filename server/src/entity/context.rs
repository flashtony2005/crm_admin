//! Context —— Agent 的语义上下文。
//!
//! **不等于传统 CMS 的 Tag**。Tag 只是一个字符串标签；Context 自身携带
//! audience / intent / industry / region 等语义（在 `data_json` 里），
//! 因此 Analyst 可以做「AI + Creator + Education」这类交叉查询，
//! 而不是简单的 `tag = 'AI'` 匹配。
//!
//! `parent_id` 自关联支持层级（如 Creator → Creator Economy）。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "contexts")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: String,
    /// topic | audience | intent | industry | region …
    pub r#type: String,
    pub name: String,
    /// 全站唯一，用于 API 查询（?context=ai）
    pub slug: String,
    pub description: Option<String>,
    /// 语义载荷：{ audience, intent, industry, region, … }
    pub data_json: String,
    pub metadata_json: String,
    /// 父级 Context，可空表示顶层
    pub parent_id: Option<String>,
    /// active | archived
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "Entity",
        from = "Column::ParentId",
        to = "Column::Id",
    )]
    Parent,
    #[sea_orm(has_many = "super::content_context::Entity")]
    ContentContext,
}

impl Related<super::content_context::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ContentContext.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
