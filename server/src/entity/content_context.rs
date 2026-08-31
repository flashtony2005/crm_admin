//! Content ↔ Context 联结表（多对多 + 角色 + 权重）。
//!
//! 复合主键 `(content_id, context_id)`：同一 Content 对同一 Context
//! 只能有一条关系，避免重复的语义挂载。
//!
//! `weight` 为 REAL，因此本 Model **不能派生 `Eq`**。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "content_contexts")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub content_id: String,
    #[sea_orm(primary_key)]
    pub context_id: String,
    /// primary | secondary | audience | topic | intent
    pub role: String,
    /// 该 Context 对这条 Content 的相关度权重，默认 1.0
    pub weight: f64,
    pub created_at: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::content::Entity",
        from = "Column::ContentId",
        to = "super::content::Column::Id",
        on_delete = "Cascade"
    )]
    Content,
    #[sea_orm(
        belongs_to = "super::context::Entity",
        from = "Column::ContextId",
        to = "super::context::Column::Id",
        on_delete = "Cascade"
    )]
    Context,
}

impl Related<super::content::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Content.def()
    }
}

impl Related<super::context::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Context.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
