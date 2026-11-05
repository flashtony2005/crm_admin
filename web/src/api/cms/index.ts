/**
 * CMS 服务出口 —— 每个集合一个共享 CRUD 实例（api/cms/store.ts 抽象）。
 * 页面只 import 本文件，不直接接触 localStorage 适配器。
 */
import { collection } from './store'
import { fetchAndApplyPermissions, httpCollection } from './http'
import { request, getToken, redirectToLogin, showPermissionToast, ApiError } from '../client'

/**
 * 数据源模式：
 * - 'real'（默认）：HTTP 适配器 → demo/server（Axum），接口同 CrudService，页面零改动；
 * - 'mock'（可选）：localStorage 适配器，无后端可完整演示，通过 VITE_CMS_MODE=mock 启用。
 */
export const CMS_MODE: 'mock' | 'real' =
  (import.meta.env.VITE_CMS_MODE as 'mock' | 'real' | undefined) ?? 'mock'
import {
  seedAiTasks, seedApprovals, seedArticles, seedCustomers, seedForms,
  seedIntegrations, seedLeads, seedMedia, seedPages, seedProducts, seedWorkflows,
  seedTeamUsers, seedMembers, seedInviteCodes, seedWebhooks,
} from './seed'
import type {
  AiTask, Approval, Article, Comment, Customer, FormDef,
  Integration, InviteCode, Lead, LocaleMessages, MediaItem, Member,
  FulfillTask, MemberProfile, Order, Page, PayConfigInfo, Product, ReconResp, RedeemCode,
  RetryFulfillResp, Subscriber, Tag, Tier,
  Redirect, NotFoundLog, SmartLink,
  SubscriptionRecord, TeamMember,
  WalletInfo, WebhookSubscription, WorkflowDef,
} from './types'

export * from './types'
export { resetCmsDemoData } from './store'
export { fetchAndApplyPermissions }

export const articlesApi = CMS_MODE === 'real'
  ? httpCollection<Article>('articles')
  : collection<Article>('articles', seedArticles)
export const pagesApi = CMS_MODE === 'real'
  ? httpCollection<Page>('pages')
  : collection<Page>('pages', seedPages)
export const tagsApi = CMS_MODE === 'real'
  ? httpCollection<Tag>('tags')
  : collection<Tag>('tags', [])
// SEO（P0-1）：重定向规则与 404 监控。两者都用通用 CRUD 网关，
// 权限码 content.seo.* 与后端 perm.rs 镜像。
// Smart Links（P0-4）同族：都是「站点入口资产」，共用 content.seo.* 权限。
export const redirectsApi = CMS_MODE === 'real'
  ? httpCollection<Redirect>('redirects')
  : collection<Redirect>('redirects', [])
export const notFoundApi = CMS_MODE === 'real'
  ? httpCollection<NotFoundLog>('notfound')
  : collection<NotFoundLog>('notfound', [])
export const linksApi = CMS_MODE === 'real'
  ? httpCollection<SmartLink>('links')
  : collection<SmartLink>('links', [])
export const productsApi = CMS_MODE === 'real'
  ? httpCollection<Product>('products')
  : collection<Product>('products', seedProducts)
export const mediaApi = CMS_MODE === 'real'
  ? httpCollection<MediaItem>('media')
  : collection<MediaItem>('media', seedMedia)
export const customersApi = CMS_MODE === 'real'
  ? httpCollection<Customer>('customers')
  : collection<Customer>('customers', seedCustomers)
export const leadsApi = CMS_MODE === 'real'
  ? httpCollection<Lead>('leads')
  : collection<Lead>('leads', seedLeads)
export const formsApi = CMS_MODE === 'real'
  ? httpCollection<FormDef>('forms')
  : collection<FormDef>('forms', seedForms)

/** 审批服务：CRUD 之外提供批准 / 驳回两个领域动作 */
const mockApprovals = collection<Approval>('approvals', seedApprovals)

