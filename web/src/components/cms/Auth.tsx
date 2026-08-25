import type { ReactNode } from 'react'
import { usePermission } from '../../hooks/usePermission'

interface AuthProps {
  /** 需要的权限码（如 'content.articles.create'），也接受通配尾段 */
  perm: string
  /**
   * 无权限时的表现：
   * - hide（默认）：不渲染 —— 对应 RuoYi v-hasPermi 的 DOM 移除；
   * - disable：置灰 + tooltip 提示所需权限 —— 保留可发现性，方便管理员理解升级路径。
   */
  mode?: 'hide' | 'disable'
  /** disable 模式下 hover 提示文案；默认「需要权限：<perm>」 */
  hint?: string
  children: ReactNode
}

/**
 * 按钮级权限守卫（抽象组件）—— RuoYi v-hasPermi 的 React 等价物。
 *
 * 注意：这只是 UX 镜像层；权威校验在后端接口（403 由 api/client.ts 统一 toast）。
 * AI 的 Capability 走同一份权限码表（见 config/permissions.ts）。
 */
export function Auth({ perm, mode = 'hide', hint, children }: AuthProps) {
  const { has } = usePermission()

  if (has(perm)) return <>{children}</>
  if (mode === 'disable') {
    return (
      <span
        title={hint ?? `需要权限：${perm}`}
        aria-disabled
        className="inline-block opacity-40 cursor-not-allowed select-none [&>*]:pointer-events-none"
      >
        {children}
      </span>
    )
  }
  return null
}
