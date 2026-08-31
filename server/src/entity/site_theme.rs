//! Site ↔ Theme 绑定。
//!
//! 一个站点可挂多套主题（如季节版 / 高对比版），`is_default` 标记生效者。
//! `config_json` 允许站点在不改 Theme 的前提下覆盖个别令牌。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "site_themes")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub site_id: String,
    #[sea_orm(primary_key)]
    pub theme_id: String,
    /// 1 = 当前生效主题
    pub is_default: i32,
    /// 站点级令牌覆盖
    pub config_json: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::site::Entity",
        from = "Column::SiteId",
        to = "super::site::Column::Id",
        on_delete = "Cascade"
    )]
    Site,
    #[sea_orm(
        belongs_to = "super::theme::Entity",
        from = "Column::ThemeId",
        to = "super::theme::Column::Id",
        on_delete = "Cascade"
    )]
    Theme,
}

impl Related<super::site::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Site.def()
    }
}

impl Related<super::theme::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Theme.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