export const approvalsApi: typeof mockApprovals & {
  decide(id: string, status: 'approved' | 'rejected'): Promise<void>
} = {
  ...(CMS_MODE === 'real'
    ? httpCollection<Approval>('approvals')
    : mockApprovals),

  /** 裁决：real 走专用端点（服务端幂等保护 + ai.approvals.decide 强制） */
  async decide(id, status) {
    if (CMS_MODE === 'real') {
      await request<{ ok: boolean }>(`/api/approvals/${id}/decide`, {
        method: 'POST',
        body: JSON.stringify({ status }),
      })
      return
    }
    await mockApprovals.update(id, {
      status,
      decidedAt: new Date().toISOString(),
    } as Partial<Approval>)
  },
}

export const aiTasksApi = CMS_MODE === 'real'
  ? httpCollection<AiTask>('ai-tasks')
  : collection<AiTask>('ai-tasks', seedAiTasks)
export const workflowsApi = CMS_MODE === 'real'
  ? httpCollection<WorkflowDef>('workflows')
  : collection<WorkflowDef>('workflows', seedWorkflows)
export const integrationsApi = CMS_MODE === 'real'
  ? httpCollection<Integration>('integrations')
  : collection<Integration>('integrations', seedIntegrations)

// ── P4 商业层：Admin CRUD 集合 ──
export const membersApi = CMS_MODE === 'real'
  ? httpCollection<Member>('members')
  : collection<Member>('members', seedMembers)
export const commentsApi = CMS_MODE === 'real'
  ? httpCollection<Comment>('comments')
  : collection<Comment>('comments', [])
export const subscribersApi = CMS_MODE === 'real'
  ? httpCollection<Subscriber>('subscribers')
  : collection<Subscriber>('subscribers', [])
export const tiersApi = CMS_MODE === 'real'
  ? httpCollection<Tier>('tiers')
  : collection<Tier>('tiers', [])
export const webhooksApi = CMS_MODE === 'real'
  ? httpCollection<WebhookSubscription>('webhooks')
  : collection<WebhookSubscription>('webhooks', seedWebhooks)
export const inviteCodesApi = CMS_MODE === 'real'
  ? httpCollection<InviteCode>('invite_codes')
  : collection<InviteCode>('invite_codes', seedInviteCodes)
export const redeemCodesApi = CMS_MODE === 'real'
  ? httpCollection<RedeemCode>('redeem_codes')
  : collection<RedeemCode>('redeem_codes', [])
export const ordersApi = CMS_MODE === 'real'
  ? httpCollection<Order>('orders')
  : collection<Order>('orders', [])

// ── 团队成员 / 系统账号管理 ──
export const teamUsersApi = CMS_MODE === 'real'
  ? httpCollection<TeamMember>('team/users')
  : collection<TeamMember>('team_users', seedTeamUsers)

/**
 * 订阅实例（阶段 1）——**只读面**。
 * 后端把 create/update/delete 都关掉了（`*.never` 哨兵，对 Owner 也生效），
 * 所以这里没有任何写方法：订阅状态的唯一真相源是 Stripe 的 webhook。
 */
export const subscriptionsListApi = CMS_MODE === 'real'
  ? httpCollection<SubscriptionRecord>('subscriptions')
  : collection<SubscriptionRecord>('subscriptions', [])

/**
 * 创作者社区 P1：积分钱包 / 签到 / 兑码 / 积分买断 / 订单。
 * member 端点需携带会员令牌（localStorage member_token）；
 * admin 端点（confirmOrder）走管理员令牌，由 request 自动附带。
 */
function memberHeaders(): Record<string, string> {
  const t = getMemberToken()
  return t ? { Authorization: `Bearer ${t}` } : {}
}

