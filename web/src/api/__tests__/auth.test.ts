import { describe, it, expect, beforeEach, vi } from 'vitest'

// auth.ts 仅依赖 client 的 api + setToken；mock 掉即可测 URL/body 与副作用。
const { apiMock, setTokenMock } = vi.hoisted(() => ({
  apiMock: vi.fn(),
  setTokenMock: vi.fn(),
}))

vi.mock('../client', () => ({
  api: (...args: any[]) => apiMock(...args),
  setToken: (...args: any[]) => setTokenMock(...args),
}))

import { login, register, refreshToken, getCurrentUser } from '../auth'
import type { LoginResponse } from '../types'

const okResponse: LoginResponse = {
  token: 'tok-xyz',
  user: { id: 1, username: 'admin', nickname: 'A', role: 'super_admin', status: 1 },
}

beforeEach(() => {
  vi.clearAllMocks()
})

describe('login', () => {
  it('posts to /api/auth/login and persists the returned token', async () => {
    apiMock.mockResolvedValue(okResponse)
    const res = await login({ username: 'admin', password: 'secret' })
    expect(apiMock).toHaveBeenCalledWith(
      '/api/auth/login',
      expect.objectContaining({ method: 'POST', body: JSON.stringify({ username: 'admin', password: 'secret' }) }),
    )
    expect(setTokenMock).toHaveBeenCalledWith('tok-xyz')
    expect(res.token).toBe('tok-xyz')
  })
})

describe('register', () => {
  it('posts to /api/auth/register', async () => {
    apiMock.mockResolvedValue(okResponse.user)
    await register({ username: 'bob', password: 'x', nickname: 'B' })
    expect(apiMock).toHaveBeenCalledWith(
      '/api/auth/register',
      expect.objectContaining({ method: 'POST' }),
    )
  })
})

describe('refreshToken', () => {
  it('posts to /api/auth/refresh', async () => {
    apiMock.mockResolvedValue(okResponse)
    await refreshToken()
    expect(apiMock).toHaveBeenCalledWith('/api/auth/refresh', expect.objectContaining({ method: 'POST' }))
  })
})

describe('getCurrentUser', () => {
  it('gets /api/user/me', async () => {
    apiMock.mockResolvedValue(okResponse.user)
    const user = await getCurrentUser()
    expect(apiMock).toHaveBeenCalledWith('/api/user/me')
    expect(user.username).toBe('admin')
  })
})
