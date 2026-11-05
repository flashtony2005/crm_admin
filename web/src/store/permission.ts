import { create } from 'zustand'
import { persist } from 'zustand/middleware'
import {
  ROLE_PERMS,
  DEFAULT_ROLES,
  permMatches,
  type PermString,
  type RoleDef,
} from '../config/permissions'

interface PermissionState {
  role: string
  roles: RoleDef[]
  rolePerms: Record<string, PermString[]>
  /** 服务端下发的权限集（为空时使用本地自定义或预设矩阵） */
  granted: PermString[] | null

  setRole: (role: string) => void
  setGranted: (perms: PermString[] | null) => void

  /** 更新角色元信息（名称、说明、AI 边界） */
  updateRole: (key: string, patch: Partial<RoleDef>) => void

  /** 新增自定义角色 */
  addRole: (role: RoleDef, initialPerms?: PermString[]) => void

  /** 删除自定义角色 */
  deleteRole: (key: string) => void

  /** 为指定角色保存授权权限集 */
  updateRolePerms: (roleKey: string, perms: PermString[]) => void

  /** 快速勾选/取消某项权限 */
  togglePermForRole: (roleKey: string, perm: PermString) => void

  /** 重置为系统初始预设 */
  resetToDefault: () => void
}

export const usePermissionStore = create<PermissionState>()(
  persist(
    (set, get) => ({
      role: 'owner',
      roles: DEFAULT_ROLES,
      rolePerms: ROLE_PERMS,
      granted: null,

      setRole: (role) => set({ role }),
      setGranted: (granted) => set({ granted }),

      updateRole: (key, patch) => {
        set((state) => ({
          roles: state.roles.map((r) => (r.key === key ? { ...r, ...patch } : r)),
        }))
      },

      addRole: (newRole, initialPerms = []) => {
        set((state) => ({
          roles: [...state.roles, newRole],
          rolePerms: {
            ...state.rolePerms,
            [newRole.key]: initialPerms,
          },
        }))
      },

      deleteRole: (key) => {
        const { role } = get()
        set((state) => {
          const nextPerms = { ...state.rolePerms }
          delete nextPerms[key]
          return {
            roles: state.roles.filter((r) => r.key !== key),
            rolePerms: nextPerms,
            role: role === key ? 'owner' : role,
          }
        })
      },

      updateRolePerms: (roleKey, perms) => {
        set((state) => ({
          rolePerms: {
            ...state.rolePerms,
            [roleKey]: perms,
          },
        }))
      },

      togglePermForRole: (roleKey, perm) => {
        const state = get()
        const current = state.rolePerms[roleKey] || ROLE_PERMS[roleKey] || []
        const has = current.includes(perm)
        const next = has ? current.filter((p) => p !== perm) : [...current, perm]
        set({
          rolePerms: {
            ...state.rolePerms,
            [roleKey]: next,
          },
        })
      },

      resetToDefault: () => {
        set({
          roles: DEFAULT_ROLES,
          rolePerms: ROLE_PERMS,
          role: 'owner',
          granted: null,
        })
      },
    }),
    {
      name: 'perm-store-v2',
    },
  ),
)

/** 当前生效的权限集合（服务端优先，本地可配矩阵兜底） */
export function effectivePerms(state: Pick<PermissionState, 'role' | 'granted' | 'rolePerms'>): PermString[] {
  if (state.granted && state.granted.length > 0) return state.granted
  if (state.rolePerms && state.rolePerms[state.role]) {
    return state.rolePerms[state.role]
  }
  return ROLE_PERMS[state.role] ?? []
}

/** 是否拥有某权限码（支持通配尾段与 '*'） */
export function hasPerm(state: Pick<PermissionState, 'role' | 'granted' | 'rolePerms'>, need: string): boolean {
  return permMatches(effectivePerms(state), need)
}
