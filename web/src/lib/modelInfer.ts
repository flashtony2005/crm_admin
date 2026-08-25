/**
 * 「从数据开始」建模推断：
 * 把用户上传的表格 / 选中的模板，推断成一份业务语言模型草稿
 * （业务对象 + 字段 + 关联 + 操作），用户只需确认，无需接触本体概念。
 */

export type FieldKind = 'text' | 'number' | 'date' | 'boolean'

export interface ModelField {
  key: string
  label: string
  kind: FieldKind
  primary?: boolean
  required?: boolean
  /** 用户确认页可取消该字段 */
  included?: boolean
}

export interface ModelLink {
  to: string
  label: string
  via: string
  included?: boolean
}

export interface ModelAction {
  name: string
  label: string
  kind: 'crud' | 'status'
  targetStatus?: string
  included?: boolean
}

export interface ModelDraft {
  id: string
  objectName: string
  objectLabel: string
  fields: ModelField[]
  links: ModelLink[]
  actions: ModelAction[]
  source: string
  createdAt: number
}

export interface ModelRelationship {
  from: string
  to: string
  label: string
  via: string
  included?: boolean
}

/** 对话式建模产出：多个业务对象 + 对象间关联 */
export interface ModelProject {
  id: string
  name: string
  objects: ModelDraft[]
  relationships: ModelRelationship[]
  source: string
  createdAt: number
}

export interface ModelTemplate {
  id: string
  name: string
  desc: string
  draft: Omit<ModelDraft, 'id' | 'createdAt' | 'source'>
}

// ── 常见英文表名 → 中文业务名 ───────────────────────────
const CN_OBJECT_NAMES: Record<string, string> = {
  customer: '客户',
  client: '客户',
  order: '订单',
  article: '文章',
  content: '内容',
  project: '项目',
  task: '任务',
  inventory: '库存',
  product: '产品',
  employee: '员工',
  supplier: '供应商',
  ticket: '工单',
}

// ── 常见列名 → 中文标签 ────────────────────────────────
const CN_FIELD_LABELS: Record<string, string> = {
  id: 'ID',
  name: '名称',
  title: '标题',
  content: '内容',
  status: '状态',
  phone: '手机号',
  email: '邮箱',
  amount: '金额',
  quantity: '数量',
  unit_price: '单价',
  category: '分类',
  progress: '进度',
  start_date: '开始日期',
  end_date: '结束日期',
  created_at: '创建时间',
  author: '作者',
  owner: '负责人',
  sku: 'SKU',
  code: '编号',
  level: '等级',
  remark: '备注',
}

function humanize(s: string): string {
  const clean = s
    .replace(/[_-]+/g, ' ')
    .replace(/([a-z])([A-Z])/g, '$1 $2')
    .trim()
  if (!clean) return s
  return clean.charAt(0).toUpperCase() + clean.slice(1)
}

function pascalName(s: string): string {
  return s
    .replace(/[^a-zA-Z0-9]+/g, ' ')
    .trim()
    .split(/\s+/)
    .map((p) => p.charAt(0).toUpperCase() + p.slice(1))
    .join('') || 'BusinessObject'
}

function inferKind(values: unknown[]): FieldKind {
  const samples = values.filter((v) => v !== '' && v !== null && v !== undefined).slice(0, 20)
  if (samples.length === 0) return 'text'
  const truthy = new Set(['true', 'false', 'yes', 'no', '是', '否'])
  if (samples.every((v) => truthy.has(String(v).toLowerCase()) || v === 1 || v === 0 || String(v) === '1' || String(v) === '0')) {
    return 'boolean'
  }
  if (samples.every((v) => typeof v === 'number' || /^-?\d+(\.\d+)?$/.test(String(v)))) return 'number'
  if (samples.every((v) => !Number.isNaN(Date.parse(String(v))))) return 'date'
  return 'text'
}

