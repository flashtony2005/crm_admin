//! Content ↔ Channel 联结表：同一份 Content 进入多个分发渠道。
//!
//! 例：Article A 同时进入 Website + X + Newsletter，三条记录各自持有
//! 渠道侧的 `external_id` / `external_url` 与发布状态。
//!
//! 这是后续 Producer / Executor 分发链路的关键基础——
//! 「内容去哪了」是这个表回答的问题，而不是 Content 自己。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "content_channels")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub content_id: String,
    #[sea_orm(primary_key)]
    pub channel_id: String,
    /// 渠道侧的内容 ID（如 X 的 tweet id、Newsletter 的 issue id）
    pub external_id: Option<String>,
    /// 渠道侧可访问地址
    pub external_url: Option<String>,
    /// pending | active | failed | removed
    pub status: String,
    pub published_at: Option<String>,
    /// 渠道专属元数据：排版变体、标签、失败原因等
    pub metadata_json: String,
    pub created_at: String,
    pub updated_at: String,
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
        belongs_to = "super::channel::Entity",
        from = "Column::ChannelId",
        to = "super::channel::Column::Id",
        on_delete = "Cascade"
    )]
    Channel,
}

impl Related<super::content::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Content.def()
    }
}

impl Related<super::channel::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Channel.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
