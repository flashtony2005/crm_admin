//! 多语言（i18n）：提供翻译字典与语言列表端点。
//! 内容侧的多语言由 articles.locale 列承载（公开列表支持 ?locale= 过滤，见 public_api）。

use axum::{extract::{Path, State}, Json};
use serde_json::{json, Value};

use crate::{error::{ok, ApiResult}, state::AppState};

/// 支持的语言
pub const LOCALES: &[&str] = &["zh", "en"];

/// 翻译字典（前端公开站点使用的 UI 文案）
fn dict(locale: &str) -> Value {
    match locale {
        "en" => json!({
            "nav.home": "Home",
            "nav.articles": "Articles",
            "nav.membership": "Membership",
            "membership.title": "Become a Member",
            "membership.subtitle": "Unlock premium content and supporter perks.",
            "newsletter.title": "Subscribe to our newsletter",
            "newsletter.placeholder": "your@email.com",
            "newsletter.subscribe": "Subscribe",
            "comments.title": "Comments",
            "comments.placeholder": "Share your thoughts…",
            "comments.post": "Post comment",
            "common.loading": "Loading…",
        }),
        _ => json!({
            "nav.home": "首页",
            "nav.articles": "文章",
            "nav.membership": "会员",
            "membership.title": "成为会员",
            "membership.subtitle": "解锁付费内容与专属权益。",
            "newsletter.title": "订阅我们的资讯",
            "newsletter.placeholder": "your@email.com",
            "newsletter.subscribe": "订阅",
            "comments.title": "评论",
            "comments.placeholder": "说点什么…",
            "comments.post": "发表评论",
            "common.loading": "加载中…",
        }),
    }
}

/// GET /api/public/i18n/:locale
pub async fn messages(State(_st): State<AppState>, Path(locale): Path<String>) -> ApiResult {
    let loc = if LOCALES.contains(&locale.as_str()) { locale } else { "zh".into() };
    ok(dict(&loc))
}

/// GET /api/public/locales
pub async fn locales(_st: State<AppState>) -> ApiResult {
    ok(json!(LOCALES))
}
