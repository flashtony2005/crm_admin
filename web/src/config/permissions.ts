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

  // 首页区块（独立 CMS 资源：关于 / 组织 / 实验 / Web3）
  contentSectionsView: 'content.sections.view',
  contentSectionsCreate: 'content.sections.create',
  contentSectionsUpdate: 'content.sections.update',
  contentSectionsDelete: 'content.sections.delete',

  // 站点导航/页脚链接（site 域，nav_links 表）
  siteNavView: 'site.nav.view',
  siteNavCreate: 'site.nav.create',
  siteNavUpdate: 'site.nav.update',
  siteNavDelete: 'site.nav.delete',

  // 主页置顶文章（home_pins 表）
  siteHomePinsView: 'site.homePins.view',
  siteHomePinsCreate: 'site.homePins.create',
  siteHomePinsUpdate: 'site.homePins.update',
  siteHomePinsDelete: 'site.homePins.delete',

  // P4 商业层
  contentMembersView: 'content.members.view',
  contentMembersCreate: 'content.members.create',
  contentMembersUpdate: 'content.members.update',
  contentMembersDelete: 'content.members.delete',

  contentCommentsView: 'content.comments.view',
  contentCommentsUpdate: 'content.comments.update',
  contentCommentsDelete: 'content.comments.delete',

  newsletterSubscribersView: 'newsletter.subscribers.view',
  newsletterCampaignSend: 'newsletter.campaign.send',

  subscriptionsTiersView: 'subscriptions.tiers.view',
  subscriptionsTiersCreate: 'subscriptions.tiers.create',
  subscriptionsTiersUpdate: 'subscriptions.tiers.update',
  subscriptionsTiersDelete: 'subscriptions.tiers.delete',

  subscriptionsListView: 'subscriptions.list.view',

  webhooksView: 'webhooks.view',
  webhooksCreate: 'webhooks.create',
  webhooksUpdate: 'webhooks.update',
  webhooksDelete: 'webhooks.delete',

  i18nView: 'i18n.view',
  i18nUpdate: 'i18n.update',

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
  teamUsersUpdate: 'team.users.update',
  teamRolesView: 'team.roles.view',
  teamRolesManage: 'team.roles.manage',
  settingsManage: 'settings.manage',

  // SEO（P0-1：重定向 + 404 监控。独立域而非 content.* —— 它管的是
  // 「站点入口资产」而不是内容生产，混在一个权限口上会切错授权面）
  contentSeoView: 'content.seo.view',
  contentSeoCreate: 'content.seo.create',
  contentSeoUpdate: 'content.seo.update',
  contentSeoDelete: 'content.seo.delete',

  // 分析归因（只读）
  analyticsAttributionView: 'analytics.attribution.view',
} as const

export type PermCode = (typeof P)[keyof typeof P]
export type PermString =
  | PermCode
  // 支持通配尾段（如 'content.articles.*'、'*'）
  | `${string}.*`
  | '*'

export type RoleKey = 'owner' | 'editor' | 'viewer' | string

export interface RoleDef {
  key: string
  name: string
  desc: string
  aiBoundary: string
  isSystem?: boolean
  cls?: string
}

export const DEFAULT_ROLES: RoleDef[] = [
  {
    key: 'owner',
    name: 'Owner（老板）',
    desc: '拥有系统全部管理权限：团队成员、审批发布、系统配置与财务账单。',
    aiBoundary: 'AI 可以发起审批请求，高风险敏感操作由本人最终裁决。',
    isSystem: true,
    cls: 'border-indigo-200 bg-gradient-to-br from-indigo-50/60 to-white',
  },
  {
    key: 'editor',
    name: 'Editor（店员/编辑）',
    desc: '创建与编辑内容、管理客户和销售线索，但不可直接越级发布高危内容。',
    aiBoundary: 'AI 可以协助润色草稿；内容发布必须经由 Owner 审批通过。',
    isSystem: true,
    cls: 'border-blue-200 bg-gradient-to-br from-blue-50/40 to-white',
  },
  {
    key: 'viewer',
    name: 'Viewer（观察者/只读）',
    desc: '只读访问系统：查看统计报表与内容列表，不可修改或提交数据。',
    aiBoundary: 'AI 仅可执行查询与摘要类只读操作，不可写入数据。',
    isSystem: true,
    cls: 'border-gray-200 bg-gradient-to-br from-gray-50/60 to-white',
  },
]

export interface PermissionItem {
  code: PermCode
  name: string
  desc: string
}

export interface PermissionGroup {
  id: string
  title: string
  icon?: string
  permissions: PermissionItem[]
}

