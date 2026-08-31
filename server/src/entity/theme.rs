//! Theme —— 决定「怎么呈现」。
//!
//! 与 Template 的分工：Template 决定结构怎么组合，Theme 决定长什么样。
//! 这里只有元数据，真正的设计令牌在 `theme_version.tokens_json`。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "themes")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: String,
    pub name: String,
    /// 全站唯一
    pub slug: String,
    pub description: Option<String>,
    /// draft | published | archived
    pub status: String,
    pub metadata_json: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::theme_version::Entity")]
    ThemeVersion,
    #[sea_orm(has_many = "super::site_theme::Entity")]
    SiteTheme,
}

impl Related<super::theme_version::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ThemeVersion.def()
    }
}

impl Related<super::site_theme::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::SiteTheme.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
