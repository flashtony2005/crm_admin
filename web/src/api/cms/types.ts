/**
 * CMS 领域类型 —— Phase 1 固定内容模型（PRODUCT_VISION §6）。
 *
 * 决策 TD-1/TD-2：不走本体建模路线，直接固定结构；
 * 未来 ContentType 扩展优先用 JSON 字段 + 校验层，而非动态 Schema 内核。
 */

export type ContentStatus = 'draft' | 'pending_review' | 'published' | 'offline' | 'scheduled'

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
  /** 独立 SEO 标题（留空则回退用 title） */
  metaTitle?: string
  /** 独立 SEO 描述（留空则回退用 summary） */
  metaDescription?: string
  /** 可见性（付费墙）：public 公开 / members 会员专享 / paid 付费会员 */
  visibility?: string
  /** 设为精选（首页 Featured 位） */
  featured?: boolean
  /** 定时发布时间（status='scheduled' 时生效；'YYYY-MM-DD HH:mm'） */
  scheduledAt?: string
  /** 规范链接（canonical URL）；留空则用默认文章 URL */
  canonicalUrl?: string
}

export interface Page extends BaseRecord {
  title: string
  path: string
  status: ContentStatus
  views: number
}

/** 独立 Tag（P3 内容组织专业化）：描述/封面/SEO 字段 */
export interface Tag extends BaseRecord {
  name: string
  slug: string
  description: string
  coverImage?: string
  metaTitle?: string
  metaDescription?: string
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
  /** 缩略图 URL（栅格图自动生成；非图片回退为 url） */
  thumbnail?: string
  /** 大图 URL（文章正文/灯箱用；非图片回退为 url） */
  large?: string
  /** 原图像素宽 */
  width?: number
  /** 原图像素高 */
  height?: number
  /** 响应式图 srcset（上传接口返回；形如 "/uploads/x_480.jpg 480w, ..."） */
  srcset?: string
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

// ── P4 商业层类型 ──

/** 会员（Members） */
export interface Member extends BaseRecord {
  email: string
  name: string
  status: number
  /** 套餐：free / 各 tier slug */
  plan: string
  stripeCustomerId?: string
}

/** 当前登录会员（不含敏感字段） */
export interface MemberProfile {
  id: string
  email: string
  name: string
  plan: string
}

/** 评论（Comments） */
export interface Comment extends BaseRecord {
  articleId: string
  parentId?: string
  authorName: string
  authorEmail?: string
  memberId?: string
  content: string
  status: 'approved' | 'pending' | 'rejected' | 'spam'
  createdAt: string
}

/** 邮件订阅者（Newsletter） */
export interface Subscriber extends BaseRecord {
  email: string
  name: string
  status: 'active' | 'unsubscribed'
}

/** 付费套餐（Subscriptions / Tiers） */
export interface Tier extends BaseRecord {
  name: string
  slug: string
  description: string
  priceMonthly: number
  priceYearly: number
  stripePriceId?: string
  features: string
  active: boolean
}

/** 出站 Webhook 订阅 */
export interface WebhookSubscription extends BaseRecord {
  event: string
  url: string
  secret: string
  active: boolean
  deliveries?: number
}

/** 多语言翻译字典 */
export type LocaleMessages = Record<string, string>
export type Locale = 'zh' | 'en'