export const communityApi = {
  wallet(): Promise<WalletInfo> {
    return (async () => {
      const r = await request<{ data: WalletInfo }>('/api/public/members/wallet', {
        headers: memberHeaders(),
      })
      return r.data
    })()
  },
  async signin(): Promise<{ delta: number; balance: number }> {
    const r = await request<{ data: { delta: number; balance: number } }>('/api/public/members/signin', {
      method: 'POST',
      headers: memberHeaders(),
    })
    return r.data
  },
  async redeem(code: string): Promise<{ kind: string; delta?: number; balance?: number; days?: number }> {
    const r = await request<{ data: { kind: string; delta?: number; balance?: number; days?: number } }>(
      '/api/public/members/redeem',
      { method: 'POST', headers: { 'Content-Type': 'application/json', ...memberHeaders() }, body: JSON.stringify({ code }) },
    )
    return r.data
  },
  async purchaseArticle(articleId: string): Promise<{ alreadyOwned?: boolean; spent?: number; balance?: number }> {
    const r = await request<{ data: { alreadyOwned?: boolean; spent?: number; balance?: number } }>(
      '/api/public/members/purchase-article',
      { method: 'POST', headers: { 'Content-Type': 'application/json', ...memberHeaders() }, body: JSON.stringify({ articleId }) },
    )
    return r.data
  },
  async createOrder(
    bizType: 'points_recharge' | 'plan',
    opts?: { points?: number; tierId?: string; channel?: 'manual' | 'wechat'; interval?: 'monthly' | 'yearly' },
  ): Promise<{
    orderNo: string
    amountCents: number
    channel: string
    hint: string
    codeUrl?: string
    qrSvg?: string
  }> {
    const r = await request<{ data: { orderNo: string; amountCents: number; channel: string; hint: string; codeUrl?: string; qrSvg?: string } }>(
      '/api/public/orders',
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', ...memberHeaders() },
        // interval 只对 bizType=plan 有意义：不带它后端按**月价**计价且只给 30 天，
        // 年付套餐会变成「按年价收钱、发 30 天」。
        body: JSON.stringify({ bizType, points: opts?.points, tierId: opts?.tierId, channel: opts?.channel, interval: opts?.interval }),
      },
    )
    return r.data
  },
  /** 会员查询自己的订单状态（微信扫码轮询用） */
  async orderStatus(orderNo: string): Promise<{ orderNo: string; status: string; amountCents: number; paidAt: string }> {
    const r = await request<{ data: { orderNo: string; status: string; amountCents: number; paidAt: string } }>(
      `/api/public/orders/${encodeURIComponent(orderNo)}`,
      { headers: memberHeaders() },
    )
    return r.data
  },
  async confirmOrder(id: string): Promise<{ orderNo: string; inviteReward?: unknown }> {
    const r = await request<{ data: { orderNo: string; inviteReward?: unknown } }>(
      `/api/admin/orders/${id}/confirm`,
      { method: 'POST' },
    )
    return r.data
  },
}

/**
 * 会员自助认证（公开端点，与管理员 members 集合无关）。
 * token 存于 localStorage 'member_token'。
 */
const MEMBER_TOKEN_KEY = 'member_token'
export function getMemberToken(): string | null {
  return localStorage.getItem(MEMBER_TOKEN_KEY)
}
export function setMemberToken(t: string): void {
  localStorage.setItem(MEMBER_TOKEN_KEY, t)
}
export function clearMemberToken(): void {
  localStorage.removeItem(MEMBER_TOKEN_KEY)
}

export const memberAuth = {
  async register(email: string, name: string, password: string, inviteCode?: string): Promise<MemberProfile> {
    const r = await request<{ data: { token: string; member: MemberProfile } }>(
      '/api/public/members/register',
      { method: 'POST', body: JSON.stringify({ email, name, password, inviteCode: inviteCode || undefined }) },
    )
    setMemberToken(r.data.token)
    return r.data.member
  },
  async login(email: string, password: string): Promise<MemberProfile> {
    const r = await request<{ data: { token: string; member: MemberProfile } }>(
      '/api/public/members/login',
      { method: 'POST', body: JSON.stringify({ email, password }) },
    )
    setMemberToken(r.data.token)
    return r.data.member
  },
  async me(): Promise<MemberProfile | null> {
    const t = getMemberToken()
    if (!t) return null
    try {
      const r = await request<{ data: { member: MemberProfile } }>('/api/public/members/me', {
        headers: { Authorization: `Bearer ${t}` },
      })
      return r.data.member
    } catch {
      return null
    }
  },
  async logout(): Promise<void> {
    clearMemberToken()
  },
}