function defaultActions(statusColumn: boolean): ModelAction[] {
  const actions: ModelAction[] = [
    { name: 'Create', label: '新建', kind: 'crud', included: true },
    { name: 'List', label: '查看列表', kind: 'crud', included: true },
    { name: 'Read', label: '查看详情', kind: 'crud', included: true },
    { name: 'Update', label: '编辑', kind: 'crud', included: true },
    { name: 'Delete', label: '删除', kind: 'crud', included: true },
  ]
  if (statusColumn) {
    actions.push({ name: 'UpdateStatus', label: '更新状态', kind: 'status', targetStatus: 'Active', included: true })
  }
  return actions
}

/** 根据表格行 + 文件名推断模型草稿 */
export function inferDraftFromRows(
  rows: Record<string, unknown>[],
  fileName: string,
): ModelDraft {
  const base = fileName.replace(/\.[^.]+$/, '')
  const asciiBase = [...base]
    .filter((c) => c.charCodeAt(0) >= 32 && c.charCodeAt(0) <= 126)
    .join('')
    .trim()
  const reverseCn = Object.entries(CN_OBJECT_NAMES).find(([, zh]) => base.includes(zh))
  const cnKey = Object.keys(CN_OBJECT_NAMES).find((k) => asciiBase.toLowerCase().includes(k))
  const objectLabel = reverseCn?.[1] ?? (cnKey ? CN_OBJECT_NAMES[cnKey] : humanize(asciiBase) || '业务对象')
  const objectName = cnKey ? pascalName(cnKey) : asciiBase ? pascalName(asciiBase) : 'BusinessObject'

  const first = rows[0] ?? {}
  const cols = Object.keys(first)
  const fields: ModelField[] = cols.map((key) => {
    const values = rows.slice(0, 100).map((r) => r[key])
    const allFilled = values.every((v) => v !== '' && v !== null && v !== undefined)
    return {
      key,
      label: CN_FIELD_LABELS[key] ?? humanize(key),
      kind: inferKind(values),
      primary: /^(id|uuid|编号|code)$/i.test(key),
      required: allFilled,
      included: true,
    }
  })

  const links: ModelLink[] = cols
    .filter((key) => /^(.+)_id$/i.test(key) && !/^(id|uuid)$/i.test(key))
    .map((key) => {
      const prefix = key.replace(/_id$/i, '').toLowerCase()
      const to = CN_OBJECT_NAMES[prefix] ?? humanize(prefix)
      return { to, label: `属于${to}`, via: key, included: true }
    })

  const hasStatus = cols.some((c) => /status|state|状态/i.test(c))
  return {
    id: crypto.randomUUID(),
    objectName,
    objectLabel,
    fields,
    links,
    actions: defaultActions(hasStatus),
    source: fileName,
    createdAt: Date.now(),
  }
}

