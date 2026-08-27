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
  (import.meta.env.VITE_CMS_MODE as 'mock' | 'real' | undefined) ?? 'real'
import {
  seedAiTasks, seedApprovals, seedArticles, seedCustomers, seedForms,
  seedIntegrations, seedLeads, seedMedia, seedPages, seedProducts, seedWorkflows,
} from './seed'
import type {
  AiTask, Approval, Article, Comment, Customer, FormDef,
  Integration, Lead, LocaleMessages, MediaItem, Member,
  MemberProfile, Page, Product, Subscriber, Tag, Tier,
  WebhookSubscription, WorkflowDef,
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
  : collection<Member>('members', [])
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
  : collection<WebhookSubscription>('webhooks', [])

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
  async register(email: string, name: string, password: string): Promise<MemberProfile> {
    const r = await request<{ data: { token: string; member: MemberProfile } }>(
      '/api/public/members/register',
      { method: 'POST', body: JSON.stringify({ email, name, password }) },
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

/** 出站 Webhook 测试触发 */
export async function triggerWebhookTest(event = 'ping'): Promise<void> {
  await request<{ ok: boolean }>('/api/webhooks/test', {
    method: 'POST',
    body: JSON.stringify({ event }),
  })
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