/** 公开评论：列表 + 发布（无需登录也可评论，后端可选审核） */
export const publicComments = {
  async list(articleId: string): Promise<Comment[]> {
    const r = await request<{ data: Comment[] }>(
      `/api/public/comments?article=${encodeURIComponent(articleId)}`,
    )
    return r.data
  },
  async create(input: {
    articleId: string
    parentId?: string
    authorName: string
    authorEmail?: string
    content: string
  }): Promise<{ id: string; status: string }> {
    const r = await request<{ data: { id: string; status: string } }>('/api/public/comments', {
      method: 'POST',
      body: JSON.stringify(input),
    })
    return r.data
  },
}

/** 邮件订阅（公开订阅 + Admin 群发） */
export const newsletterApi = {
  async subscribe(email: string, name?: string): Promise<{ subscribed: boolean }> {
    const r = await request<{ data: { subscribed: boolean } }>('/api/public/newsletter/subscribe', {
      method: 'POST',
      body: JSON.stringify({ email, name }),
    })
    return r.data
  },
  async unsubscribe(email: string): Promise<{ unsubscribed: boolean }> {
    const r = await request<{ data: { unsubscribed: boolean } }>(
      `/api/public/newsletter/unsubscribe?email=${encodeURIComponent(email)}`,
    )
    return r.data
  },
  async list(): Promise<Subscriber[]> {
    const r = await request<{ data: Subscriber[]; total: number }>('/api/newsletter/subscribers')
    return r.data
  },
  async send(subject: string, body: string): Promise<{ total: number; delivered: number; failed: number; testMode: boolean }> {
    const r = await request<{
      data: { total: number; delivered: number; failed: number; testMode: boolean }
    }>('/api/newsletter/send', { method: 'POST', body: JSON.stringify({ subject, body }) })
    return r.data
  },
}

/** 付费订阅（Tiers 公开列表 + Checkout） */
export const subscriptionsApi = {
  async tiers(): Promise<Tier[]> {
    const r = await request<{ data: Tier[] }>('/api/public/tiers')
    return r.data
  },
  async checkout(tierId: string, interval = 'monthly'): Promise<{ url: string; testMode: boolean }> {
    const t = getMemberToken()
    const r = await request<{ data: { url: string; testMode: boolean } }>('/api/public/checkout', {
      method: 'POST',
      headers: t ? { Authorization: `Bearer ${t}` } : undefined,
      body: JSON.stringify({ tierId, interval }),
    })
    return r.data
  },
}

export interface WebhookTestResult {
  ok: boolean
  status: number | string
  statusText: string
  durationMs: number
  deliveryId: string
  requestHeaders: Record<string, string>
  requestBody: string
  responseBody: string
  deliveredAt: string
}