// ── 内置模板（Starter Kit）──────────────────────────────
export const MODEL_TEMPLATES: ModelTemplate[] = [
  {
    id: 'customer',
    name: '客户管理',
    desc: '客户资料、等级与状态管理',
    draft: {
      objectName: 'Customer',
      objectLabel: '客户',
      fields: [
        { key: 'id', label: 'ID', kind: 'number', primary: true, included: true },
        { key: 'name', label: '客户名称', kind: 'text', required: true, included: true },
        { key: 'phone', label: '手机号', kind: 'text', included: true },
        { key: 'email', label: '邮箱', kind: 'text', included: true },
        { key: 'level', label: '客户等级', kind: 'text', included: true },
        { key: 'status', label: '状态', kind: 'text', included: true },
      ],
      links: [],
      actions: [
        ...defaultActions(true),
        { name: 'MarkVip', label: '标记为 VIP', kind: 'status', targetStatus: 'VIP', included: true },
      ],
    },
  },
  {
    id: 'order',
    name: '订单管理',
    desc: '订单金额、状态与客户关联',
    draft: {
      objectName: 'Order',
      objectLabel: '订单',
      fields: [
        { key: 'id', label: 'ID', kind: 'number', primary: true, included: true },
        { key: 'order_no', label: '订单号', kind: 'text', required: true, included: true },
        { key: 'customer_id', label: '客户 ID', kind: 'number', included: true },
        { key: 'amount', label: '金额', kind: 'number', included: true },
        { key: 'status', label: '状态', kind: 'text', included: true },
        { key: 'created_at', label: '创建时间', kind: 'date', included: true },
      ],
      links: [{ to: '客户', label: '属于客户', via: 'customer_id', included: true }],
      actions: [
        ...defaultActions(true),
        { name: 'Approve', label: '审核通过', kind: 'status', targetStatus: 'Approved', included: true },
        { name: 'Complete', label: '标记完成', kind: 'status', targetStatus: 'Completed', included: true },
      ],
    },
  },
  {
    id: 'article',
    name: '内容管理',
    desc: '文章创作、审核与发布',
    draft: {
      objectName: 'Article',
      objectLabel: '文章',
      fields: [
        { key: 'id', label: 'ID', kind: 'number', primary: true, included: true },
        { key: 'title', label: '标题', kind: 'text', required: true, included: true },
        { key: 'content', label: '正文', kind: 'text', included: true },
        { key: 'author', label: '作者', kind: 'text', included: true },
        { key: 'status', label: '状态', kind: 'text', included: true },
      ],
      links: [],
      actions: [
        ...defaultActions(true),
        { name: 'Approve', label: '审核通过', kind: 'status', targetStatus: 'Approved', included: true },
        { name: 'Publish', label: '发布', kind: 'status', targetStatus: 'Published', included: true },
      ],
    },
  },
  {
    id: 'project',
    name: '项目管理',
    desc: '项目进度、负责人与起止日期',
    draft: {
      objectName: 'Project',
      objectLabel: '项目',
      fields: [
        { key: 'id', label: 'ID', kind: 'number', primary: true, included: true },
        { key: 'name', label: '项目名称', kind: 'text', required: true, included: true },
        { key: 'owner', label: '负责人', kind: 'text', included: true },
        { key: 'start_date', label: '开始日期', kind: 'date', included: true },
        { key: 'end_date', label: '结束日期', kind: 'date', included: true },
        { key: 'status', label: '状态', kind: 'text', included: true },
        { key: 'progress', label: '进度', kind: 'number', included: true },
      ],
      links: [],
      actions: [
        ...defaultActions(true),
        { name: 'Start', label: '启动项目', kind: 'status', targetStatus: 'Active', included: true },
        { name: 'Complete', label: '标记完成', kind: 'status', targetStatus: 'Completed', included: true },
      ],
    },
  },
  {
    id: 'inventory',
    name: '库存管理',
    desc: '商品、数量、价格与分类',
    draft: {
      objectName: 'Inventory',
      objectLabel: '库存',
      fields: [
        { key: 'id', label: 'ID', kind: 'number', primary: true, included: true },
        { key: 'sku', label: 'SKU', kind: 'text', required: true, included: true },
        { key: 'name', label: '商品名称', kind: 'text', included: true },
        { key: 'category', label: '分类', kind: 'text', included: true },
        { key: 'quantity', label: '数量', kind: 'number', included: true },
        { key: 'unit_price', label: '单价', kind: 'number', included: true },
        { key: 'status', label: '状态', kind: 'text', included: true },
      ],
      links: [],
      actions: [
        ...defaultActions(true),
        { name: 'StockCheck', label: '标记盘点', kind: 'status', targetStatus: 'Checked', included: true },
      ],
    },
  },
]

/** 「从决定开始」入口：先选一个真实决定，再倒推最小模型 */
export interface DecisionStarter {
  id: string
  name: string
  desc: string
  objectIds: string[]
  extraAction?: { name: string; label: string }
}

