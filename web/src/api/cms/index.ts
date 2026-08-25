/**
 * CMS 服务出口 —— 每个集合一个共享 CRUD 实例（api/cms/store.ts 抽象）。
 * 页面只 import 本文件，不直接接触 localStorage 适配器。
 */
import { collection } from './store'
import { fetchAndApplyPermissions, httpCollection } from './http'
import { request } from '../client'

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
  AiTask, Approval, Article, Customer, FormDef,
  Integration, Lead, MediaItem, Page, Product, WorkflowDef,
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
