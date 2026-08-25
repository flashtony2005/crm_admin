import { StatusBadge } from '../common/StatusBadge'
import type { ContentStatus } from '../../api/cms'
import { CONTENT_STATUS_META } from './contentStatus'

/** 内容状态徽章（Pages / Articles / Products 共用） */
export function ContentStatusBadge({ status }: { status: ContentStatus }) {
  const m = CONTENT_STATUS_META[status] ?? { label: String(status), tone: 'neutral' as const }
  return (
    <StatusBadge tone={m.tone} dot>
      {m.label}
    </StatusBadge>
  )
}