export const DECISION_STARTERS: DecisionStarter[] = [
  {
    id: 'order-transfer',
    name: '订单调货',
    desc: '缺货订单从其他仓调货，涉及客户、订单与库存',
    objectIds: ['order', 'customer', 'inventory'],
    extraAction: { name: 'TransferStock', label: '发起调货' },
  },
  {
    id: 'ticket-close',
    name: '工单闭环',
    desc: '客户问题创建、处理到关闭的完整闭环',
    objectIds: ['ticket', 'customer'],
  },
  {
    id: 'project-go',
    name: '项目启动',
    desc: '项目立项、任务拆分与负责人跟进',
    objectIds: ['project', 'task', 'employee'],
  },
  {
    id: 'refund-approve',
    name: '退款审批',
    desc: '退款申请、金额校验与主管审批',
    objectIds: ['refund', 'order', 'customer'],
  },
]

// ── 对话式建模：名词 → 内置对象草稿 ─────────────────────
const EXTRA_TALK_TEMPLATES: ModelTemplate[] = [
  {
    id: 'employee',
    name: '员工',
    desc: '员工档案、部门与状态',
    draft: {
      objectName: 'Employee',
      objectLabel: '员工',
      fields: [
        { key: 'id', label: 'ID', kind: 'number', primary: true, included: true },
        { key: 'name', label: '姓名', kind: 'text', required: true, included: true },
        { key: 'department', label: '部门', kind: 'text', included: true },
        { key: 'phone', label: '手机号', kind: 'text', included: true },
        { key: 'status', label: '状态', kind: 'text', included: true },
        { key: 'hired_at', label: '入职日期', kind: 'date', included: true },
      ],
      links: [],
      actions: [...defaultActions(true)],
    },
  },
  {
    id: 'task',
    name: '任务',
    desc: '任务负责人、进度与完成',
    draft: {
      objectName: 'Task',
      objectLabel: '任务',
      fields: [
        { key: 'id', label: 'ID', kind: 'number', primary: true, included: true },
        { key: 'title', label: '任务标题', kind: 'text', required: true, included: true },
        { key: 'project_id', label: '项目 ID', kind: 'number', included: true },
        { key: 'owner', label: '负责人', kind: 'text', included: true },
        { key: 'due_date', label: '截止日期', kind: 'date', included: true },
        { key: 'status', label: '状态', kind: 'text', included: true },
      ],
      links: [{ to: '项目', label: '属于项目', via: 'project_id', included: true }],
      actions: [
        ...defaultActions(true),
        { name: 'Complete', label: '标记完成', kind: 'status', targetStatus: 'Completed', included: true },
      ],
    },
  },
  {
    id: 'ticket',
    name: '工单',
    desc: '客户问题、优先级与关闭',
    draft: {
      objectName: 'Ticket',
      objectLabel: '工单',
      fields: [
        { key: 'id', label: 'ID', kind: 'number', primary: true, included: true },
        { key: 'title', label: '问题标题', kind: 'text', required: true, included: true },
        { key: 'customer_id', label: '客户 ID', kind: 'number', included: true },
        { key: 'priority', label: '优先级', kind: 'text', included: true },
        { key: 'status', label: '状态', kind: 'text', included: true },
        { key: 'created_at', label: '创建时间', kind: 'date', included: true },
      ],
      links: [{ to: '客户', label: '属于客户', via: 'customer_id', included: true }],
      actions: [
        ...defaultActions(true),
        { name: 'Close', label: '关闭工单', kind: 'status', targetStatus: 'Closed', included: true },
      ],
    },
  },
  {
    id: 'refund',
    name: '退款',
    desc: '退款金额、原因与审核',
    draft: {
      objectName: 'Refund',
      objectLabel: '退款',
      fields: [
        { key: 'id', label: 'ID', kind: 'number', primary: true, included: true },
        { key: 'order_id', label: '订单 ID', kind: 'number', included: true },
        { key: 'amount', label: '退款金额', kind: 'number', included: true },
        { key: 'reason', label: '退款原因', kind: 'text', included: true },
        { key: 'status', label: '状态', kind: 'text', included: true },
        { key: 'created_at', label: '创建时间', kind: 'date', included: true },
      ],
      links: [{ to: '订单', label: '属于订单', via: 'order_id', included: true }],
      actions: [
        ...defaultActions(true),
        { name: 'Approve', label: '审核通过', kind: 'status', targetStatus: 'Approved', included: true },
      ],
    },
  },
]

