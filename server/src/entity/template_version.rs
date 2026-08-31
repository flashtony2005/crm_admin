//! TemplateVersion —— 真正的页面结构。
//!
//! `definition_json` 遵循 `admin.template.v1`：
//! `{ schema, type, sections: [{ id, component, binding, props }] }`。
//!
//! `binding.source` 决定数据从哪来：
//! - `site.profile`     → 站点身份
//! - `content`          → 按 type 查 Content
//! - `profile.metrics`  → profile 类型内容里的指标
//!
//! 版本一旦发布**不可变**：改结构必须新增版本，而不是 UPDATE 旧版本，
//! 否则已发布的 Renderer 会在无感知的情况下换掉页面。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "template_versions")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: String,
    pub template_id: String,
    /// 同一 template 内递增，UNIQUE(template_id, version)
    pub version: i32,
    /// 页面结构定义（admin.template.v1）
    pub definition_json: String,
    /// draft | published | archived
    pub status: String,
    pub created_at: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::template::Entity",
        from = "Column::TemplateId",
        to = "super::template::Column::Id",
        on_delete = "Cascade"
    )]
    Template,
}

impl Related<super::template::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Template.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
