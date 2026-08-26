/**
 * 产品导航配置 —— AI-Native Small Business CMS
 *
 * 设计决策（docs/PRODUCT_VISION.md §3 + RBAC_BUTTON_LEVEL_DESIGN.md）：
 * - 用户看到的是固定产品面：Home / Content / AI / Business / Automation / Team / Settings；
 * - 菜单由前端持有（本文件），不再依赖后端 /api/user/menus 动态下发；
 * - 每个入口的可见性由「准入权限码」决定（perm 字段），从权限矩阵派生：
 *   管理员看全部，经办者只看与其角色相关的菜单 —— 单一事实源。
 */

/** 叶子节点：一个真实页面 */
export interface NavLeaf {
  key: string
  label: string
  path: string
  /** Sidebar ICON_D 的图标名 */
  icon: string
  /** 一句话说明（Home 快捷入口等场景复用） */
  desc?: string
  /** 准入权限码（config/permissions.ts）；空 = 登录即可见 */
  perm?: string
}

/** 分组节点：产品 IA 的一级模块 */
export interface NavSection {
  key: string
  label: string
  icon: string
  children: NavLeaf[]
}

export type NavNode = NavLeaf | NavSection

export function isSection(n: NavNode): n is NavSection {
  return 'children' in n
}

/** 产品信息架构（与 PRODUCT_VISION.md §3 严格一致） */
export const PRODUCT_NAV: NavNode[] = [
  {
    key: 'home',
    label: '首页',
    path: '/home',
    icon: 'home',
    desc: '经营概览与待办',
  },
  {
    key: 'content',
    label: '内容',
    icon: 'book',
    children: [
      { key: 'pages', label: '页面', path: '/content/pages', icon: 'page', desc: '网站页面', perm: 'content.pages.view' },
      { key: 'articles', label: '文章', path: '/content/articles', icon: 'article', desc: '文章与动态', perm: 'content.articles.view' },
      { key: 'tags', label: '标签', path: '/content/tags', icon: 'tag', desc: '标签库（描述/封面/SEO）', perm: 'content.tags.view' },
      { key: 'products', label: '产品', path: '/content/products', icon: 'box', desc: '产品与服务', perm: 'content.products.view' },
      { key: 'media', label: '素材', path: '/content/media', icon: 'image', desc: '图片与素材', perm: 'content.media.view' },
    ],
  },
  {
    key: 'ai',
    label: 'AI 工作台',
    icon: 'sparkles',
    children: [
      { key: 'assistant', label: 'AI 助手', path: '/ai/assistant', icon: 'sparkles', desc: '让 AI 帮你完成工作', perm: 'ai.assistant.use' },
      { key: 'tasks', label: '执行记录', path: '/ai/tasks', icon: 'clock', desc: 'AI 执行记录', perm: 'ai.tasks.view' },
      { key: 'approvals', label: '待办审批', path: '/ai/approvals', icon: 'check', desc: '等待你批准的操作', perm: 'ai.approvals.decide' },
    ],
  },
  {
    key: 'business',
    label: '业务',
    icon: 'users',
    children: [
      { key: 'customers', label: '客户', path: '/business/customers', icon: 'users', desc: '客户档案', perm: 'business.customers.view' },
      { key: 'forms', label: '表单', path: '/business/forms', icon: 'form', desc: '表单与收集', perm: 'business.forms.view' },
      { key: 'leads', label: '线索', path: '/business/leads', icon: 'target', desc: '线索跟进', perm: 'business.leads.view' },
    ],
  },
  {
    key: 'automation',
    label: '自动化',
    icon: 'workflow',
    children: [
      { key: 'workflows', label: '工作流', path: '/automation/workflows', icon: 'workflow', desc: '自动化流程', perm: 'automation.workflows.toggle' },
      { key: 'integrations', label: '集成', path: '/automation/integrations', icon: 'plug', desc: '应用集成', perm: 'automation.integrations.toggle' },
    ],
  },
  {
    key: 'team',
    label: '团队',
    icon: 'shield',
    children: [
      { key: 'users', label: '成员', path: '/team/users', icon: 'users', desc: '成员管理', perm: 'team.users.view' },
      { key: 'roles', label: '角色', path: '/team/roles', icon: 'shield', desc: '角色与权限', perm: 'team.roles.view' },
    ],
  },
  {
    key: 'settings',
    label: '设置',
    path: '/settings',
    icon: 'settings',
    desc: '企业设置',
    perm: 'settings.manage',
  },
]

/** 展开为叶子列表（HorizontalMenuBar / 快捷入口等场景） */
export function flattenNavLeaves(nodes: NavNode[] = PRODUCT_NAV): NavLeaf[] {
  const out: NavLeaf[] = []
  for (const n of nodes) {
    if (isSection(n)) out.push(...flattenNavLeaves(n.children))
    else out.push(n)
  }
  return out
}

/** 按 path 查找导航标签（面包屑等场景），未命中返回 undefined */
export function findNavLabel(path: string): string | undefined {
  for (const leaf of flattenNavLeaves()) {
    if (leaf.path === path) return leaf.label
  }
  return undefined
}

/**
 * 按「准入权限」过滤导航树：has(perm) 为 true 才保留。
 * - 管理员（owner）持全量码 → 看到全部菜单；
 * - 经办者（editor）只持其职责内的码 → 只看到与其相关的菜单；
 * - 分组下所有叶子被过滤光时整组隐藏。
 */
export function filterNavByPerm(
  nodes: NavNode[] = PRODUCT_NAV,
  has: (perm: string) => boolean,
): NavNode[] {
  const keepLeaf = (leaf: NavLeaf) => !leaf.perm || has(leaf.perm)
  return nodes
    .map((n) => {
      if (isSection(n)) {
        const children = n.children.filter(keepLeaf)
        return children.length > 0 ? { ...n, children } : null
      }
      return keepLeaf(n) ? n : null
    })
    .filter((n): n is NavNode => n !== null)
}
