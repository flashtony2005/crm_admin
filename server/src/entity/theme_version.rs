//! ThemeVersion —— 设计令牌与组件样式。
//!
//! `tokens_json` 遵循 `admin.theme.v1`：
//! `{ schema, tokens: { color, typography, spacing, layout, radius } }`
//! `components_json` 放组件级样式覆盖（可选）。
//!
//! 站点只依赖 Renderer 消费这些令牌，因此**外部网站不必依赖 Admin UI**，
//! Next / Astro / Vue / Svelte 都能作为 Renderer。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "theme_versions")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: String,
    pub theme_id: String,
    /// 同一 theme 内递增，UNIQUE(theme_id, version)
    pub version: i32,
    /// 设计令牌（admin.theme.v1）
    pub tokens_json: String,
    /// 组件级样式覆盖
    pub components_json: String,
    /// draft | published | archived
    pub status: String,
    pub created_at: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::theme::Entity",
        from = "Column::ThemeId",
        to = "super::theme::Column::Id",
        on_delete = "Cascade"
    )]
    Theme,
}

impl Related<super::theme::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Theme.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
