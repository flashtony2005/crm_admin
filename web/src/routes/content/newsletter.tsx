import { createFileRoute } from '@tanstack/react-router'
import { useState } from 'react'
import { Button, Input, Label } from '@heroui/react'
import { TextArea } from '@heroui/react/textarea'
import { subscribersApi, newsletterApi, type Subscriber } from '../../api/cms'
import { CmsDataTable, type CmsColumn } from '../../components/cms/CmsDataTable'
import { CmsPageHeader } from '../../components/cms/CmsPageHeader'
import { CmsToolbar } from '../../components/cms/CmsToolbar'
import { useCmsCollection } from '../../components/cms/useCmsCollection'
import { fmtDate } from '../../components/cms/format'
import { Auth } from '../../components/cms/Auth'
import { P } from '../../config/permissions'
import { toast } from '../../components/cms/toast'

function NewsletterPage() {
  const t = useCmsCollection(subscribersApi, ['cms-subscribers'], { searchFields: ['email', 'name'] })
  const [subject, setSubject] = useState('')
  const [body, setBody] = useState('')
  const [sending, setSending] = useState(false)

  const send = async () => {
    if (!subject.trim() || !body.trim()) { toast.error('主题与正文必填'); return }
    setSending(true)
    try {
      const r = await newsletterApi.send(subject, body)
      toast.success(`群发完成：送达 ${r.delivered} / ${r.total}${r.testMode ? '（测试模式：SMTP 未配置）' : ''}`)
      setSubject(''); setBody('')
    } catch (e: any) {
      toast.error(e?.message || '群发失败')
    } finally {
      setSending(false)
    }
  }

  const columns: CmsColumn<Subscriber>[] = [
    { id: 'email', header: '邮箱', render: (r) => <span className="font-medium">{r.email}</span> },
    { id: 'name', header: '昵称', render: (r) => <span>{r.name || '—'}</span> },
    {
      id: 'status', header: '状态', render: (r) =>
        <span className={`px-2 py-0.5 rounded-full text-xs ${r.status === 'active' ? 'bg-green-100 text-green-700' : 'bg-gray-100 text-gray-600'}`}>{r.status === 'active' ? '订阅中' : '已退订'}</span>,
    },
    { id: 'createdAt', header: '订阅时间', render: (r) => <time className="text-xs text-os-text-muted">{fmtDate(r.createdAt)}</time> },
  ]

  return (
    <div className="p-1 md:p-2 space-y-4">
      <CmsPageHeader title="邮件订阅" desc="订阅者管理与资讯群发。配置 SMTP 环境变量后即为真实发送（lettre + rustls-tls）。" />
      <div className="rounded-xl border bg-white p-4 space-y-3">
        <h3 className="font-semibold">群发资讯</h3>
        <div className="space-y-1.5"><Label>主题</Label><Input value={subject} onChange={(e) => setSubject(e.target.value)} placeholder="新品尝鲜｜本周会员日" /></div>
        <div className="space-y-1.5"><Label>正文（支持简单 HTML）</Label><TextArea value={body} onChange={(e) => setBody(e.target.value)} rows={4} placeholder="<p>你好，</p>…" /></div>
        <Auth perm={P.newsletterCampaignSend}>
          <Button variant="primary" isDisabled={sending} onPress={() => void send()}>发送群发</Button>
        </Auth>
      </div>
      <CmsToolbar searchPlaceholder="搜索订阅者…" searchValue={t.search} onSearchChange={t.setSearch} />
      <CmsDataTable
        columns={columns}
        rows={t.paged}
        rowKey={(r) => r.id}
        isLoading={t.isLoading}
        emptyIcon="📧"
        emptyTitle="还没有订阅者"
        emptyHint="访客在公开站底部订阅框提交后即出现在此"
      />
    </div>
  )
}

export const Route = createFileRoute('/content/newsletter')({ component: NewsletterPage })
