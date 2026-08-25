import type { ReactNode } from 'react'
import { Button, Spinner, Table } from '@heroui/react'

/** 列定义：header 为表头文案，render 输出单元格内容 */
export interface CmsColumn<T> {
  id: string
  header: ReactNode
  render: (row: T) => ReactNode
  /** 单元格附加类名 */
  cellClassName?: string
  /** 是否作为表格行头（HeroUI v3 要求至少一列设为 true，默认第一列） */
  isRowHeader?: boolean
}

interface Props<T> {
  columns: CmsColumn<T>[]
  rows: T[]
  rowKey: (row: T) => string
  isLoading?: boolean
  onRetry?: () => void
  emptyIcon?: ReactNode
  emptyTitle?: ReactNode
  emptyHint?: ReactNode
  /** 每行末尾的操作区（编辑/删除等） */
  actions?: (row: T) => ReactNode
}

const TABLE_CLS =
  'w-full [&_td]:whitespace-nowrap [&_td]:px-4 [&_td]:py-3 [&_th]:px-4 [&_th]:py-3 [&_th]:text-xs [&_th]:uppercase [&_th]:tracking-wider [&_th]:font-medium [&_th]:text-os-text-secondary'

/**
 * CMS 通用数据表格（抽象组件，供所有集合页面复用）。
 * 统一处理：加载 / 出错 / 空态 / 行操作区；样式对齐 JobTable。
 */
export function CmsDataTable<T extends object>({
  columns,
  rows,
  rowKey,
  isLoading,
  onRetry,
  emptyIcon = '📭',
  emptyTitle = '暂无数据',
  emptyHint = '点击右上角「新建」创建第一条记录',
  actions,
}: Props<T>) {
  if (isLoading) {
    return (
      <div className="flex justify-center items-center h-64">
        <Spinner size="lg" />
      </div>
    )
  }

  if (rows.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center h-64 text-os-text-muted">
        <div className="text-5xl mb-3 opacity-60">{emptyIcon}</div>
        <p>{emptyTitle}</p>
        <p className="text-sm mt-1">{emptyHint}</p>
        {onRetry && (
          <Button variant="ghost" size="sm" className="mt-3" onPress={onRetry}>
            重新加载
          </Button>
        )}
      </div>
    )
  }

  return (
    <div className="overflow-x-auto rounded-xl border border-os-border shadow-sm bg-white">
      <Table aria-label="cms-data-table" className={TABLE_CLS}>
        <Table.Content>
          <Table.Header className="bg-os-bg-base">
            {columns.map((c, i) => (
              <Table.Column key={c.id} id={c.id} isRowHeader={c.isRowHeader ?? i === 0}>
                {c.header}
              </Table.Column>
            ))}
            {actions && <Table.Column id="__actions">操作</Table.Column>}
          </Table.Header>
          <Table.Body items={rows}>
            {(row: T) => (
              <Table.Row id={rowKey(row)} className="border-b border-os-border-light hover:bg-os-bg-hover transition-colors">
                {columns.map((c) => (
                  <Table.Cell key={c.id} className={c.cellClassName}>
                    {c.render(row)}
                  </Table.Cell>
                ))}
                {actions && <Table.Cell>{actions(row)}</Table.Cell>}
              </Table.Row>
            )}
          </Table.Body>
        </Table.Content>
      </Table>
    </div>
  )
}