export const PERMISSION_GROUPS: PermissionGroup[] = [
  {
    id: 'content',
    title: '内容生产与发布',
    permissions: [
      { code: P.contentPagesView, name: '查看页面', desc: '查看网站静态页面列表与详情' },
      { code: P.contentPagesCreate, name: '新建页面', desc: '创建新页面与编辑草稿' },
      { code: P.contentPagesUpdate, name: '编辑页面', desc: '修改页面内容、结构与排版' },
      { code: P.contentPagesDelete, name: '删除页面', desc: '删除已有网站页面' },
      { code: P.contentPagesPublish, name: '直接发布页面', desc: '免审批直接发布页面到线上' },
      { code: P.contentArticlesView, name: '查看文章', desc: '查看文章列表与阅读统计' },
      { code: P.contentArticlesCreate, name: '新建文章', desc: '起草撰写新文章与动态' },
      { code: P.contentArticlesUpdate, name: '编辑文章', desc: '修改正文、摘要与 SEO 字段' },
      { code: P.contentArticlesDelete, name: '删除文章', desc: '删除文章归档' },
      { code: P.contentArticlesPublish, name: '直接发布文章', desc: '免审批直接上线文章' },
      { code: P.contentTagsView, name: '查看标签', desc: '查看标签库与文章关联' },
      { code: P.contentTagsCreate, name: '新建标签', desc: '添加新分类与聚合标签' },
      { code: P.contentTagsUpdate, name: '编辑标签', desc: '修改标签名称、封面与描述' },
      { code: P.contentTagsDelete, name: '删除标签', desc: '删除标签项' },
      { code: P.contentProductsView, name: '查看产品', desc: '浏览产品与服务名录' },
      { code: P.contentProductsCreate, name: '新建产品', desc: '录入新商品与服务项目' },
      { code: P.contentProductsUpdate, name: '编辑产品', desc: '调整商品价格、规格与描述' },
      { code: P.contentProductsDelete, name: '删除产品', desc: '下架或删除商品项' },
      { code: P.contentProductsPublish, name: '直接发布产品', desc: '免审批直接上线商品' },
      { code: P.contentMediaView, name: '查看素材库', desc: '浏览媒体图库与上传历史' },
      { code: P.contentMediaUpload, name: '上传素材', desc: '上传图片、图标与媒体文件' },
      { code: P.contentMediaDelete, name: '删除素材', desc: '删除存储的媒体图片' },
      { code: P.contentSectionsView, name: '查看首页区块', desc: '查看首页栏目与展示配置' },
      { code: P.contentSectionsUpdate, name: '编辑首页区块', desc: '编辑首页介绍、柱石与组织' },
      { code: P.siteNavView, name: '查看导航与页脚', desc: '查看站点顶栏导航与页脚链接' },
      { code: P.siteNavUpdate, name: '配置导航与页脚', desc: '修改导航菜单项与外部链接' },
      { code: P.siteHomePinsView, name: '查看主页置顶', desc: '查看首页头条置顶文章' },
      { code: P.siteHomePinsUpdate, name: '管理主页置顶', desc: '置顶或取消首页推荐文章' },
      { code: P.contentSeoView, name: '查看 SEO 与重定向', desc: '查看 301 重定向规则与 404 监控' },
      { code: P.contentSeoUpdate, name: '配置重定向规则', desc: '增改删短链与 URL 跳转规则' },
    ],
  },
  {
    id: 'business',
    title: '客户业务与商业化',
    permissions: [
      { code: P.businessCustomersView, name: '查看客户', desc: '查看客户档案与联系方式' },
      { code: P.businessCustomersCreate, name: '新建客户', desc: '录入新客户资料' },
      { code: P.businessCustomersUpdate, name: '编辑客户', desc: '更新客户信息与偏好' },
      { code: P.businessCustomersDelete, name: '删除客户', desc: '移除客户归档' },
      { code: P.businessLeadsView, name: '查看线索', desc: '查看销售线索看板与跟进' },
      { code: P.businessLeadsCreate, name: '新建线索', desc: '录入销售意向与潜客' },
      { code: P.businessLeadsUpdate, name: '跟进线索', desc: '更新线索阶段与跟进记录' },
      { code: P.businessLeadsDelete, name: '删除线索', desc: '关闭或清除无效线索' },
      { code: P.businessFormsView, name: '查看表单', desc: '查看表单收集与填写结果' },
      { code: P.businessFormsCreate, name: '创建表单', desc: '设计新收集表单并生成链接' },
      { code: P.contentMembersView, name: '查看会员', desc: '查看注册会员与到期日' },
      { code: P.contentMembersUpdate, name: '管理会员', desc: '调整会员套餐、余额与状态' },
      { code: P.contentCommentsView, name: '查看评论', desc: '查看公开站点用户留言' },
      { code: P.contentCommentsUpdate, name: '审核与回复评论', desc: '批准或屏蔽用户评论' },
      { code: P.newsletterSubscribersView, name: '查看邮件订阅', desc: '查看 Newsletter 订阅者' },
      { code: P.newsletterCampaignSend, name: '发送邮件群发', desc: '向订阅者群发活动与期刊' },
      { code: P.subscriptionsTiersView, name: '查看付费方案', desc: '查看定价方案与权益说明' },
      { code: P.subscriptionsTiersUpdate, name: '配置付费方案', desc: '修改套餐价格与功能权限' },
      { code: P.subscriptionsListView, name: '查看订阅账单', desc: '查看用户订阅与续费周期' },
      { code: P.analyticsAttributionView, name: '获客归因分析', desc: '查看哪篇文章带来会员与收入' },
    ],
  },
  {
    id: 'automation',
    title: 'AI 助手与自动化',
    permissions: [
      { code: P.aiAssistantUse, name: '使用 AI 助手', desc: '调用智能辅助撰写与分析' },
      { code: P.aiTasksView, name: '查看 AI 任务记录', desc: '查看 AI 异步执行队列与结果' },
      { code: P.aiApprovalsView, name: '查看审批中心', desc: '查看等待审批的高风险待办' },
      { code: P.aiApprovalsDecide, name: '审批决策权', desc: '批准或驳回发布与关键改动' },
      { code: P.automationWorkflowsView, name: '查看工作流', desc: '查看自动化触发器与步骤' },
      { code: P.automationWorkflowsToggle, name: '管理工作流', desc: '开启、停用与设计自动化' },
      { code: P.automationIntegrationsView, name: '查看集成', desc: '查看第三方应用连接状态' },
      { code: P.automationIntegrationsToggle, name: '配置应用集成', desc: '连接或断开外部工具' },
      { code: P.webhooksView, name: '查看 Webhook', desc: '查看出站事件推送订阅' },
      { code: P.webhooksCreate, name: '管理 Webhook', desc: '新增与配置出站事件推送' },
    ],
  },
  {
    id: 'team_settings',
    title: '团队安全与系统设置',
    permissions: [
      { code: P.teamUsersView, name: '查看团队成员', desc: '查看系统成员列表与状态' },
      { code: P.teamUsersInvite, name: '邀请成员', desc: '邀请新团队成员加入系统' },
      { code: P.teamUsersUpdate, name: '成员角色与停用', desc: '调整成员角色或启用/停用账号' },
      { code: P.teamRolesView, name: '查看角色与权限', desc: '查看角色权限矩阵与说明' },
      { code: P.teamRolesManage, name: '角色管理与授权', desc: '创建角色、分配权限与授权成员' },
      { code: P.settingsManage, name: '系统设置管理', desc: '管理企业资料与外观模板' },
      { code: P.i18nView, name: '查看多语言', desc: '查看多语言翻译表' },
      { code: P.i18nUpdate, name: '更新多语言', desc: '保存中英文对照翻译' },
    ],
  },
]