const TALK_TEMPLATES: ModelTemplate[] = [...MODEL_TEMPLATES, ...EXTRA_TALK_TEMPLATES]

/** 中文业务名词 → 模板 id */
const TALK_NOUN_IDS: Record<string, string> = {
  客户: 'customer',
  订单: 'order',
  文章: 'article',
  内容: 'article',
  项目: 'project',
  库存: 'inventory',
  商品: 'inventory',
  产品: 'inventory',
  员工: 'employee',
  任务: 'task',
  工单: 'ticket',
  退款: 'refund',
}

/** 同时出现时自动建立的对象间关联 */
const TALK_RELATION_RULES: Array<{ from: string; to: string }> = [
  { from: '订单', to: '客户' },
  { from: '退款', to: '订单' },
  { from: '退款', to: '客户' },
  { from: '任务', to: '项目' },
  { from: '任务', to: '员工' },
  { from: '工单', to: '客户' },
]

function cloneTemplate(id: string): ModelDraft {
  const tpl = TALK_TEMPLATES.find((t) => t.id === id)
  if (!tpl) return genericDraft()
  return {
    ...tpl.draft,
    id: crypto.randomUUID(),
    source: '对话描述',
    createdAt: Date.now(),
    fields: tpl.draft.fields.map((f) => ({ ...f })),
    links: tpl.draft.links.map((l) => ({ ...l })),
    actions: tpl.draft.actions.map((a) => ({ ...a })),
  }
}

/** 把决策 Starter 展开成多对象模型项目（对象 + 动作 + 审批） */
export function buildDecisionProject(starter: DecisionStarter): ModelProject {
  const objects = starter.objectIds.map((id) => cloneTemplate(id))
  const first = objects[0]
  if (starter.extraAction && first && !first.actions.some((a) => a.name === starter.extraAction!.name)) {
    first.actions.push({
      name: starter.extraAction.name,
      label: starter.extraAction.label,
      kind: 'status',
      targetStatus: 'Transferring',
      included: true,
    })
  }
  return {
    id: crypto.randomUUID(),
    name: starter.name,
    objects,
    relationships: [],
    source: `决定：${starter.name}`,
    createdAt: Date.now(),
  }
}

function genericDraft(): ModelDraft {
  return {
    id: crypto.randomUUID(),
    objectName: 'BusinessObject',
    objectLabel: '业务对象',
    fields: [
      { key: 'id', label: 'ID', kind: 'number', primary: true, included: true },
      { key: 'name', label: '名称', kind: 'text', required: true, included: true },
      { key: 'status', label: '状态', kind: 'text', included: true },
    ],
    links: [],
    actions: defaultActions(true),
    source: '对话描述',
    createdAt: Date.now(),
  }
}

