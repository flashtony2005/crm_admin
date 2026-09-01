/**
 * Domain Model V1 API 客户端 —— /api/v1/*（Agent-first 领域模型）
 *
 * 与旧 CMS 网关（/api/{table}，见 api/cms/http.ts）不同：
 * - 更新用 PATCH（不是 PUT）；
 * - 响应信封 { ok, data, total }，字段 camelCase；
 * - 有 挂载 Context / 一稿多投 Channel / Site 绑定模板主题 / render 等关联端点。
 *
 * 复用 client.ts 的 api / apiList / request：自动注入 Bearer、401 跳登录、
 * 403 权限 toast、ok===false 抛 ApiError。
 */

import { api, apiList, request } from './client'

// ── 领域类型（与 server/src/entity + api 层序列化契约一致）───────────────

export interface DvBase {
  id: string
  createdAt: string
  updatedAt: string
}

/** Site：呈现载体 + 模板路由绑定 + 主题绑定 */
export interface DvSite extends DvBase {
  name: string
  slug: string
  domain?: string | null
  description?: string | null
  defaultLocale?: string | null
  timezone?: string | null
  status: string
  metadata: Record<string, unknown>
  settings: Record<string, unknown>
}

/** Theme：怎么呈现（带版本化） */
export interface DvTheme extends DvBase {
  name: string
  slug: string
  description?: string | null
  status: string
  latestVersion: number
  metadata: Record<string, unknown>
}

/** Template：怎么组合（带版本化） */
export interface DvTemplate extends DvBase {
  name: string
  slug: string
  type: string
  description?: string | null
  status: string
  latestVersion: number
  metadata: Record<string, unknown>
}

/** Context：Agent 的语义上下文（不等价于 Tag） */
export interface DvContext extends DvBase {
  name: string
  slug: string
  type: string
  description?: string | null
  parentId?: string | null
  status: string
  data: Record<string, unknown>
  metadata: Record<string, unknown>
  contentCount?: number
}

/** Channel：分发出口 */
export interface DvChannel extends DvBase {
  name: string
  type: string
  url?: string | null
  provider?: string | null
  status: string
  externalId?: string | null
  metadata: Record<string, unknown>
  contentCount?: number
}

/** Content 已挂载的 Context 引用 */
export interface DvContentContextRef {
  id: string
  name: string
  slug: string
  type: string
  role?: string
  weight?: number
}

/** Content 已分发的 Channel 引用 */
export interface DvContentChannelRef {
  id: string
  name: string
  type: string
  status: string
  publishedAt?: string | null
  externalId?: string | null
  externalUrl?: string | null
}

/** Content：业务事实（列表含嵌套 contexts / channels） */
export interface DvContent extends DvBase {
  type: string
  slug?: string | null
  title?: string | null
  summary?: string | null
  status: string
  locale: string
  data: Record<string, unknown>
  metadata: Record<string, unknown>
  authorId?: string | null
  version: number
  publishedAt?: string | null
  contexts?: DvContentContextRef[]
  channels?: DvContentChannelRef[]
}

/** Site 绑定的模板路由 */
export interface DvSiteTemplate {
  templateId: string
  route: string
  isDefault?: boolean
  template?: DvTemplate
}

/** Site 绑定的主题 */
export interface DvSiteTheme {
  themeId: string
  isDefault?: boolean
  theme?: DvTheme
}

/** 版本记录（template_version / theme_version） */
export interface DvVersion {
  version: number
  status: string
  definition?: Record<string, unknown>
  tokens?: Record<string, unknown>
  createdAt: string
}

/** Render Contract：外部 Renderer 消费的呈现契约 */
export interface DvRender {
  schema: string
  navigation: { nav: unknown[] }
  sections: {
    id: string
    component: string
    binding: Record<string, unknown>
    data: unknown
    props: Record<string, unknown>
  }[]
  theme?: { schema: string; tokens: Record<string, unknown> }
  template?: { schema: string; version: number }
  site?: { name: string; slug: string }
}

// ── 通用 CRUD 工厂（PATCH 更新）────────────────────────────────────────

function crud<T extends DvBase>(resource: string) {
  const base = `/api/v1/${resource}`
  return {
    async list(): Promise<T[]> {
      const b = await apiList<T>(base)
      return b.data ?? []
    },
    async get(id: string): Promise<T> {
      return api<T>(`${base}/${id}`)
    },
    async create(input: Record<string, unknown>): Promise<string> {
      const b = await api<{ id: string }>(base, { method: 'POST', body: JSON.stringify(input) })
      return b.id
    },
    async update(id: string, patch: Partial<T>): Promise<T> {
      return api<T>(`${base}/${id}`, { method: 'PATCH', body: JSON.stringify(patch) })
    },
    async remove(id: string): Promise<void> {
      await request<{ ok: boolean }>(`${base}/${id}`, { method: 'DELETE' })
    },
  }
}

