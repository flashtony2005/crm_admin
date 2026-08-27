/**
 * 统一 API 客户端
 *
 * 适配新后端反射式 API（{ ok, data, total, error, events } 响应格式）。
 * 底层 fetch 封装，提供泛型类型推断。
 *
 * 设计原则（Phase 1.3）：所有请求最终都经过 request() / requestRaw()，
 * 统一处理：注入 Bearer token + JSON 头、401→跳转登录、403→权限 toast、
 * body.ok===false→抛出 ApiError。上层只调用语义化封装（api / apiList /
 * apiInvoke / request / requestRaw）。
 */

import { businessMessage } from '../lib/business'

const TOKEN_KEY = 'auth_token'

export function getToken(): string | null {
  return localStorage.getItem(TOKEN_KEY)
}

export function setToken(token: string) {
  localStorage.setItem(TOKEN_KEY, token)
}

export function clearToken() {
  localStorage.removeItem(TOKEN_KEY)
}

/** Token 过期/401 → 清理 token 并跳转登录页 */
export function redirectToLogin() {
  clearToken()
  // 覆写 Zustand persist 为未登录状态（同步写入，不依赖 debounce）
  const cleanState = JSON.stringify({
    state: { token: null, user: null, isAuthenticated: false },
    version: 0,
  })
  try { localStorage.setItem('auth-store', cleanState) } catch {}
  try { sessionStorage.removeItem('auth-store') } catch {}
  // 使用 window.location 确保全页面跳转，清除所有前端状态
  if (window.location.pathname !== '/login') {
    window.location.href = '/login'
  }
}

export class ApiError extends Error {
  code: number
  constructor(code: number, message: string) {
    super(businessMessage(message))
    this.code = code
    this.name = 'ApiError'
  }
}

let toastFunction: ((message: string) => void) | null = null
export function setToastFunction(fn: (message: string) => void) {
  toastFunction = fn
}

export function showPermissionToast(message: string) {
  if (toastFunction) {
    toastFunction(businessMessage(message))
  } else {
    console.warn('权限不足:', message)
  }
}

// ── 底层请求核心 ──────────────────────────────────────────

function buildHeaders(init?: RequestInit): Record<string, string> {
  const token = getToken()
  return {
    'Content-Type': 'application/json',
    ...(token ? { Authorization: `Bearer ${token}` } : {}),
    ...(init?.headers as Record<string, string> | undefined),
  }
}

interface RawBody {
  ok?: boolean
  error?: string
  data?: unknown
  events?: unknown[]
  total?: number
  page?: number
  pageSize?: number
}

/**
 * 统一请求核心 —— 所有 API 调用的唯一出口。
 * - 自动注入 token + JSON 头
 * - 401 → redirectToLogin；403 → 权限 toast
 * - body.ok === false → 抛出 ApiError（含状态码与后端 error 信息）
 * 返回完整响应体，调用方按需取 data / events 等字段。
 */
export async function request<T = unknown>(url: string, init?: RequestInit): Promise<T> {
  const res = await fetch(url, {
    ...init,
    headers: buildHeaders(init),
  })
  const body: RawBody = await res
    .json()
    .catch(() => ({ ok: false, error: `请求失败 (${res.status})` }))

  if (body.ok === false) {
    if (res.status === 401) redirectToLogin()
    if (res.status === 403) showPermissionToast(body.error || '权限不足')
    throw new ApiError(res.status, body.error || '请求失败')
  }
  return body as T
}

/**
 * 与 request() 行为一致，但 body.ok === false 时不抛错（仍处理 401 跳转）。
 * 用于以 ok 布尔值表达成败的端点（更新/删除）或本身无 ok 字段的端点。
 */
export async function requestRaw<T = unknown>(url: string, init?: RequestInit): Promise<T> {
  const res = await fetch(url, {
    ...init,
    headers: buildHeaders(init),
  })
  const body: RawBody = await res
    .json()
    .catch(() => ({ ok: false, error: `请求失败 (${res.status})` }))
  if (res.status === 401) redirectToLogin()
  return body as T
}

// ── 语义化封装 ────────────────────────────────────────────

/** 标准 { ok, data } 端点 → 返回 data 部分 */
export async function api<T = unknown>(url: string, init?: RequestInit): Promise<T> {
  const body = await request<{ ok: boolean; data: T }>(url, init)
  return body.data as T
}

/** 列表 { ok, data, total, page, pageSize } 端点 → 展开为对象 */
export async function apiList<T = unknown>(
  url: string,
  init?: RequestInit,
): Promise<{ data: T[]; total: number; page: number; pageSize: number }> {
  const body = await request<{
    ok: boolean
    data: T[]
    total?: number
    page?: number
    pageSize?: number
  }>(url, init)
  return {
    data: (body.data as T[]) || [],
    total: (body.total as number) ?? 0,
    page: (body.page as number) ?? 1,
    pageSize: (body.pageSize as number) ?? 0,
  }
}

/** invoke 类端点 → 返回 { ok, events } */
export async function apiInvoke(
  url: string,
  init?: RequestInit,
): Promise<{ ok: boolean; events: unknown[]; error?: string }> {
  const body = await request<{ ok: boolean; events?: unknown[] }>(url, init)
  return { ok: true, events: body.events || [] }
}
