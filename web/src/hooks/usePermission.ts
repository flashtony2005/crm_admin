import { usePermissionStore, hasPerm } from '../store/permission'
import type { PermString } from '../config/permissions'

/**
 * 权限 hook：组件内判断当前用户是否拥有某权限码。
 *
 * 用法（等价于 RuoYi 的 v-hasPermi / checkPermi，但走 React 惯用法）：
 *   const { has } = usePermission()
 *   {has('content.articles.create') && <Button>新建</Button>}
 */
export function usePermission() {
  const state = usePermissionStore()

  /** 单个权限码判断 */
  const has = (perm: string): boolean => hasPerm(state, perm)

  /** 任一命中即可（对应 RuoYi logical = OR） */
  const hasAny = (perms: string[]): boolean => perms.some((p) => has(p))

  /** 全部命中才可（对应 RuoYi logical = AND） */
  const hasAll = (perms: string[]): boolean => perms.every((p) => has(p))

  return { role: state.role, setRole: state.setRole, has, hasAny, hasAll }
}

export type { PermString }
