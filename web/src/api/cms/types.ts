/**
 * CMS 领域类型 —— Phase 1 固定内容模型（PRODUCT_VISION §6）。
 *
 * 决策 TD-1/TD-2：不走本体建模路线，直接固定结构；
 * 未来 ContentType 扩展优先用 JSON 字段 + 校验层，而非动态 Schema 内核。
 */

export type ContentStatus = 'draft' | 'pending_review' | 'published' | 'offline'

export type LeadStatus = 'new' | 'following' | 'won' | 'lost'
export type CustomerPriority = 'high' | 'mid' | 'low'
export type ApprovalStatus = 'pending' | 'approved' | 'rejected'
export type AiTaskStatus = 'running' | 'waiting_approval' | 'done' | 'failed'

/** 所有实体共享的最小痕迹字段（审计起点，Phase 3 扩展为完整 Audit） */
export interface BaseRecord {
  id: string
  createdAt: string
  updatedAt: string
}

export interface Article extends BaseRecord {
  title: string
  slug: string
  summary: string
  category: string
  status: ContentStatus
  views: number
  author: string
  /** 富文本正文（HTML 字符串，图片以内联 data URL 形式存储） */
  content: string
  /** 标签：后端以逗号分隔字符串存储；前端提交时用英文逗号分隔 */
  tags?: string
  /** 封面图（URL 或内联 data URL） */
  featuredImage?: string
  /** 计划发布时间（定时发布元数据；ISO 字符串或 'YYYY-MM-DD HH:mm'） */
  publishedAt?: string
}

export interface Page extends BaseRecord {
  title: string
  path: string
  status: ContentStatus
  views: number
}

export interface Product extends BaseRecord {
  name: string
  sku: string
  price: number
  stock: number
  status: ContentStatus
}

export interface MediaItem extends BaseRecord {
  name: string
  type: 'image' | 'video' | 'file'
  sizeKb: number
  url: string
}

export interface Customer extends BaseRecord {
  name: string
  phone: string
  source: string
  tags: string[]
  priority: CustomerPriority
  note: string
  lastContactAt: string
}

export interface Lead extends BaseRecord {
  name: string
  phone: string
  interest: string
  source: string
  status: LeadStatus
}

export interface FormDef extends BaseRecord {
  title: string
  descr?: string
  fieldCount: number
  submissions: number
  status: 'open' | 'published' | 'closed'
}

export interface Approval extends BaseRecord {
  /** 操作类型：发布 / 更新 / 删除 */
  action: 'publish' | 'update' | 'delete'
  target: string
  requestedBy: string
  risk: 'low' | 'mid' | 'high'
  status: ApprovalStatus
  summary: string
  decidedAt?: string
}

export interface AiTask extends BaseRecord {
  title: string
  /** AI capability 名（对用户隐藏细节，仅作展示标签用） */
  capability: string
  status: AiTaskStatus
  result?: string
}

/** 可视化节点编辑器中的一个节点（对应后端 workflows.steps JSON 数组中的元素）
 *  约定：节点本身即 steps 数组元素，后端执行引擎按 type/message/title 驱动，
 *  其余字段（label/x/y/next）仅供可视化编辑器使用。 */
export interface WorkflowNode {
  id: string
  /** 节点类型：trigger / notify / task / delay / webhook / condition */
  type: string
  /** 展示名 */
  label: string
  /** 通知内容（type=notify）或任务标题（type=task），支持 {字段} 模板 */
  message?: string
  /** 画布坐标 */
  x: number
  y: number
  /** 后继节点 id（连线），按顺序执行 */
  next?: string[]
}

export interface WorkflowDef extends BaseRecord {
  name: string
  trigger: string
  /** 订阅的事件类型（automation::trigger 入口；manual = 仅手动） */
  event: string
  stepCount: number
  enabled: boolean
  lastRunAt?: string
  /** 可视化编辑器保存的节点数组（后端 steps 列，JSON） */
  steps?: WorkflowNode[]
}

export interface Integration extends BaseRecord {
  key: string
  name: string
  category: 'seo' | 'analytics' | 'message' | 'commerce' | 'crm'
  desc: string
  connected: boolean
  /** OAuth2 provider（google/github）；缺省 = API Key 方式 */
  oauthProvider?: string
  oauthClientId?: string
  oauthClientSecret?: string
}

/** 表单字段描述（驱动 CmsFormModal 的通用 schema） */
export interface FormFieldDef {
  key: string
  label: string
  type: 'text' | 'textarea' | 'number' | 'select' | 'richtext'
  options?: { value: string; label: string }[]
  required?: boolean
  placeholder?: string
  defaultValue?: string | number
  /** richtext 类型专用：编辑区最小高度（px） */
  height?: number
}
