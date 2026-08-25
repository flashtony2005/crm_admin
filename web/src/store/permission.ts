import { create } from 'zustand'
import { persist } from 'zustand/middleware'

import { ROLE_PERMS, permMatches, type PermString, type RoleKey } from '../config/permissions'

/**
 * 权限状态（Phase 1 mock 源）。
 *
 * - `role` 当前仅是本地演示态（默认 owner，可切换预览权限效果）；
 * - 后端就绪后的唯一改动：登录后由 /api/auth/me 下发 `{ roles, permissions }`，
 *   本 store 改存服务端权限集合（granted），ROLE_PERMS 矩阵退役为测试夹具；
 * - 判定入口始终是 has()，调用方无感知切换。
 */
interface PermissionState {
  role: RoleKey
  /** 服务端下发的权限集（Phase 1 为空 = 使用本地矩阵推导） */
  granted: PermString[] | null
  setRole: (role: RoleKey) => void
  setGranted: (perms: PermString[] | null) => void
}

export const usePermissionStore = create<PermissionState>()(
  persist(
    (set) => ({
      role: 'owner',
      granted: null,
      setRole: (role) => set({ role }),
      setGranted: (granted) => set({ granted }),
    }),
    { name: 'perm-store' },
  ),
)

/** 当前生效的权限集合（服务端优先，本地矩阵兜底） */
export function effectivePerms(state: Pick<PermissionState, 'role' | 'granted'>): PermString[] {
  if (state.granted && state.granted.length > 0) return state.granted
  return ROLE_PERMS[state.role] ?? []
}

/** 是否拥有某权限码（支持通配尾段与 '*'） */
export function hasPerm(state: Pick<PermissionState, 'role' | 'granted'>, need: string): boolean {
  return permMatches(effectivePerms(state), need)
}
