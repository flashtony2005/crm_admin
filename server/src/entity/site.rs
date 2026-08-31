//! Site —— 呈现载体。
//!
//! 只承载身份（name / slug / domain）、本地化（default_locale / timezone）
//! 与配置（settings_json）；**不内联** template_json / theme_json /
//! content_json——呈现绑定一律走 `site_templates` / `site_themes`。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "sites")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: String,
    pub name: String,
    /// 人类可读标识，全站唯一；用于 API 路径（/api/v1/sites/{slug}）
    pub slug: String,
    /// 绑定的主域名，可空（尚未接入域名时）
    pub domain: Option<String>,
    pub description: Option<String>,
    pub default_locale: String,
    pub timezone: String,
    /// draft | active | archived
    pub status: String,
    pub settings_json: String,
    pub metadata_json: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::site_template::Entity")]
    SiteTemplate,
    #[sea_orm(has_many = "super::site_theme::Entity")]
    SiteTheme,
}

impl Related<super::site_template::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::SiteTemplate.def()
    }
}

impl Related<super::site_theme::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::SiteTheme.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