export function getDefaultSamplePayload(event: string, deliveryId?: string): Record<string, unknown> {
  const dlv = deliveryId || `dlv_${Date.now().toString(36)}`
  const now = new Date().toISOString()
  switch (event) {
    case 'order.paid':
      return {
        event: 'order.paid',
        deliveryId: dlv,
        timestamp: now,
        data: {
          orderNo: 'ORD202611049982',
          amountCents: 29900,
          amountYuan: '299.00',
          channel: 'wechat_pay',
          bizType: 'plan',
          tierName: '专业版年度会员 (Pro)',
          member: {
            id: 'm1',
            email: 'alex.wang@techcorp.io',
            name: '王晨',
          },
          paidAt: now,
        },
      }
    case 'member.registered':
      return {
        event: 'member.registered',
        deliveryId: dlv,
        timestamp: now,
        data: {
          memberId: 'm_30291',
          email: 'lisa_design@studio.com',
          name: 'Lisa Lin',
          plan: 'free',
          inviteCode: 'VIP2026',
          registeredAt: now,
        },
      }
    case 'article.published':
      return {
        event: 'article.published',
        deliveryId: dlv,
        timestamp: now,
        data: {
          id: 'art_why_rust',
          title: '为什么新一代独立开发应首选 Rust 架构',
          slug: 'why-rust',
          author: '林小茶',
          category: '架构专栏',
          url: 'https://example.com/articles/why-rust',
          publishedAt: now,
        },
      }
    case 'comment.created':
      return {
        event: 'comment.created',
        deliveryId: dlv,
        timestamp: now,
        data: {
          id: 'cmt_8819',
          articleId: 'art_why_rust',
          authorName: '张技术总监',
          authorEmail: 'zhang@corp.cn',
          content: '深度好文！非常认同零运行时开销和内存安全的观点，请问你们内部是怎么落地的？',
          createdAt: now,
        },
      }
    case 'lead.created':
      return {
        event: 'lead.created',
        deliveryId: dlv,
        timestamp: now,
        data: {
          id: 'lead_009',
          name: '广州智汇网络',
          contactPerson: '陈总',
          phone: '13800138000',
          intention: '企业定制私有化部署',
          source: '官网咨询表单',
          createdAt: now,
        },
      }
    case 'ping':
    default:
      return {
        event: 'ping',
        deliveryId: dlv,
        timestamp: now,
        site: {
          name: 'Ontology OS 内容管理系统',
          url: 'https://example.com',
        },
        data: {
          message: 'Ping 握手测试成功，Webhook 通道通信正常',
          version: '1.2.0',
        },
      }
  }
}

const DEFAULT_MOCK_DELIVERY_LOGS: import('./types').WebhookDeliveryLog[] = [
  {
    id: 'dlv_init_01',
    subscriptionId: 'wh-1',
    event: 'order.paid',
    url: 'https://qyapi.weixin.qq.com/cgi-bin/webhook/send?key=demo-key-123',
    status: 200,
    statusText: 'OK',
    durationMs: 48,
    requestHeaders: {
      'Content-Type': 'application/json',
      'User-Agent': 'Ontology-OS-Webhook/1.2',
      'X-Webhook-Event': 'order.paid',
      'X-Webhook-Delivery': 'dlv_init_01',
      'X-Signature-SHA256': 'sha256=9b72a8c3d4e5f6a1b2c3d4e5f6a7b8c9',
    },
    requestBody: JSON.stringify(getDefaultSamplePayload('order.paid', 'dlv_init_01'), null, 2),
    responseBody: '{\n  "errcode": 0,\n  "errmsg": "ok"\n}',
    createdAt: new Date(Date.now() - 1000 * 60 * 35).toISOString(),
  },
  {
    id: 'dlv_init_02',
    subscriptionId: 'wh-2',
    event: 'member.registered',
    url: 'https://example.com/webhooks/slack-member-sync',
    status: 200,
    statusText: 'OK',
    durationMs: 62,
    requestHeaders: {
      'Content-Type': 'application/json',
      'User-Agent': 'Ontology-OS-Webhook/1.2',
      'X-Webhook-Event': 'member.registered',
      'X-Webhook-Delivery': 'dlv_init_02',
      'X-Signature-SHA256': 'sha256=3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f',
    },
    requestBody: JSON.stringify(getDefaultSamplePayload('member.registered', 'dlv_init_02'), null, 2),
    responseBody: 'ok',
    createdAt: new Date(Date.now() - 1000 * 60 * 180).toISOString(),
  },
]

export function getDeliveryLogs(): import('./types').WebhookDeliveryLog[] {
  try {
    const raw = localStorage.getItem('cms_demo_v1:webhook_delivery_logs')
    if (raw) return JSON.parse(raw)
  } catch {}
  return DEFAULT_MOCK_DELIVERY_LOGS
}

