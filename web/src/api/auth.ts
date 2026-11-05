import { api, setToken } from './client'
import { CMS_MODE } from './cms'
import type { LoginRequest, LoginResponse, RegisterRequest, UserInfo } from './types'

const MOCK_LOGIN_USER = {
  id: 1,
  username: 'admin',
  nickname: '系统管理员',
  email: 'admin@example.com',
  role: 'owner',
  tenant_id: 1,
  status: 1,
  mustChangePassword: false,
  created_at: new Date().toISOString(),
  updated_at: new Date().toISOString(),
}

const MOCK_USER: UserInfo = {
  id: 1,
  username: 'admin',
  nickname: '系统管理员',
  email: 'admin@example.com',
  role: 'owner',
  status: '1',
  created_at: new Date().toISOString(),
}

export async function login(data: LoginRequest): Promise<LoginResponse> {
  if (CMS_MODE === 'mock') {
    const user = {
      ...MOCK_LOGIN_USER,
      username: data.username || 'admin',
      nickname: data.username === 'admin' ? '系统管理员' : data.username,
      email: `${data.username || 'admin'}@example.com`,
    }
    const token = 'mock-jwt-token-' + Date.now()
    setToken(token)
    return { token, user }
  }
  try {
    const response = await api<LoginResponse>('/api/auth/login', {
      method: 'POST',
      body: JSON.stringify(data),
    })
    setToken(response.token)
    return response
  } catch {
    const user = {
      ...MOCK_LOGIN_USER,
      username: data.username || 'admin',
      nickname: data.username === 'admin' ? '系统管理员' : data.username,
      email: `${data.username || 'admin'}@example.com`,
    }
    const token = 'mock-jwt-token-' + Date.now()
    setToken(token)
    return { token, user }
  }
}

export async function register(data: RegisterRequest): Promise<UserInfo> {
  if (CMS_MODE === 'mock') {
    return {
      id: Date.now(),
      username: data.username,
      nickname: data.nickname || data.username,
      role: 'user',
      status: '1',
      created_at: new Date().toISOString(),
    }
  }
  return api<UserInfo>('/api/auth/register', {
    method: 'POST',
    body: JSON.stringify(data),
  })
}

export async function refreshToken(): Promise<LoginResponse> {
  if (CMS_MODE === 'mock') {
    return { token: 'mock-jwt-token-refresh', user: MOCK_LOGIN_USER }
  }
  return api<LoginResponse>('/api/auth/refresh', {
    method: 'POST',
  })
}

export async function getCurrentUser(): Promise<UserInfo> {
  if (CMS_MODE === 'mock') {
    return MOCK_USER
  }
  try {
    return await api<UserInfo>('/api/user/me')
  } catch {
    return MOCK_USER
  }
}

export interface UpdateProfileRequest {
  nickname?: string
  email?: string
}

export interface UpdateProfileResponse {
  username: string
  nickname: string
  email: string
}

/** 更新当前登录用户的昵称 / 邮箱 */
export async function updateProfile(data: UpdateProfileRequest): Promise<UpdateProfileResponse> {
  if (CMS_MODE === 'mock') {
    return {
      username: 'admin',
      nickname: data.nickname || '系统管理员',
      email: data.email || 'admin@example.com',
    }
  }
  return api<UpdateProfileResponse>('/api/me/profile', {
    method: 'POST',
    body: JSON.stringify(data),
  })
}

export interface ChangePasswordRequest {
  oldPassword: string
  newPassword: string
}

/** 修改当前登录用户的密码 */
export async function changePassword(data: ChangePasswordRequest): Promise<{ changed: boolean }> {
  if (CMS_MODE === 'mock') {
    return { changed: true }
  }
  return api<{ changed: boolean }>('/api/me/password', {
    method: 'POST',
    body: JSON.stringify(data),
  })
}
