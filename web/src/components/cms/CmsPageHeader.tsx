import type { ReactNode } from 'react'

interface Props {
  /** 页面标题（面包屑之外的大标题） */
  title: ReactNode
  /** 一句话说明 */
  desc?: ReactNode
  /** 右侧动作区（新建按钮等） */
  actions?: ReactNode
}

/**
 * CMS 页头（抽象组件）：所有内容/业务页共用，保持一致的标题层级与间距。
 */
export function CmsPageHeader({ title, desc, actions }: Props) {
  return (
    <div className="flex items-start justify-between gap-4 mb-4">
      <div className="min-w-0">
        <h1 className="text-xl font-semibold text-os-text-primary tracking-tight">{title}</h1>
        {desc && <p className="text-sm text-os-text-muted mt-0.5">{desc}</p>}
      </div>
      {actions && <div className="flex items-center gap-2 flex-shrink-0">{actions}</div>}
    </div>
  )
}