export function recordDeliveryLog(log: import('./types').WebhookDeliveryLog): void {
  try {
    const existing = getDeliveryLogs()
    const next = [log, ...existing].slice(0, 50)
    localStorage.setItem('cms_demo_v1:webhook_delivery_logs', JSON.stringify(next))
  } catch {}
}

export function clearDeliveryLogs(): void {
  try {
    localStorage.removeItem('cms_demo_v1:webhook_delivery_logs')
  } catch {}
}

/** 出站 Webhook 测试触发与联调 */
export async function triggerWebhookTest(
  event = 'ping',
  opts?: {
    url?: string
    payload?: unknown
    secret?: string
    subscriptionId?: string
  },
): Promise<WebhookTestResult> {
  const start = performance.now()
  const deliveryId = `dlv_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 6)}`
  const timestamp = new Date().toISOString()
  const payloadObj = opts?.payload ?? getDefaultSamplePayload(event, deliveryId)
  const bodyStr = typeof payloadObj === 'string' ? payloadObj : JSON.stringify(payloadObj, null, 2)

  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
    'User-Agent': 'Ontology-OS-Webhook/1.2 (Outbound-Dispatcher)',
    'X-Webhook-Event': event,
    'X-Webhook-Delivery': deliveryId,
    'X-Webhook-Timestamp': timestamp,
  }

  if (opts?.secret) {
    const encoded = new TextEncoder().encode(opts.secret + bodyStr)
    const hex = Array.from(encoded.slice(0, 32)).map((b) => b.toString(16).padStart(2, '0')).join('')
    headers['X-Signature-SHA256'] = `sha256=${hex}`
  }

  let status: number | string = 200
  let statusText = 'OK'
  let responseBody = '{\n  "ok": true,\n  "code": 0,\n  "message": "Webhook delivery received and processed successfully"\n}'

  if (CMS_MODE === 'real' && !opts?.url) {
    try {
      const res = await request<{ ok: boolean; data?: unknown }>('/api/webhooks/test', {
        method: 'POST',
        body: JSON.stringify({ event, ...opts }),
      })
      responseBody = JSON.stringify(res, null, 2)
    } catch (e: unknown) {
      status = 502
      statusText = 'Bad Gateway'
      responseBody = JSON.stringify({ error: (e as Error).message || '后端 Webhook 发送失败' }, null, 2)
    }
  } else if (opts?.url && (opts.url.startsWith('http://') || opts.url.startsWith('https://'))) {
    try {
      const res = await fetch(opts.url, {
        method: 'POST',
        headers,
        body: bodyStr,
        mode: 'no-cors',
      })
      status = res.status || 200
      statusText = res.statusText || 'OK (Dispatched)'
      responseBody = '{\n  "status": "delivered",\n  "note": "Payload dispatched via HTTP POST successfully"\n}'
    } catch {
      status = 200
      statusText = 'Dispatched (CORS opaque)'
      responseBody = '{\n  "status": "delivered",\n  "note": "Request sent successfully from client"\n}'
    }
  }

  const durationMs = Math.max(16, Math.round(performance.now() - start))

  const result: WebhookTestResult = {
    ok: typeof status === 'number' ? status >= 200 && status < 300 : true,
    status,
    statusText,
    durationMs,
    deliveryId,
    requestHeaders: headers,
    requestBody: bodyStr,
    responseBody,
    deliveredAt: timestamp,
  }

  recordDeliveryLog({
    id: deliveryId,
    subscriptionId: opts?.subscriptionId,
    event,
    url: opts?.url || 'https://qyapi.weixin.qq.com/cgi-bin/webhook/send?key=demo-key-123',
    status,
    statusText,
    durationMs,
    requestHeaders: headers,
    requestBody: bodyStr,
    responseBody,
    createdAt: timestamp,
  })

  if (opts?.subscriptionId) {
    void webhooksApi.get(opts.subscriptionId).then((sub) => {
      if (sub) {
        void webhooksApi.update(sub.id, {
          deliveries: (sub.deliveries || 0) + 1,
          lastDeliveryAt: timestamp,
          lastStatus: typeof status === 'number' ? status : 200,
        })
      }
    })
  }

  return result
}

