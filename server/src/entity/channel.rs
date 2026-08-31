//! Channel —— 分发出口。
//!
//! 表示「在哪里分发」，**不是「网站的一部分」**。
//! 同一份 Content 可以同时进入 Website / X / Newsletter 等多个 Channel，
//! 关联见 `content_channel`。
//!
//! `provider` + `external_id` 联合唯一，防止同一外部渠道重复登记；
//! 纯本地渠道两者皆为 NULL（SQLite 下 NULL 不参与唯一性判定）。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "channels")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: String,
    pub name: String,
    /// website | x | wechat | telegram | newsletter | youtube |
    /// tiktok | xiaohongshu | api | custom
    pub r#type: String,
    /// 具体平台标识（如 twitter / telegram-bot），本地渠道为空
    pub provider: Option<String>,
    /// 平台侧的账号或频道 ID
    pub external_id: Option<String>,
    pub url: Option<String>,
    /// 凭据、默认排版、发布参数等；不落库到代码
    pub config_json: String,
    pub metadata_json: String,
    /// active | paused | archived
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::content_channel::Entity")]
    ContentChannel,
}

impl Related<super::content_channel::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ContentChannel.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
