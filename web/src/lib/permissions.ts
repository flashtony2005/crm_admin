// 前端权限镜像 —— 与后端 ODL 的授权矩阵（can <Entity>:Delete / can <Entity>:Edit / can <Entity>:Create）
// 保持一致。后端 delete_handler / list_handler 仍是唯一权威校验，这里仅做 UX 前置置灰/隐藏。
//
// 角色模型（见 definitions/admin.odl）：
//   super_admin / admin       → 全量 CRUD
//   tenant_admin              → 租户内 CRUD
//   editor / viewer / auditor → 最小权限（User:ListSelf + Menu:View:/dashboard），无写权限
//
// 因此「写操作」能力白名单 = { super_admin, admin, tenant_admin }。

export const WRITE_CAPABLE_ROLES = new Set<string>([
  'super_admin',
  'admin',
  'tenant_admin',
])

/** 当前角色是否具备删除能力 */
export function canDelete(role?: string | null): boolean {
  return !!role && WRITE_CAPABLE_ROLES.has(role)
}

/** 当前角色是否具备编辑能力 */
export function canEdit(role?: string | null): boolean {
  return canDelete(role)
}

/** 当前角色是否具备创建能力 */
export function canCreate(role?: string | null): boolean {
  return canDelete(role)
}
