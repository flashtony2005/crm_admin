/**
 * 按钮级权限码表 + 角色→权限矩阵（Phase 1 mock 源）。
 *
 * 设计（docs/RBAC_BUTTON_LEVEL_DESIGN.md）：
 * - 权限码 = capability code，点分格式 `域.资源.动作`，一处定义、三方共用：
 *   前端按钮显隐 / 后端接口校验（Axum extractor）/ AI 能力（Policy 链路）；
 * - Phase 1 权限来源是本地角色矩阵（owner/editor/viewer）；
 * - 后端就绪后，唯一改动：store/permission.ts 的权限集合改由 /api/auth/me 下发，
 *   本文件的码表不变。
 */

/** 所有权限码（字面量联合，拼写错误在编译期暴露） */
export const P = {
  // Content
  contentPagesView: 'content.pages.view',
  contentPagesCreate: 'content.pages.create',
  contentPagesUpdate: 'content.pages.update',
  contentPagesDelete: 'content.pages.delete',
  contentPagesPublish: 'content.pages.publish',
  contentArticlesView: 'content.articles.view',
  contentArticlesCreate: 'content.articles.create',
  contentArticlesUpdate: 'content.articles.update',
  contentArticlesDelete: 'content.articles.delete',
  contentArticlesPublish: 'content.articles.publish',
  contentTagsView: 'content.tags.view',
  contentTagsCreate: 'content.tags.create',
  contentTagsUpdate: 'content.tags.update',
  contentTagsDelete: 'content.tags.delete',
  contentProductsView: 'content.products.view',
  contentProductsCreate: 'content.products.create',
  contentProductsUpdate: 'content.products.update',
  contentProductsDelete: 'content.products.delete',
  contentProductsPublish: 'content.products.publish',
  contentMediaView: 'content.media.view',
  contentMediaUpload: 'content.media.upload',
  contentMediaDelete: 'content.media.delete',
  // AI
  aiAssistantUse: 'ai.assistant.use',
  aiTasksView: 'ai.tasks.view',
  aiApprovalsView: 'ai.approvals.view',
  /** 批准/驳回高风险操作：Owner 专属（AI 发布请求的裁决者） */
  aiApprovalsDecide: 'ai.approvals.decide',
  // Business
  businessCustomersView: 'business.customers.view',
  businessCustomersCreate: 'business.customers.create',
  businessCustomersUpdate: 'business.customers.update',
  businessCustomersDelete: 'business.customers.delete',
  businessLeadsView: 'business.leads.view',
  businessLeadsCreate: 'business.leads.create',
  businessLeadsUpdate: 'business.leads.update',
  businessLeadsDelete: 'business.leads.delete',
  businessFormsView: 'business.forms.view',
  businessFormsCreate: 'business.forms.create',
  // Automation
  automationWorkflowsView: 'automation.workflows.view',
  automationWorkflowsToggle: 'automation.workflows.toggle',
  automationIntegrationsView: 'automation.integrations.view',
  automationIntegrationsToggle: 'automation.integrations.toggle',
  // Team / Settings
  teamUsersView: 'team.users.view',
  teamUsersInvite: 'team.users.invite',
  teamRolesView: 'team.roles.view',
  teamRolesManage: 'team.roles.manage',
  settingsManage: 'settings.manage',
} as const

export type PermCode = (typeof P)[keyof typeof P]
export type PermString =
  | PermCode
  // 支持通配尾段（如 'content.articles.*'、'*'）
  | `${string}.*`
  | '*'

export type RoleKey = 'owner' | 'editor' | 'viewer'

const VIEW_ALL: PermString[] = [
  'content.pages.view', 'content.articles.view', 'content.products.view',
  'content.media.view', 'ai.assistant.use', 'ai.tasks.view', 'ai.approvals.view',
  'business.customers.view', 'business.leads.view', 'business.forms.view',
  'automation.workflows.view', 'automation.integrations.view',
]

/** 角色 → 权限集矩阵（Phase 3 由后端角色配置取代；语义保持一致） */
export const ROLE_PERMS: Record<RoleKey, PermString[]> = {
  owner: ['*'],
  editor: [
    ...VIEW_ALL,
    // 内容：可增删改，但发布权（*.publish）刻意不授予 —— 必须走 Owner 审批（纲领 §7）
    'content.pages.create', 'content.pages.update', 'content.pages.delete',
    'content.articles.create', 'content.articles.update', 'content.articles.delete',
    'content.products.create', 'content.products.update', 'content.products.delete',
    'content.media.upload', 'content.media.delete',
    'business.customers.create', 'business.customers.update', 'business.customers.delete',
    'business.leads.create', 'business.leads.update', 'business.leads.delete',
    'business.forms.create',
  ],
  viewer: [...VIEW_ALL],
}

/** 编辑器角色额外授予的自动化管理权（与后端 perm.rs Editor 矩阵严格镜像） */
const EDITOR_EXTRA: PermString[] = [
  'automation.workflows.toggle',
  'automation.integrations.toggle',
]

// 将自动化管理权并入 editor 矩阵（UX 镜像，确保「新建/编辑」按钮可见且后端放行）
ROLE_PERMS.editor = [...ROLE_PERMS.editor, ...EDITOR_EXTRA]

/**
 * 权限匹配：精确命中或通配尾段（'content.articles.*' 匹配该域全部动作，
 * '*' 为全量）。与 RuoYi hasPermi 的通配语义对齐，但码表强类型。
 * ⚠️ 通配是双刃剑：'content.articles.*' 会连 publish 一起授予，
 * 因此角色矩阵一律显式枚举写权限，通配仅用于服务端下发的聚合场景。
 */
export function permMatches(granted: PermString[], need: string): boolean {
  return granted.some((g) =>
    g === '*' || g === need ||
    (g.endsWith('.*') && need.startsWith(g.slice(0, -1))),
  )
}
