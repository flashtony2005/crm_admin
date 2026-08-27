//! 文件上传：multipart 接收 → 保存到 uploads/ → 返回公开 URL。
//!
//! 设计要点：
//! - 字段名固定为 `file`（支持一次多文件：循环 next_field）
//! - 文件名用 UUID 重命名（防冲突 / 防路径注入），保留原扩展名
//! - 写入 cwd 下的 `uploads/`（可用环境变量 UPLOAD_DIR 覆盖）
//! - 需 `content.media.upload` 权限；单文件上限 20MB（与全局请求体一致）
//! - 栅格图（jpg/png/gif）自动生成缩略图(320)与大图(1280)变体，返回结构化 URL
//! - 返回 URL 支持 PUBLIC_BASE_URL 前缀（CDN/反向代理就绪）；未设置则回退相对路径 /uploads/*

use axum::extract::{Multipart, State};
use image::{DynamicImage, ImageFormat, imageops::FilterType};
use serde_json::json;
use std::io::Cursor;
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

/// 站点资源根：PUBLIC_BASE_URL（如 https://cdn.example.com）优先；缺省为空（相对路径）。
/// 设置后，所有返回的资源 URL 会带该前缀，便于前端 CDN / 反向代理直接命中。
fn asset_base() -> String {
    std::env::var("PUBLIC_BASE_URL")
        .unwrap_or_default()
        .trim_end_matches('/')
        .to_string()
}

/// 由文件名拼出可被浏览器/CDN 直接访问的资源 URL
fn asset_url(fname: &str) -> String {
    let base = asset_base();
    if base.is_empty() {
        format!("/uploads/{}", fname)
    } else {
        format!("{}/uploads/{}", base, fname)
    }
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

/// 缩略图最大长边（网格/列表用）
const THUMB_MAX: u32 = 320;
/// 大图最大长边（文章正文/灯箱用）
const LARGE_MAX: u32 = 1280;
/// 响应式宽度（srcset 用，命名与 public_api::build_srcset 对齐）
const SRCSET_WIDTHS: &[u32] = &[480, 960, 1600];

/// 等比缩放：长边不超过 max；原图已更小则原样返回（不放大）
fn fit(img: &DynamicImage, max: u32) -> DynamicImage {
    let (w, h) = (img.width(), img.height());
    if w == 0 || h == 0 || (w <= max && h <= max) {
        return img.clone();
    }
    let (nw, nh) = if w >= h {
        (max, (h * max / w).max(1))
    } else {
        ((w * max / h).max(1), max)
    };
    img.resize(nw, nh, FilterType::Lanczos3)
}

/// 编码为字节：jpg 用默认质量，png 保留透明通道
fn encode(im: &DynamicImage, vext: &str) -> Vec<u8> {
    let fmt = if vext == "jpg" {
        ImageFormat::Jpeg
    } else {
        ImageFormat::Png
    };
    let mut c = Cursor::new(Vec::new());
    let _ = im.write_to(&mut c, fmt);
    c.into_inner()
}

/// 对栅格图生成缩略图(320) + 大图(1280) + 响应式变体(480/960/1600)；
/// 非栅格（视频/pdf/svg 或解码失败）返回 None。
/// 返回 (缩略图URL, 大图URL, 宽, 高, srcset)。
async fn make_image_assets(
    bytes: &[u8],
    vext: &str,
    stem: &str,
    dir: &PathBuf,
) -> Option<(String, String, i64, i64, String)> {
    let img = image::load_from_memory(bytes).ok()?;
    let (w, h) = (img.width(), img.height());
    // 缩略图 / 大图
    let thumb = encode(&fit(&img, THUMB_MAX), vext);
    let large = encode(&fit(&img, LARGE_MAX), vext);
    let tn = format!("{}_thumb.{}", stem, vext);
    let ln = format!("{}_lg.{}", stem, vext);
    let _ = tokio::fs::write(dir.join(&tn), thumb).await;
    let _ = tokio::fs::write(dir.join(&ln), large).await;
    // 响应式变体（srcset）
    let mut parts: Vec<String> = Vec::new();
    for wd in SRCSET_WIDTHS {
        let v = encode(&fit(&img, *wd), vext);
        let sn = format!("{}_{}.{}", stem, wd, vext);
        let _ = tokio::fs::write(dir.join(&sn), v).await;
        parts.push(format!("{} {}w", asset_url(&sn), wd));
    }
    Some((asset_url(&tn), asset_url(&ln), w as i64, h as i64, parts.join(", ")))
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
        // 变体编码格式：jpg/jpeg → jpg；png/gif → png；其它类型不生成变体
        let (vext, can_variant) = match ext.as_str() {
            "jpg" | "jpeg" => ("jpg", true),
            "png" | "gif" => ("png", true),
            _ => ("png", false),
        };

        let fname = format!("{}.{}", uuid::Uuid::new_v4(), ext);
        let dir = uploads_dir();
        tokio::fs::write(dir.join(&fname), &bytes)
            .await
            .map_err(|e| ApiError::bad(format!("写入失败：{e}")))?;

        // 生成缩略图 / 大图 / 响应式变体（栅格图）
        let (thumb_url, large_url, width, height, srcset) = if can_variant {
            let stem = fname.strip_suffix(&format!(".{}", ext)).unwrap_or(&fname);
            match make_image_assets(&bytes, vext, stem, &dir).await {
                Some((t, l, w, h, ss)) => (t, l, w, h, ss),
                None => (asset_url(&fname), asset_url(&fname), 0, 0, String::new()),
            }
        } else {
            (asset_url(&fname), asset_url(&fname), 0, 0, String::new())
        };

        items.push(json!({
            "url": asset_url(&fname),
            "thumbnail": thumb_url,
            "large": large_url,
            "srcset": srcset,
            "name": orig,
            "type": kind_of(&content_type),
            "sizeKb": (len + 1023) / 1024,
            "width": width,
            "height": height,
            "mime": content_type,
        }));
    }

    if items.is_empty() {
        return Err(ApiError::bad("未收到文件（请使用字段名 file）"));
    }
    ok(json!({ "items": items }))
}