const VIEW_ALL: PermString[] = [
  'content.pages.view', 'content.articles.view', 'content.products.view',
  'content.media.view', 'ai.assistant.use', 'ai.tasks.view', 'ai.approvals.view',
  'business.customers.view', 'business.leads.view', 'business.forms.view',
  'automation.workflows.view', 'automation.integrations.view',
  'site.nav.view', 'site.homePins.view',
  'content.seo.view',
  'analytics.attribution.view',
  'team.users.view',
  'team.roles.view',
]

/** 角色 → 默认权限集矩阵 */
export const ROLE_PERMS: Record<string, PermString[]> = {
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
    // P4 商业层
    'content.members.view', 'content.members.create', 'content.members.update', 'content.members.delete',
    'content.comments.view', 'content.comments.update', 'content.comments.delete',
    'newsletter.subscribers.view', 'newsletter.campaign.send',
    'subscriptions.tiers.view', 'subscriptions.tiers.create', 'subscriptions.tiers.update', 'subscriptions.tiers.delete',
    'subscriptions.list.view',
    'webhooks.view', 'webhooks.create', 'webhooks.update', 'webhooks.delete',
    'i18n.view', 'i18n.update',
    'content.sections.view', 'content.sections.create', 'content.sections.update', 'content.sections.delete',
    'site.nav.view', 'site.nav.create', 'site.nav.update', 'site.nav.delete',
    'site.homePins.view', 'site.homePins.create', 'site.homePins.update', 'site.homePins.delete',
    'content.seo.view', 'content.seo.create', 'content.seo.update', 'content.seo.delete',
    'automation.workflows.toggle',
    'automation.integrations.toggle',
  ],
  viewer: [...VIEW_ALL, 'content.sections.view'],
}

/** 权限匹配：精确命中或通配尾段 */
export function permMatches(granted: PermString[], need: string): boolean {
  return granted.some((g) =>
    g === '*' || g === need ||
    (g.endsWith('.*') && need.startsWith(g.slice(0, -1))),
  )
}
