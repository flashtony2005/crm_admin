import type { ReactNode } from 'react'
import { Button } from '@heroui/react'

interface Props {
  /** 搜索占位文案 */
  searchPlaceholder?: string
  searchValue: string
  onSearchChange: (v: string) => void
  /** 左侧附加过滤器（状态下拉等） */
  filters?: ReactNode
  /** 右侧动作区 */
  children?: ReactNode
}

/**
 * CMS 工具栏（抽象组件）：搜索 + 过滤器 + 主操作，所有列表页共用。
 */
export function CmsToolbar({
  searchPlaceholder = '搜索…',
  searchValue,
  onSearchChange,
  filters,
  children,
}: Props) {
  return (
    <div className="flex items-center justify-between gap-3 mb-3 flex-wrap">
      <div className="flex items-center gap-2 flex-wrap">
        <div className="relative">
          <input
            value={searchValue}
            onChange={(e) => onSearchChange(e.target.value)}
            placeholder={searchPlaceholder}
            aria-label={searchPlaceholder}
            className="w-56 h-9 pl-8 pr-3 text-sm rounded-lg border border-os-border bg-white outline-none focus:border-[#6366f1] focus:ring-2 focus:ring-indigo-100 transition-all"
          />
          <svg
            className="absolute left-2.5 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-gray-400"
            viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"
            strokeLinecap="round" strokeLinejoin="round"
          >
            <circle cx="11" cy="11" r="8" />
            <path d="m21 21-4.3-4.3" />
          </svg>
        </div>
        {filters}
      </div>
      {children && <div className="flex items-center gap-2">{children}</div>}
    </div>
  )
}

/** 分页条（与 CmsDataTable 配套） */
export function CmsPagination({
  page, pageCount, total, onPageChange,
}: {
  page: number
  pageCount: number
  total: number
  onPageChange: (p: number) => void
}) {
  if (pageCount <= 1) {
    return <p className="text-xs text-os-text-muted mt-2">共 {total} 条</p>
  }
  return (
    <div className="flex items-center gap-3 mt-3">
      <span className="text-xs text-os-text-muted">
        共 {total} 条 · 第 {page} / {pageCount} 页
      </span>
      <div className="flex gap-1.5 ml-auto">
        <Button size="sm" variant="ghost" isDisabled={page <= 1} onPress={() => onPageChange(page - 1)}>
          上一页
        </Button>
        <Button size="sm" variant="ghost" isDisabled={page >= pageCount} onPress={() => onPageChange(page + 1)}>
          下一页
        </Button>
      </div>
    </div>
  )
}