// ── 领域 API 门面 ──────────────────────────────────────────────────────

export const domainApi = {
  sites: {
    ...crud<DvSite>('sites'),
    async templates(id: string): Promise<DvSiteTemplate[]> {
      const b = await apiList<DvSiteTemplate>(`/api/v1/sites/${id}/templates`)
      return b.data ?? []
    },
    async bindTemplate(id: string, body: { templateId: string; route: string; isDefault?: boolean }): Promise<void> {
      await request<{ ok: boolean }>(`/api/v1/sites/${id}/templates`, { method: 'POST', body: JSON.stringify(body) })
    },
    async unbindTemplate(id: string, templateId: string): Promise<void> {
      await request<{ ok: boolean }>(`/api/v1/sites/${id}/templates/${templateId}`, { method: 'DELETE' })
    },
    async themes(id: string): Promise<DvSiteTheme[]> {
      const b = await apiList<DvSiteTheme>(`/api/v1/sites/${id}/themes`)
      return b.data ?? []
    },
    async bindTheme(id: string, body: { themeId: string; isDefault?: boolean }): Promise<void> {
      await request<{ ok: boolean }>(`/api/v1/sites/${id}/themes`, { method: 'POST', body: JSON.stringify(body) })
    },
    async unbindTheme(id: string, themeId: string): Promise<void> {
      await request<{ ok: boolean }>(`/api/v1/sites/${id}/themes/${themeId}`, { method: 'DELETE' })
    },
    async render(id: string, route = '/'): Promise<DvRender> {
      return api<DvRender>(`/api/v1/sites/${id}/render?route=${encodeURIComponent(route)}`)
    },
  },
  themes: {
    ...crud<DvTheme>('themes'),
    async versions(id: string): Promise<DvVersion[]> {
      const b = await apiList<DvVersion>(`/api/v1/themes/${id}/versions`)
      return b.data ?? []
    },
    async createVersion(id: string, body: { tokens?: Record<string, unknown>; status?: string }): Promise<void> {
      await request<{ ok: boolean }>(`/api/v1/themes/${id}/versions`, { method: 'POST', body: JSON.stringify(body) })
    },
  },
  templates: {
    ...crud<DvTemplate>('templates'),
    async versions(id: string): Promise<DvVersion[]> {
      const b = await apiList<DvVersion>(`/api/v1/templates/${id}/versions`)
      return b.data ?? []
    },
    async createVersion(id: string, body: { definition?: Record<string, unknown>; status?: string }): Promise<void> {
      await request<{ ok: boolean }>(`/api/v1/templates/${id}/versions`, { method: 'POST', body: JSON.stringify(body) })
    },
  },
  contexts: {
    ...crud<DvContext>('contexts'),
  },
  channels: {
    ...crud<DvChannel>('channels'),
  },
  content: {
    ...crud<DvContent>('content'),
    /** 挂载 Context（role: primary/secondary/audience/topic/intent；weight 0-10） */
    async attachContext(id: string, body: { contextId: string; role?: string; weight?: number }): Promise<void> {
      await request<{ ok: boolean }>(`/api/v1/content/${id}/contexts`, { method: 'POST', body: JSON.stringify(body) })
    },
    async detachContext(id: string, contextId: string): Promise<void> {
      await request<{ ok: boolean }>(`/api/v1/content/${id}/contexts/${contextId}`, { method: 'DELETE' })
    },
    /** 分发到渠道（要求内容已 published） */
    async publishToChannel(
      id: string,
      channelId: string,
      body: { externalId?: string; externalUrl?: string; status?: string } = {},
    ): Promise<void> {
      await request<{ ok: boolean }>(`/api/v1/content/${id}/channels/${channelId}`, {
        method: 'POST',
        body: JSON.stringify(body),
      })
    },
    async unpublishFromChannel(id: string, channelId: string): Promise<void> {
      await request<{ ok: boolean }>(`/api/v1/content/${id}/channels/${channelId}`, { method: 'DELETE' })
    },
  },
}

// ── 领域枚举（与 server/src/entity/mod.rs 白名单一致）────────────────────

export const CONTENT_TYPES = [
  'profile', 'article', 'organization', 'project', 'product',
  'experiment', 'link', 'media', 'event', 'custom',
] as const

export const CHANNEL_TYPES = [
  'website', 'x', 'wechat', 'telegram', 'newsletter', 'youtube',
  'tiktok', 'xiaohongshu', 'api', 'custom',
] as const

export const CONTEXT_ROLES = ['primary', 'secondary', 'audience', 'topic', 'intent'] as const

export const CONTEXT_TYPES = ['topic', 'audience', 'intent', 'region', 'industry', 'custom'] as const

export const CONTENT_STATUSES = ['draft', 'published', 'archived'] as const
