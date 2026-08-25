import { create } from 'zustand'
import { persist } from 'zustand/middleware'
import { login as apiLogin } from '../api/auth'
import { clearToken, getToken } from '../api/client'
import { CMS_MODE, fetchAndApplyPermissions } from '../api/cms'

import { extractError } from '../lib/error'

export interface UserInfo {
  id: number
  username: string
  nickname: string
  email?: string
  role: string
  tenant_id?: number
  status: number
  /** 首登/被重置后必须改密（服务端 users.must_change_password） */
  mustChangePassword?: boolean
  created_at: string
  updated_at: string
}

interface AuthState {
  token: string | null
  user: UserInfo | null
  isAuthenticated: boolean
  isLoading: boolean
  error: string | null

  login: (username: string, password: string) => Promise<UserInfo>
  logout: () => void
  clearError: () => void
  restoreAuth: () => Promise<void>
}

export const useAuthStore = create<AuthState>()(
  persist(
    (set, get) => ({
      token: null,
      user: null,
      isAuthenticated: false,
      isLoading: false,
      error: null,

      login: async (username: string, password: string) => {
        set({ isLoading: true, error: null })
        try {
          const res = await apiLogin({ username, password })
          const userInfo: UserInfo = {
            id: res.user.id ?? 0,
            username: res.user.username ?? username,
            nickname: res.user.nickname ?? username,
            email: res.user.email ?? '',
            role: res.user.role ?? 'user',
            tenant_id: res.user.tenant_id ?? 0,
            status: res.user.status ?? 1,
            mustChangePassword: res.user.mustChangePassword ?? false,
            created_at: res.user.created_at ?? '',
            updated_at: res.user.updated_at ?? '',
          }
          set({
            token: res.token,
            user: userInfo,
            isAuthenticated: true,
            isLoading: false,
            error: null,
          })
          // 登录成功后同步用户缓存到服务端
          syncUserCache(userInfo)
          // 真实模式：拉取服务端权限集（owner/editor/viewer 矩阵的权威源）
          if (CMS_MODE === 'real') {
            void fetchAndApplyPermissions()
          }
          return userInfo
        } catch (err: unknown) {
          const msg = extractError(err, '登录失败')
          set({ isLoading: false, error: msg })
          throw err
        }
      },

      logout: () => {
        clearToken()
        // 直接覆写 Zustand persist 同步存储，确保 reload 后 isAuthenticated=false
        // 不比依赖 set() 触发的 debounce 异步写入
        const cleanState = JSON.stringify({
          state: { token: null, user: null, isAuthenticated: false },
          version: 0,
        })
        try { localStorage.setItem('auth-store', cleanState) } catch {}
        try { sessionStorage.removeItem('auth-store') } catch {}
        set({
          token: null,
          user: null,
          isAuthenticated: false,
          error: null,
        })
        // replace 而非 href，避免浏览历史中被拦截
        window.location.replace('/login')
      },

      clearError: () => set({ error: null }),

      restoreAuth: async () => {
        const token = getToken()
        if (!token) {
          set({ isAuthenticated: false, token: null, user: null })
          return
        }
        // 如果有 token 但 user 为 null，标记为已认证（用户信息通过 /api/user/me 懒加载）
        const { user } = get()
        if (!user) {
          set({ isAuthenticated: true, token })
          // 尝试从服务端缓存恢复用户信息
          loadUserCache(token)
        }
      },
    }),
    {
      name: 'auth-store',
      partialize: (state) => ({
        token: state.token,
        user: state.user,
        isAuthenticated: state.isAuthenticated,
      }),
    },
  ),
)

// ─── 服务端缓存同步 ──────────────────────────────────────────

async function syncUserCache(user: UserInfo) {
  // 取消服务端缓存写入（endpoint 已移除），改用 localStorage
  try {
    localStorage.setItem('user:cache', JSON.stringify(user))
  } catch {
    // 静默失败
  }
}

async function loadUserCache(_token: string) {
  try {
    // 先从 localStorage 的 auth-store 中尝试读取 user
    const raw = localStorage.getItem('auth-store')
    if (raw) {
      const parsed = JSON.parse(raw)
      if (parsed?.state?.user) {
        useAuthStore.setState({ user: parsed.state.user })
        // 也尝试从服务端缓存恢复（覆盖本地）
      }
    }
  } catch {
    // 静默失败
  }
}
