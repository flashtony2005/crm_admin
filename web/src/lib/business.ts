/**
 * 业务语言化：
 * 把底层技术错误 / 能力代号翻译成用户能直接理解的提示，
 * 普通用户不需要知道 token、capability、precondition 这些概念。
 */

const PATTERNS: Array<[RegExp, string]> = [
  [/未认证或 token 已失效/i, '登录已过期，请重新登录'],
  [/missing input\.id/i, '缺少必要信息，请补充完整后重试'],
  [/missing\s+[a-z_:.0-9-]+/i, '缺少必要信息，请补充后重试'],
  [/capability.*not found|未找到.*能力|未匹配到可执行/i, '系统还没有这项操作，换个说法试试'],
  [/not authorized|unauthorized|permission denied|权限不足|未授权/i, '你还没有执行此操作的权限'],
  [/precondition/i, '当前条件不满足，需要审批或更高权限'],
  [/failed to fetch|fetch failed|network/i, '网络连接失败，请稍后重试'],
  [/内部错误/i, '操作遇到内部错误，请稍后重试'],
]

/** 技术错误 → 业务提示（未命中时原样返回） */
export function businessMessage(raw: string): string {
  for (const [re, msg] of PATTERNS) {
    if (re.test(raw)) return msg
  }
  return raw
}

const OBJECT_CN: Record<string, string> = {
  User: '用户',
  Role: '角色',
  Menu: '菜单',
  Tenant: '租户',
  Permission: '权限',
  Department: '部门',
  Article: '文章',
  Customer: '客户',
  Order: '订单',
  Project: '项目',
  Task: '任务',
  Employee: '员工',
  Refund: '退款',
  Ticket: '工单',
  Inventory: '库存',
  SuperAdmin: '超级管理员',
}

const VERB_CN: Record<string, string> = {
  Create: '新建',
  CreateSQL: '新建',
  Read: '查看',
  Update: '编辑',
  Delete: '删除',
  List: '查看列表',
  ListAll: '查看全部',
  ListTenant: '查看本租户',
  ListSelf: '查看自己',
  Approve: '审核',
  Reject: '驳回',
  Publish: '发布',
  SubmitReview: '提交审核',
  UpdateStatus: '更新状态',
  UpdatePassword: '修改密码',
  AssignRole: '分配角色',
  Start: '启动',
  Complete: '标记完成',
  Close: '关闭',
  StockCheck: '盘点',
}

const NO_OBJECT_VERBS = new Set(['UpdatePassword'])

/** 能力代号 → 业务操作名（如 User:ListAll → 查看全部用户） */
export function humanizeCapability(name: string): string {
  const idx = name.indexOf(':')
  if (idx < 0) return name
  const obj = name.slice(0, idx)
  const verb = name.slice(idx + 1)
  const verbZh = VERB_CN[verb] ?? verb.replace(/([a-z])([A-Z])/g, '$1 $2')
  if (NO_OBJECT_VERBS.has(verb)) return verbZh
  const objZh = OBJECT_CN[obj] ?? obj
  return `${verbZh}${objZh}`
}
