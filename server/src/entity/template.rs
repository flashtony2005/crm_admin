//! Template —— 决定「怎么组合」。
//!
//! 只放元数据与身份；真正的页面结构在 `template_version.definition_json`
//! （schema `admin.template.v1`）。
//!
//! V1 **刻意不建** components / component_versions / component_props /
//! component_bindings / component_slots 表——那等于重新发明
//! Webflow / Elementor / Builder.io。section + component + binding + props
//! 直接声明在 definition_json 内即可，未来真出现「多模板共用组件 /
//! 组件市场 / 第三方组件」需求再抽 Component Registry。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "templates")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: String,
    pub name: String,
    /// 全站唯一，Renderer 侧按 slug 引用
    pub slug: String,
    /// site | home | article | listing | project | product | landing | custom
    pub r#type: String,
    pub description: Option<String>,
    /// draft | published | archived
    pub status: String,
    pub metadata_json: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::template_version::Entity")]
    TemplateVersion,
    #[sea_orm(has_many = "super::site_template::Entity")]
    SiteTemplate,
}

impl Related<super::template_version::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::TemplateVersion.def()
    }
}

impl Related<super::site_template::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::SiteTemplate.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
