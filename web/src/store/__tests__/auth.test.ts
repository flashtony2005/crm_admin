import { describe, it, expect, beforeEach, vi } from 'vitest'

// Mock the API + client modules the auth store depends on, so we test pure state logic.
const { loginApi, clearToken, getToken } = vi.hoisted(() => ({
  loginApi: vi.fn(),
  clearToken: vi.fn(),
  getToken: vi.fn(() => null),
}))

vi.mock('../../api/auth', () => ({ login: (...args: any[]) => loginApi(...args) }))
vi.mock('../../api/client', () => ({
  clearToken: (...args: any[]) => clearToken(...args),
  getToken: (...args: any[]) => getToken(...args),
}))

import { useAuthStore, type UserInfo } from '../auth'

function fakeUser(over: Partial<UserInfo> = {}): UserInfo {
  return {
    id: 1,
    username: 'admin',
    nickname: 'A',
    role: 'super_admin',
    tenant_id: 1,
    status: 1,
    created_at: '',
    updated_at: '',
    ...over,
  }
}

let replaceMock: ReturnType<typeof vi.fn>

beforeEach(() => {
  localStorage.clear()
  vi.clearAllMocks()
  getToken.mockReturnValue(null)
  replaceMock = vi.fn()
  Object.defineProperty(window, 'location', {
    writable: true,
    value: { replace: replaceMock, href: '' },
  })
  useAuthStore.setState({
    token: null,
    user: null,
    isAuthenticated: false,
    isLoading: false,
    error: null,
  })
})

describe('clearError', () => {
  it('clears the error field', () => {
    useAuthStore.setState({ error: 'oops' })
    useAuthStore.getState().clearError()
    expect(useAuthStore.getState().error).toBeNull()
  })
})

describe('logout', () => {
  it('clears token, user and auth flag and persists a clean state', () => {
    useAuthStore.setState({ token: 'tok', user: fakeUser(), isAuthenticated: true })
    useAuthStore.getState().logout()
    const s = useAuthStore.getState()
    expect(s.token).toBeNull()
    expect(s.user).toBeNull()
    expect(s.isAuthenticated).toBe(false)
    expect(clearToken).toHaveBeenCalled()
    const persisted = JSON.parse(localStorage.getItem('auth-store')!)
    expect(persisted.state.isAuthenticated).toBe(false)
    expect(replaceMock).toHaveBeenCalledWith('/login')
  })
})

describe('restoreAuth', () => {
  it('clears auth state when there is no token', () => {
    useAuthStore.setState({ token: 'stale', user: fakeUser(), isAuthenticated: true })
    useAuthStore.getState().restoreAuth()
    const s = useAuthStore.getState()
    expect(s.token).toBeNull()
    expect(s.user).toBeNull()
    expect(s.isAuthenticated).toBe(false)
  })

  it('treats a present token (without user) as authenticated', () => {
    getToken.mockReturnValue('tok-123')
    useAuthStore.getState().restoreAuth()
    const s = useAuthStore.getState()
    expect(s.token).toBe('tok-123')
    expect(s.isAuthenticated).toBe(true)
  })
})

describe('login', () => {
  it('stores token + user and marks authenticated on success', async () => {
    loginApi.mockResolvedValue({ token: 'tok', user: fakeUser() })
    const info = await useAuthStore.getState().login('admin', 'secret')
    const s = useAuthStore.getState()
    expect(info.username).toBe('admin')
    expect(s.token).toBe('tok')
    expect(s.isAuthenticated).toBe(true)
    expect(s.isLoading).toBe(false)
    expect(s.error).toBeNull()
    // 同步用户缓存写入 localStorage
    expect(JSON.parse(localStorage.getItem('user:cache')!).username).toBe('admin')
  })

  it('applies safe defaults when user fields are missing', async () => {
    loginApi.mockResolvedValue({ token: 'tok', user: { id: 7 } })
    const info = await useAuthStore.getState().login('bob', 'x')
    expect(info.username).toBe('bob') // 回退到入参 username
    expect(info.role).toBe('user')
    expect(info.status).toBe(1)
  })

  it('records the error and rethrows on failure', async () => {
    loginApi.mockRejectedValue(new Error('boom'))
    await expect(useAuthStore.getState().login('admin', 'x')).rejects.toThrow('boom')
    const s = useAuthStore.getState()
    expect(s.isLoading).toBe(false)
    expect(s.error).toBe('boom')
    expect(s.isAuthenticated).toBe(false)
  })
})
