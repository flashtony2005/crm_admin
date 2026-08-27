import type { ContentStatus } from '../../api/cms'

/** 内容状态元信息（与 StatusBadge 色板对应），供表单选项、表格徽章共用 */
export const CONTENT_STATUS_META: Record<
  ContentStatus,
  { label: string; tone: 'success' | 'purple' | 'neutral' | 'warning' }
> = {
  draft: { label: '草稿', tone: 'neutral' },
  pending_review: { label: '待审核', tone: 'purple' },
  published: { label: '已发布', tone: 'success' },
  offline: { label: '已下线', tone: 'warning' },
  scheduled: { label: '定时发布', tone: 'purple' },
}

/** 表单下拉用的状态选项（值/标签对） */
export const CONTENT_STATUS_OPTIONS = (
  Object.keys(CONTENT_STATUS_META) as ContentStatus[]
).map((value) => ({ value, label: CONTENT_STATUS_META[value].label }))

export function contentStatusLabel(s: ContentStatus): string {
  return CONTENT_STATUS_META[s]?.label ?? String(s)
}
