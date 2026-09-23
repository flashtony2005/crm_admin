/**
 * HTTP 版 CrudService —— 真实后端适配器（demo/server）。
 *
 * 与 store.ts 的 localStorage 适配器实现**同一接口**：
 * 页面 / useCmsCollection 零改动切换（VITE_CMS_MODE=real 启用）。
 * 复用 client.ts 的 request()：自动注入 Bearer、401 跳登录、403 权限 toast。
 */
import { api, apiList, request } from '../client'
import type { CrudService } from './store'

export function httpCollection<T extends { id: string }>(resource: string): CrudService<T> {
  const base = `/api/${resource}`

  return {
    async list(): Promise<T[]> {
      const body = await apiList<T>(base)
      return body.data ?? []
    },

    async listPaged(
      page: number,
      pageSize: number,
      filters?: Record<string, string>,
    ): Promise<{ items: T[]; total: number }> {
      const qs = new URLSearchParams()
      qs.set('page', String(page))
      qs.set('pageSize', String(pageSize))
      // 后端 /api/{table} 支持白名单列等值过滤（?col=value）；
      // 空值不发 —— 传空串会被当成「筛选该列为空」，语义完全不同。
      for (const [k, v] of Object.entries(filters ?? {})) {
        if (v !== '') qs.set(k, v)
      }
      const body = await apiList<T>(`${base}?${qs.toString()}`)
      return { items: body.data ?? [], total: body.total ?? 0 }
    },

    async get(id: string): Promise<T | undefined> {
      try {
        return await api<T>(`${base}/${id}`)
      } catch (e) {
        if (e instanceof Error && 'code' in e && (e as { code: number }).code === 404) {
          return undefined
        }
        throw e
      }
    },

    async create(input): Promise<T> {
      return api<T>(base, { method: 'POST', body: JSON.stringify(input) })
    },

    async update(id: string, patch): Promise<T> {
      return api<T>(`${base}/${id}`, { method: 'PUT', body: JSON.stringify(patch) })
    },

    async remove(id: string): Promise<void> {
      await request<{ ok: boolean; data?: unknown }>(`${base}/${id}`, { method: 'DELETE' })
    },
  }
}

/** 登录后拉取权限集写入 permission store（真实模式专用） */
export async function fetchAndApplyPermissions(): Promise<void> {
  try {
    const me = await api<{ permissions?: string[]; role?: string }>('/api/user/me')
    const { usePermissionStore } = await import('../../store/permission')
    usePermissionStore.getState().setGranted((me.permissions ?? []) as never)
    if (me.role) {
      const role = me.role as 'owner' | 'editor' | 'viewer'
      usePermissionStore.getState().setRole(role)
    }
  } catch {
    // 拉取失败（如 mock 模式误调）：保持本地矩阵，不阻塞登录
  }
}
