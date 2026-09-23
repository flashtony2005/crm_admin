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
  /** 售卖分级（优先于 visibility）：0 公开 / 1 订阅会员 / 2 积分买断 / 3 邀请专享 */
  paidLevel?: number
  /** 积分解锁单价（paidLevel=2 时生效，P1 积分商城消费） */
  pricePoints?: number
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
/** SEO 重定向（P0-1，对标 Rank Math 的重定向管理）。 */
export interface Redirect extends BaseRecord {
  /** 完整请求路径（含 /t/<slug> 前缀）—— 与 404 日志口径一致，可直接互相转换 */
  fromPath: string
  /** 站点相对路径 /xxx 或 http(s) 绝对地址；410 时可不填 */
  toPath: string
  /** 301 / 302 / 307 / 308 / 410 */
  code: number
  note: string
  /** 命中次数，只增不减：判断这条规则是否真在救流量 */
  hits: number
  enabled: boolean
}

/** 404 监控记录（P0-1）。公开站渲染「未找到」时前端上报，同路径只累加 hits。 */
export interface NotFoundLog extends BaseRecord {
  path: string
  referer: string
  ua: string
  hits: number
  lastSeen: string
  /** 已处理（通常表示已建重定向或确认无需处理） */
  resolved: boolean
}

/**
 * Smart Link（P0-4）：对外发的短链 `/go/{token}`。
 *
 * 计两个数：`clicks` 是真人点击，`prefetch` 是邮件网关/IM 的链接预览抓取。
 * **必须分开**：一封群发邮件能在无人点开前就把链接抓几十遍，
 * 混在一起算，这个数字从一开始就是假的。
 */
export interface SmartLink extends BaseRecord {
  /** 短链尾段（`/go/{token}`），只允许字母数字与 `-` `_` */
  token: string
  /** 目标地址，仅 http(s) */
  url: string
  /** 备注名，列表里认人用 */
  label: string
  /** 逗号分隔；点击者会自动获得这些标签 */
  tags: string
  enabled: boolean
  /** 真人点击数（已剔除预取） */
  clicks: number
  /** 预取次数（机器人抓取，不计入 clicks） */
  prefetch: number
  /** 最近一次真人点击时间 */
  lastClickAt: string | null
}

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
  /** 邀请人 member id（非空 = 凭邀请码注册加入） */
  invitedBy?: string
  /**
   * 归因三列（后端**只读**，由公开站注册流程写入）。
   * 后台只能读、不能改 —— 这类「当初真实发生了什么」的数据一旦可编辑，
   * 归因结论就不可信了。
   */
  visitorId?: string
  /** 首触文章 id（不是末次：末次会被站内推荐位改写） */
  firstTouchArticleId?: string
  firstTouchAt?: string
}

/** 邀请码（额度制：好友凭码注册核销 used+1） */
export interface InviteCode extends BaseRecord {
  code: string
  /** 邀请人 member id（后台生成的运营码可留空） */
  ownerMemberId?: string
  /** 总额度 */
  quota: number
  /** 已核销次数（注册时自动 +1） */
  used: number
  /** 过期时间（ISO 字符串；空 = 永不过期） */
  expiresAt?: string
  enabled: boolean
}

/** 兑换码（一次性：kind points=充积分 / plan_days=会员天数） */
export interface RedeemCode extends BaseRecord {
  code: string
  kind: 'points' | 'plan_days' | string
  value: number
  status: 'unused' | 'used' | string
  usedBy?: string
  usedAt?: string
}

/** 订单（人工确认收款；channel 预留 manual/code/wechat） */
export interface Order extends BaseRecord {
  orderNo: string
  memberId: string
  bizType: 'points_recharge' | 'plan' | string
  tierId?: string
  points?: number
  planDays?: number
  amountCents: number
  channel: string
  status: 'pending' | 'paid' | 'closed' | string
  refNo?: string
  paidAt?: string
}

/** 积分流水行 */
export interface LedgerEntry {
  delta: number
  balanceAfter: number
  reason: string
  refId: string
  note: string
  createdAt: string
}

/** 会员钱包（GET /api/public/members/wallet） */
export interface WalletInfo {
  balance: number
  plan: string
  planExpiresAt: string
  /** 付费计划剩余天数（-1 = 免费/无期限） */
  planDaysLeft?: number
  /** 7 天内到期 */
  planExpiring?: boolean
  /** 已过期（subscribed() 已视为无效） */
  planExpired?: boolean
  invited: boolean
  signedToday: boolean
  signinPoints: number
  inviteRewardPoints: number
  ledger: LedgerEntry[]
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
  /** 年付专用的 Stripe Price（迁移 0016）。月/年各自独立，缺年价时在线年付会被服务端拒绝 */
  stripePriceYearlyId?: string
  features: string
  active: boolean
  /** 公开套餐接口下发的「该周期能否在线支付」（仅 /api/public/tiers，不下发 Price ID 本身） */
  onlineMonthly?: boolean
  onlineYearly?: boolean
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

/** 微信支付配置查看（脱敏；P2 在线支付） */
export interface PayConfigInfo {
  appid: string
  mchid: string
  serialNo: string
  apiV3KeySet: boolean
  apiV3KeyMasked: string
  privateKeySet: boolean
  platformPubKeySet: boolean
  ready: boolean
}

/** 对账分组行：日期 × 渠道 × 状态 */
export interface ReconRow {
  date: string
  channel: string
  status: string
  count: number
  amountCents: number
}

/** 对账响应（窗口内汇总） */
export interface ReconResp {
  days: number
  rows: ReconRow[]
  summary: Record<string, { count: number; amountCents: number }>
}

/** 审计日志行（P2：/api 写操作留痕） */
export interface AuditRow {
  userId: string
  username: string
  method: string
  path: string
  status: number
  durationMs: number
  requestId: string
  createdAt: string
}

/** 发货未完成项（outbox，F2 防漏发货） */
export interface FulfillTask {
  orderId: string
  orderNo: string
  stageKey: string
  stage: string
  status: string
  attempts: number
  lastError: string
  updatedAt: string
}

/** 重试发货结果 */
export interface RetryFulfillResp {
  retried: number
  recovered: number
  stillFailing: string[]
}
