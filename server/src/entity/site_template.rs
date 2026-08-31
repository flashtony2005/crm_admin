//! Site ↔ Template 绑定：路由到模板。
//!
//! 例：CouCouYa 站点下
//! - `/`               → PersonalBrandHome
//! - `/article/:slug`  → Article
//! - `/project/:slug`  → Project
//!
//! 主键为 `(site_id, template_id, route)`：同一路由允许挂多个模板
//! （A/B 或多语言变体），由 `is_default` 标记实际生效者。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "site_templates")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub site_id: String,
    #[sea_orm(primary_key)]
    pub template_id: String,
    #[sea_orm(primary_key)]
    /// 路由模式，如 `/`、`/article/:slug`
    pub route: String,
    /// 1 = 该路由的生效模板
    pub is_default: i32,
    /// 站点侧覆盖配置：区块开关、每页条数等
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
        belongs_to = "super::template::Entity",
        from = "Column::TemplateId",
        to = "super::template::Column::Id",
        on_delete = "Cascade"
    )]
    Template,
}

impl Related<super::site::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Site.def()
    }
}

impl Related<super::template::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Template.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