/** 从一句自然语言描述生成多对象模型项目 */
export function inferProjectFromSentence(sentence: string): ModelProject {
  const found = new Set<string>()
  for (const noun of Object.keys(TALK_NOUN_IDS)) {
    if (sentence.includes(noun)) found.add(noun)
  }

  if (found.size === 0) {
    return {
      id: crypto.randomUUID(),
      name: '业务模型',
      objects: [genericDraft()],
      relationships: [],
      source: '对话描述',
      createdAt: Date.now(),
    }
  }

  const nouns = [...found]
  const objects = nouns.map((n) => cloneTemplate(TALK_NOUN_IDS[n]))
  const relationships: ModelRelationship[] = []

  for (const rule of TALK_RELATION_RULES) {
    if (!found.has(rule.from) || !found.has(rule.to)) continue
    const toId = TALK_NOUN_IDS[rule.to]
    const via = `${toId}_id`
    relationships.push({ from: rule.from, to: rule.to, label: `属于${rule.to}`, via, included: true })
    const fromObj = objects.find((o) => o.objectLabel === rule.from)
    if (fromObj && !fromObj.fields.some((f) => f.key === via)) {
      fromObj.fields.push({
        key: via,
        label: `${rule.to} ID`,
        kind: 'number',
        included: true,
      })
    }
  }

  const name = sentence.replace(/[，,。.\s]/g, '').slice(0, 12) || '业务模型'
  return {
    id: crypto.randomUUID(),
    name,
    objects,
    relationships,
    source: '对话描述',
    createdAt: Date.now(),
  }
}

// ── ODL 生成 ───────────────────────────────────────────
const ODL_TYPE: Record<FieldKind, string> = {
  text: 'String',
  number: 'Int',
  date: 'Date',
  boolean: 'Bool',
}

/** 中文业务名 → ODL 实体名（用于跨对象 relation 的合法标识符） */
const DEFAULT_NAME_MAP: Record<string, string> = {
  客户: 'Customer',
  订单: 'Order',
  文章: 'Article',
  项目: 'Project',
  库存: 'Inventory',
  商品: 'Inventory',
  产品: 'Inventory',
  员工: 'Employee',
  任务: 'Task',
  工单: 'Ticket',
  退款: 'Refund',
}

export function draftToOdl(d: ModelDraft, nameMap: Record<string, string> = DEFAULT_NAME_MAP): string {
  const fields = d.fields.filter((f) => f.included !== false)
  const links = d.links.filter((l) => l.included !== false)
  const actions = d.actions.filter((a) => a.included !== false)
  const lines: string[] = []
  lines.push(`// 由「从数据开始」向导自动生成 · ${d.objectLabel} · ${new Date().toLocaleString('zh-CN')}`)
  lines.push(`entity ${d.objectName} {`)
  for (const f of fields) lines.push(`  property ${f.key}: ${ODL_TYPE[f.kind]}`)
  for (const l of links) {
    const target = nameMap[l.to] ?? l.to
    lines.push(`  relation ${target}BelongsTo: ${target}`)
  }
  lines.push('  crud: all')
  lines.push('}')
  for (const a of actions.filter((x) => x.kind === 'status')) {
    lines.push('')
    lines.push(`capability ${d.objectName}:${a.name} {`)
    lines.push('  kind: mutate')
    lines.push(`  target: ${d.objectName}`)
    lines.push('  scope: entity')
    lines.push('  input: id')
    lines.push(`  effect: set status = ${a.targetStatus ?? 'Active'}`)
    lines.push('}')
  }
  // 授权规则：管理员 / 超级管理员可直接执行新建对象的所有操作
  for (const role of ['admin', 'super_admin']) {
    const suffix = role === 'admin' ? 'Admin' : 'SuperAdmin'
    lines.push('')
    lines.push(`rule Grant${d.objectName}${suffix}Crud {`)
    lines.push(`  when: ?u hasRole Role:${role}`)
    lines.push(
      `  then: ?u can ${d.objectName}:Create, ?u can ${d.objectName}:Read, ?u can ${d.objectName}:Update, ?u can ${d.objectName}:Delete, ?u can ${d.objectName}:List`,
    )
    lines.push('}')
  }
  return lines.join('\n')
}

/** 多对象项目 → 合并 ODL */
export function projectToOdl(p: ModelProject): string {
  const map: Record<string, string> = { ...DEFAULT_NAME_MAP }
  for (const o of p.objects) map[o.objectLabel] = o.objectName
  return p.objects.map((o) => draftToOdl(o, map)).join('\n\n')
}
