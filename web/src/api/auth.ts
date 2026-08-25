import { api, setToken } from './client'
import type { LoginRequest, LoginResponse, RegisterRequest, UserInfo } from './types'

export async function login(data: LoginRequest): Promise<LoginResponse> {
  const response = await api<LoginResponse>('/api/auth/login', {
    method: 'POST',
    body: JSON.stringify(data),
  })
  setToken(response.token)
  return response
}

export async function register(data: RegisterRequest): Promise<UserInfo> {
  return api<UserInfo>('/api/auth/register', {
    method: 'POST',
    body: JSON.stringify(data),
  })
}

export async function refreshToken(): Promise<LoginResponse> {
  return api<LoginResponse>('/api/auth/refresh', {
    method: 'POST',
  })
}

export async function getCurrentUser(): Promise<UserInfo> {
  return api<UserInfo>('/api/user/me')
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
  return api<UpdateProfileResponse>('/api/me/profile', {
    method: 'POST',
    body: JSON.stringify(data),
  })
}

export interface ChangePasswordRequest {
  old_password: string
  new_password: string
}

/** 修改当前登录用户的密码 */
export async function changePassword(data: ChangePasswordRequest): Promise<{ changed: boolean }> {
  return api<{ changed: boolean }>('/api/me/password', {
    method: 'POST',
    body: JSON.stringify(data),
  })
}