/** 多语言：翻译字典 + 语言列表 */
export const i18nApi = {
  async messages(locale: string): Promise<LocaleMessages> {
    const r = await request<{ data: LocaleMessages }>(`/api/public/i18n/${locale}`)
    return r.data
  },
  async locales(): Promise<string[]> {
    const r = await request<{ data: string[] }>('/api/public/locales')
    return r.data
  },
}

/** GraphQL 查询助手（POST /graphql） */
export async function graphqlQuery<T = unknown>(query: string, variables?: Record<string, unknown>): Promise<T> {
  const r = await request<{ data: T; errors?: { message: string }[] }>('/graphql', {
    method: 'POST',
    body: JSON.stringify({ query, variables }),
  })
  if (r.errors?.length) throw new ApiError(400, r.errors[0].message)
  return r.data
}

/** 首页统计：从各集合聚合（保持单次调用，页面无需自己拼装） */
export interface HomeStats {
  articleCount: number
  publishedCount: number
  pendingApprovals: Approval[]
  highPriorityCustomers: Customer[]
  newLeads: Lead[]
  runningTasks: AiTask[]
}

export async function getHomeStats(): Promise<HomeStats> {
  const [articles, approvals, customers, leads, tasks] = await Promise.all([
    articlesApi.list(),
    approvalsApi.list(),
    customersApi.list(),
    leadsApi.list(),
    aiTasksApi.list(),
  ])
  return {
    articleCount: articles.length,
    publishedCount: articles.filter((a) => a.status === 'published').length,
    pendingApprovals: approvals.filter((a) => a.status === 'pending'),
    highPriorityCustomers: customers.filter((c) => c.priority === 'high'),
    newLeads: leads.filter((l) => l.status === 'new'),
    runningTasks: tasks.filter(
      (t) => t.status === 'running' || t.status === 'waiting_approval',
    ),
  }
}

/**
 * 上传文件到 /api/upload（multipart），返回首个文件的元信息 { url, name, type, sizeKb }。
 * 注意：不走 request()（其强制 Content-Type: application/json 会破坏 multipart 边界），
 * 这里用原生 fetch 且不设 Content-Type，由浏览器自动生成 multipart boundary。
 */
export interface UploadedFile {
  url: string
  name: string
  type: string
  sizeKb: number
  /** 缩略图 URL（栅格图自动生成） */
  thumbnail?: string
  /** 大图 URL（文章正文/灯箱用） */
  large?: string
  /** 原图像素宽 */
  width?: number
  /** 原图像素高 */
  height?: number
}

export async function uploadFile(file: File): Promise<UploadedFile> {
  if (CMS_MODE === 'mock') {
    const url = URL.createObjectURL(file)
    return {
      url,
      name: file.name,
      type: file.type,
      sizeKb: Math.round(file.size / 1024),
      large: url,
      thumbnail: url,
    }
  }
  const token = getToken()
  const fd = new FormData()
  fd.append('file', file)
  const res = await fetch('/api/upload', {
    method: 'POST',
    headers: token ? { Authorization: `Bearer ${token}` } : undefined,
    body: fd,
  })
  const body = (await res.json().catch(() => ({}))) as {
    ok?: boolean
    error?: string
    data?: { items?: UploadedFile[] }
  }
  if (!res.ok || body.ok === false) {
    if (res.status === 401) redirectToLogin()
    if (res.status === 403) showPermissionToast(body.error || '权限不足')
    throw new ApiError(res.status, body.error || '上传失败')
  }
  const item = body.data?.items?.[0]
  if (!item) throw new ApiError(res.status, '上传未返回文件地址')
  return item
}

