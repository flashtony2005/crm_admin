//! 文件上传：multipart 接收 → 保存到 uploads/ → 返回公开 URL。
//!
//! 设计要点：
//! - 字段名固定为 `file`（支持一次多文件：循环 next_field）
//! - 文件名用 UUID 重命名（防冲突 / 防路径注入），保留原扩展名
//! - 写入 cwd 下的 `uploads/`（可用环境变量 UPLOAD_DIR 覆盖）
//! - 需 `content.media.upload` 权限；单文件上限 20MB（与全局请求体一致）

use axum::extract::{Multipart, State};
use serde_json::json;
use std::path::PathBuf;

use crate::auth::{self, Auth};
use crate::error::{ok, ApiError, ApiResult};
use crate::state::AppState;

/// 上传目录：环境变量 UPLOAD_DIR 优先，缺省 cwd 下的 uploads/
pub fn uploads_dir() -> PathBuf {
    std::env::var("UPLOAD_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("uploads"))
}

/// 启动期确保上传目录存在（幂等）
pub fn ensure_uploads_dir() {
    let _ = std::fs::create_dir_all(uploads_dir());
}

/// 由原文件名 / Content-Type 推导安全扩展名
fn ext_of(name: &str, content_type: &str) -> String {
    if let Some(ext) = name.rsplit('.').next() {
        let e = ext.to_ascii_lowercase();
        if !e.is_empty() && e.len() <= 5 && e.chars().all(|c| c.is_ascii_alphanumeric()) {
            return e;
        }
    }
    match content_type {
        "image/png" => "png",
        "image/jpeg" | "image/jpg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/svg+xml" => "svg",
        "video/mp4" => "mp4",
        "application/pdf" => "pdf",
        _ => "bin",
    }
    .to_string()
}

fn kind_of(content_type: &str) -> &'static str {
    if content_type.starts_with("image/") {
        "image"
    } else if content_type.starts_with("video/") {
        "video"
    } else {
        "file"
    }
}

/// POST /api/upload —— 需 content.media.upload 权限；字段名 `file`
pub async fn upload(
    State(_st): State<AppState>,
    auth: Auth,
    mut mp: Multipart,
) -> ApiResult {
    auth::ensure(&auth, "content.media.upload")?;

    let mut items = Vec::new();
    while let Some(field) = mp
        .next_field()
        .await
        .map_err(|e| ApiError::bad(format!("解析上传失败：{e}")))?
    {
        // 仅处理名为 file 的字段（忽略其它表单字段）
        if field.name() != Some("file") {
            continue;
        }
        let orig = field.file_name().unwrap_or("file").to_string();
        let content_type = field
            .content_type()
            .unwrap_or("application/octet-stream")
            .to_string();
        let bytes = field
            .bytes()
            .await
            .map_err(|e| ApiError::bad(format!("读取文件失败：{e}")))?;
        let len = bytes.len();
        if len == 0 {
            return Err(ApiError::bad("文件为空"));
        }
        if len > 20 * 1024 * 1024 {
            return Err(ApiError::bad("单文件超过 20MB 上限"));
        }
        let ext = ext_of(&orig, &content_type);
        let fname = format!("{}.{}", uuid::Uuid::new_v4(), ext);
        let path = uploads_dir().join(&fname);
        tokio::fs::write(&path, bytes)
            .await
            .map_err(|e| ApiError::bad(format!("写入失败：{e}")))?;
        items.push(json!({
            "url": format!("/uploads/{}", fname),
            "name": orig,
            "type": kind_of(&content_type),
            "sizeKb": (len + 1023) / 1024,
        }));
    }

    if items.is_empty() {
        return Err(ApiError::bad("未收到文件（请使用字段名 file）"));
    }
    ok(json!({ "items": items }))
}
