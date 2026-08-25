//! 权限码表与角色矩阵 —— 与 web `src/config/permissions.ts` 严格镜像。
//!
//! 单一语义，两端各自实现（前端 TS / 后端 Rust）：
//! 后端是**权威**（RequirePerm extractor 逐请求校验），
//! 前端是 UX 镜像。Phase 3 落 role_permission 表后，
//! 矩阵退役为默认种子数据。

pub const P: &[(&str, &str)] = &[
    ("contentPagesView", "content.pages.view"),
    ("contentArticlesCreate", "content.articles.create"),
    ("contentArticlesUpdate", "content.articles.update"),
    ("contentArticlesDelete", "content.articles.delete"),
    ("contentArticlesPublish", "content.articles.publish"),
];

/// 角色键
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Owner,
    Editor,
    Viewer,
}

impl Role {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "owner" => Some(Role::Owner),
            "editor" => Some(Role::Editor),
            "viewer" => Some(Role::Viewer),
            _ => None,
        }
    }

    /// 角色的权限集合（owner = 全量通配）
    pub fn perms(&self) -> Vec<String> {
        match self {
            Role::Owner => vec!["*".to_string()],
            Role::Editor => [
                // 只读面
                "content.pages.view",
                "content.articles.view",
                "content.products.view",
                "content.media.view",
                "ai.assistant.use",
                "ai.tasks.view",
                "ai.approvals.view",
                "business.customers.view",
                "business.leads.view",
                "business.forms.view",
                "automation.workflows.view",
                "automation.workflows.toggle",
                "automation.integrations.view",
                "automation.integrations.toggle",
                // 内容写权（发布权刻意不授 → 必须走审批，纲领 §7）
                "content.pages.create",
                "content.pages.update",
                "content.pages.delete",
                "content.articles.create",
                "content.articles.update",
                "content.articles.delete",
                "content.products.create",
                "content.products.update",
                "content.products.delete",
                "content.media.upload",
                "content.media.delete",
                "business.customers.create",
                "business.customers.update",
                "business.customers.delete",
                "business.leads.create",
                "business.leads.update",
                "business.leads.delete",
                "business.forms.create",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
            Role::Viewer => [
                "content.pages.view",
                "content.articles.view",
                "content.products.view",
                "content.media.view",
                "ai.assistant.use",
                "ai.tasks.view",
                "ai.approvals.view",
                "business.customers.view",
                "business.leads.view",
                "business.forms.view",
                "automation.workflows.view",
                "automation.integrations.view",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        }
    }
}

/// 权限匹配：精确命中或通配尾段（'content.articles.*' / '*'）。
/// 与前端 permMatches 行为一致；前缀相似不得误命中。
pub fn perm_matches(granted: &[String], need: &str) -> bool {
    granted.iter().any(|g| {
        g == "*"
            || g == need
            || (g.ends_with(".*") && need.starts_with(g.trim_end_matches('*')))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_is_wildcard() {
        assert!(perm_matches(&Role::Owner.perms(), "anything.at.all"));
    }

    #[test]
    fn editor_can_write_but_not_publish() {
        let p = Role::Editor.perms();
        assert!(perm_matches(&p, "content.articles.create"));
        assert!(perm_matches(&p, "content.articles.update"));
        assert!(!perm_matches(&p, "content.articles.publish"));
        assert!(!perm_matches(&p, "ai.approvals.decide"));
        assert!(!perm_matches(&p, "team.users.invite"));
    }

    #[test]
    fn viewer_is_readonly() {
        let p = Role::Viewer.perms();
        assert!(perm_matches(&p, "content.articles.view"));
        assert!(!perm_matches(&p, "content.articles.create"));
        assert!(!perm_matches(&p, "business.customers.delete"));
    }

    #[test]
    fn wildcard_no_false_positive() {
        let p = vec!["content.articles.*".to_string()];
        assert!(perm_matches(&p, "content.articles.delete"));
        assert!(!perm_matches(&p, "content.articlesX.create"));
    }
}
