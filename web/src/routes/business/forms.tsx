import { createFileRoute } from '@tanstack/react-router'
import { Button } from '@heroui/react'
import { useQuery } from '@tanstack/react-query'

import { formsApi, type FormDef } from '../../api/cms'
import { CmsPageHeader } from '../../components/cms/CmsPageHeader'
import { fmtDate } from '../../components/cms/format'
import { StatusBadge } from '../../components/common/StatusBadge'

function FormsPage() {
  const { data: forms = [], isLoading } = useQuery({
    queryKey: ['cms-forms'],
    queryFn: () => formsApi.list(),
  })

  return (
    <div className="p-1 md:p-2">
      <CmsPageHeader
        title="Forms"
        desc="收集客户信息：预约、询价、会员登记。"
        actions={<Button variant="primary" size="sm" isDisabled>+ 新建表单</Button>}
      />

      {isLoading ? (
        <p className="text-sm text-os-text-muted py-16 text-center">加载中…</p>
      ) : (
        <div className="grid md:grid-cols-2 xl:grid-cols-3 gap-3">
          {forms.map((f) => <FormCard key={f.id} form={f} />)}
        </div>
      )}
    </div>
  )
}

function FormCard({ form }: { form: FormDef }) {
  return (
    <div className="rounded-xl border border-os-border bg-white p-4 shadow-sm hover:shadow-md transition-shadow">
      <div className="flex items-center justify-between mb-2">
        <StatusBadge tone={form.status === 'open' ? 'success' : 'neutral'} dot>
          {form.status === 'open' ? '收集中' : '已关闭'}
        </StatusBadge>
        <time className="text-xs text-os-text-muted">{fmtDate(form.createdAt)}</time>
      </div>
      <h3 className="text-base font-medium text-os-text-primary">{form.title}</h3>
      <p className="text-xs text-os-text-muted mt-1">{form.fieldCount} 个字段</p>

      <div className="flex items-end justify-between mt-4 pt-3 border-t border-os-border-light">
        <div>
          <p className="text-2xl font-semibold text-[#6366F1] tabular-nums">{form.submissions}</p>
          <p className="text-xs text-os-text-muted">提交数</p>
        </div>
        <Button variant="ghost" size="sm" isDisabled>
          分享链接
        </Button>
      </div>
    </div>
  )
}

export const Route = createFileRoute('/business/forms')({
  component: FormsPage,
})
