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
