import { type ReactNode } from 'react'
import type { FormValue } from '../../api/types'

/** 状态徽章色调，对应 DESIGN_SYSTEM.md 中的状态色表 */
export type StatusTone =
  | 'success'
  | 'warning'
  | 'info'
  | 'danger'
  | 'purple'
  | 'neutral'

interface StatusBadgeProps {
  tone?: StatusTone
  children: ReactNode
  className?: string
  /** 在文字前显示一个小圆点（增强状态可辨识度，颜色与文字同色） */
  dot?: boolean
}

const TONE_CLASS: Record<StatusTone, string> = {
  success: 'bg-os-success-bg text-os-success-text',
  warning: 'bg-os-warning-bg text-os-warning-text',
  info: 'bg-os-info-bg text-os-info-text',
  danger: 'bg-os-danger-bg text-os-danger-text',
  purple: 'bg-os-purple-bg text-os-purple-text',
  neutral: 'bg-os-neutral-bg text-os-neutral-text',
}

const TONE_DOT: Record<StatusTone, string> = {
  success: 'bg-os-success-text',
  warning: 'bg-os-warning-text',
  info: 'bg-os-info-text',
  danger: 'bg-os-danger-text',
  purple: 'bg-os-purple-text',
  neutral: 'bg-os-neutral-text',
}

/** 把业务状态字符串映射到统一的色调（大小写不敏感） */
export function toneFromStatus(status?: FormValue): StatusTone {
  const s = String(status ?? '').toLowerCase()
  if (['active', 'paid', 'success', 'done', 'completed', 'online'].includes(s)) return 'success'
  if (['inactive', 'refunded', 'error', 'failed', 'offline', 'deleted'].includes(s)) return 'danger'
  if (['processing', 'running', 'in_progress', 'progress'].includes(s)) return 'info'
  if (['pending', 'review', 'await', 'wait', 'to_confirm'].includes(s)) return 'purple'
  if (['unfulfilled', 'warning', 'warn', 'hold', 'suspended'].includes(s)) return 'warning'
  return 'neutral'
}

/**
 * 统一状态徽章组件（设计系统 §7.3）。
 * 浅底深字、圆角胶囊、低饱和，符合 Confidency OS 风格。
 */
export function StatusBadge({ tone = 'neutral', children, className = '', dot = false }: StatusBadgeProps) {
  return (
    <span
      className={`inline-flex items-center gap-1.5 rounded-full px-2.5 py-0.5 text-xs font-medium whitespace-nowrap ${TONE_CLASS[tone]} ${className}`}
    >
      {dot && <span className={`w-1.5 h-1.5 rounded-full ${TONE_DOT[tone]}`} />}
      {children}
    </span>
  )
}