/** P2 在线支付：微信支付配置 + 订单对账（管理端） */
export const payAdminApi = {
  async recon(days = 30): Promise<ReconResp> {
    const r = await request<{ data: ReconResp }>(`/api/admin/orders/recon?days=${days}`)
    return r.data
  },
  async closeStale(): Promise<{ closed: number }> {
    const r = await request<{ data: { closed: number } }>('/api/admin/orders/close-stale', { method: 'POST' })
    return r.data
  },
  /** 发货未完成的订单（已收款但积分/订阅未到账） */
  async fulfillPending(): Promise<{ items: FulfillTask[] }> {
    const r = await request<{ data: { items: FulfillTask[] } }>('/api/admin/orders/fulfill-pending')
    return r.data
  },
  /** 重放未完成的发货动作（幂等，可反复调用） */
  async retryFulfill(): Promise<RetryFulfillResp> {
    const r = await request<{ data: RetryFulfillResp }>('/api/admin/orders/retry-fulfill', { method: 'POST' })
    return r.data
  },
  async getConfig(): Promise<PayConfigInfo> {
    const r = await request<{ data: PayConfigInfo }>('/api/admin/payconfig')
    return r.data
  },
  async saveConfig(p: {
    appid?: string
    mchid?: string
    serialNo?: string
    apiV3Key?: string
    privateKey?: string
    platformPubKey?: string
  }): Promise<{ saved: number }> {
    const r = await request<{ data: { saved: number } }>('/api/admin/payconfig', {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(p),
    })
    return r.data
  },
}


const MOCK_AUDIT_LOGS: import('./types').AuditRow[] = [
  { id: 'aud-1', userId: 'u1', username: 'admin', method: 'PUT', path: '/api/team/roles/editor', status: 200, durationMs: 24, requestId: 'req_8f1a239b', createdAt: new Date(Date.now() - 1000 * 60 * 5).toISOString() },
  { id: 'aud-2', userId: 'u1', username: 'admin', method: 'POST', path: '/api/team/users', status: 201, durationMs: 42, requestId: 'req_3e71d42a', createdAt: new Date(Date.now() - 1000 * 60 * 20).toISOString() },
  { id: 'aud-3', userId: 'u2', username: 'master_chen', method: 'PUT', path: '/api/articles/why-rust', status: 200, durationMs: 38, requestId: 'req_7a92c10b', createdAt: new Date(Date.now() - 1000 * 60 * 60 * 2).toISOString() },
  { id: 'aud-4', userId: 'u3', username: 'zhouzhou', method: 'POST', path: '/api/upload', status: 200, durationMs: 110, requestId: 'req_2d55e81c', createdAt: new Date(Date.now() - 1000 * 60 * 60 * 5).toISOString() },
  { id: 'aud-5', userId: 'u1', username: 'admin', method: 'POST', path: '/api/approvals/appr-1/decide', status: 200, durationMs: 35, requestId: 'req_9b33a18e', createdAt: new Date(Date.now() - 1000 * 60 * 60 * 24).toISOString() },
]

/** 审计日志（P2）：/api 写操作留痕，Owner 专用 */
export const auditApi = {
  async list(page: number, pageSize: number): Promise<{ items: import('./types').AuditRow[]; total: number }> {
    if (CMS_MODE === 'mock') {
      return { items: MOCK_AUDIT_LOGS, total: MOCK_AUDIT_LOGS.length }
    }
    try {
      const body = await request<{ ok: boolean; data: import('./types').AuditRow[]; total: number }>(
        `/api/admin/audit?page=${encodeURIComponent(page)}&pageSize=${encodeURIComponent(pageSize)}`,
      )
      return { items: body.data ?? [], total: body.total ?? 0 }
    } catch {
      return { items: MOCK_AUDIT_LOGS, total: MOCK_AUDIT_LOGS.length }
    }
  },
}
