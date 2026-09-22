//! 公开主页「版式预设」目录 —— 全站**单一权威**（single source of truth）。
//!
//! 背景：预设清单曾在三处各存一份副本 —— 后端模板清单、后台 UI、主页前端 ——
//! 手工同步极易漂移（后台加了一档、主页没加，选完不生效）。现在收敛为
//! **本模块一个定义**：
//!
//! - `site.rs::update_site` —— 用 [`is_valid`] 做写入白名单（非法值直接拒绝）；
//! - `site.rs::site_value` —— 把 [`CATALOG`] 作为 `homePresets` 下发给后台 UI；
//! - `templates.rs` —— 模板清单的 `presets` 字段从 [`ids`] 派生，不再手抄。
//!
//! 职责边界：**主页前端（coucouya）的 `src/presets.ts` 仍是"版式实现"的权威** ——
//! 它决定每个预设的区块顺序与骨架；本目录只描述"有哪些预设、叫什么、长什么样、
//! 是否双栏"，供后台展示与校验。新增一个预设需要两处各加一条：本模块 + coucouya
//! 的实现（外加 `src/themes.ts` 若需要新配色）。
//!
//! `crate::presets::tests` 会断言 [`DEFAULT`] 一定在 [`CATALOG`] 内。

use serde_json::{json, Value};

/// 一个版式预设的目录元信息（**展示用**，不含版式实现）。
pub struct PresetMeta {
    /// 预设键，与 coucouya `src/presets.ts` 的 `PresetId` 一一对应
    pub id: &'static str,
    /// 后台卡片标题
    pub label: &'static str,
    /// 后台卡片说明（一句话讲清顺序 / 骨架差异）
    pub desc: &'static str,
    /// 骨架缩略图的行宽百分比（纯示意，不是真实版式）
    pub bars: &'static [u8],
    /// 是否双栏（后台缩略图右侧多画一块侧栏）
    pub sidebar: bool,
}

/// 默认预设 —— 读不到 / 值非法时的回落目标。**必须**在 [`CATALOG`] 内（有测试兜底）。
pub const DEFAULT: &str = "classic";

/// 预设目录。顺序即后台展示顺序；唯一权威，勿在别处再抄一份。
pub const CATALOG: &[PresetMeta] = &[
    PresetMeta {
        id: "classic",
        label: "经典版式",
        desc: "主视觉 → 组织 → 代表文章 → 系列 → Web3，信息最全",
        bars: &[100, 58, 84, 50, 70],
        sidebar: false,
    },
    PresetMeta {
        id: "editorial",
        label: "杂志编辑",
        desc: "内容前置：大标题主视觉 + 文章栅格 + 系列分区",
        bars: &[100, 88, 72, 54, 62],
        sidebar: false,
    },
    PresetMeta {
        id: "gallery",
        label: "影像优先",
        desc: "深底 + 两列大图，封面图主导，去掉旁枝",
        bars: &[58, 96, 74, 52],
        sidebar: false,
    },
    PresetMeta {
        id: "sidebar",
        label: "侧栏双栏",
        desc: "主内容 + 常驻侧栏（订阅位 / 最近更新 / 目录）",
        bars: &[72, 88, 76, 64],
        sidebar: true,
    },
    PresetMeta {
        id: "minimal",
        label: "极简单栏",
        desc: "只留主视觉与单列文章流，阅读动线最短",
        bars: &[100, 76],
        sidebar: false,
    },
];

/// 全部预设键（用于白名单与模板清单派生）。
pub fn ids() -> Vec<String> {
    CATALOG.iter().map(|p| p.id.to_string()).collect()
}

/// 是否为合法预设键。
pub fn is_valid(id: &str) -> bool {
    CATALOG.iter().any(|p| p.id == id)
}

/// 合法预设键的展示串（错误信息用），如 `classic | editorial | ...`。
pub fn id_list() -> String {
    CATALOG.iter().map(|p| p.id).collect::<Vec<_>>().join(" | ")
}

/// 目录的 JSON 形态，下发给后台 UI（字段名与前端 `HOME_PRESETS` 对齐）。
pub fn as_json() -> Value {
    Value::Array(
        CATALOG
            .iter()
            .map(|p| {
                json!({
                    "id": p.id,
                    "name": p.label,
                    "desc": p.desc,
                    "bars": p.bars,
                    "sidebar": p.sidebar,
                })
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_in_catalog() {
        assert!(
            is_valid(DEFAULT),
            "DEFAULT={DEFAULT} 不在 CATALOG 内，读侧回落会落到非法值"
        );
    }

    #[test]
    fn ids_unique_and_nonempty() {
        let ids = ids();
        assert!(!ids.is_empty());
        let mut sorted = ids.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), ids.len(), "CATALOG 存在重复 id：{ids:?}");
        assert!(ids.iter().all(|s| !s.is_empty()), "CATALOG 存在空 id");
    }

    #[test]
    fn catalog_json_shape() {
        let v = as_json();
        let arr = v.as_array().expect("必须是数组");
        assert_eq!(arr.len(), CATALOG.len());
        let first = &arr[0];
        assert!(first.get("id").and_then(|x| x.as_str()).is_some());
        assert!(first.get("name").and_then(|x| x.as_str()).is_some());
        assert!(first.get("bars").and_then(|x| x.as_array()).is_some());
        assert!(first.get("sidebar").and_then(|x| x.as_bool()).is_some());
    }

    #[test]
    fn is_valid_rejects_unknown_and_empty() {
        assert!(is_valid("sidebar"));
        assert!(!is_valid("duck")); // 风格键不是预设键
        assert!(!is_valid(""));
        assert!(!is_valid("CLASSIC"));
        assert!(!is_valid("经典版式"));
    }
}
