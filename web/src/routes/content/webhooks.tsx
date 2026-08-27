import { createFileRoute } from '@tanstack/react-router'
import { useState } from 'react'
import { Button, Input, Label } from '@heroui/react'
import { webhooksApi, triggerWebhookTest, type WebhookSubscription } from '../../api/cms'
import { CmsDataTable, type CmsColumn } from '../../components/cms/CmsDataTable'
import { CmsPageHeader } from '../../components/cms/CmsPageHeader'
import { CmsToolbar } from '../../components/cms/CmsToolbar'
import { useCmsCollection } from '../../components/cms/useCmsCollection'
import { Auth } from '../../components/cms/Auth'
import { P } from '../../config/permissions'
import { toast } from '../../components/cms/toast'

function WebhooksPage() {
  const [event, setEvent] = useState('')
  const [url, setUrl] = useState('')
  const [creating, setCreating] = useState(false)
  const t = useCmsCollection(webhooksApi, ['cms-webhooks'], { searchFields: ['event', 'url'] })

  const create = async () => {
    if (!event.trim() || !url.trim()) { toast.error('事件与 URL 必填'); return }
    setCreating(true)
    try {
      await webhooksApi.create({ event, url, secret: '', active: true })
      setEvent(''); setUrl('')
      t.refetch()
      toast.success('已创建订阅')
    } catch (e: any) { toast.error(e?.message || '创建失败') } finally { setCreating(false) }
  }

  const test = async () => {
    try { await triggerWebhookTest('ping'); toast.success('已触发 ping 事件') } catch (e: any) { toast.error(e?.message || '触发失败') }
  }

  const columns: CmsColumn<WebhookSubscription>[] = [
    { id: 'event', header: '事件', render: (r) => <code className="text-xs bg-gray-100 px-1.5 py-0.5 rounded">{r.event}</code> },
    { id: 'url', header: '目标 URL', render: (r) => <span className="text-sm break-all max-w-[360px] block">{r.url}</span> },
    { id: 'deliveries', header: '投递数', render: (r) => <span>{r.deliveries ?? 0}</span> },
    { id: 'active', header: '启用', render: (r) => <span>{r.active ? '是' : '否'}</span> },
  ]

  return (
    <div className="p-1 md:p-2 space-y-4">
      <CmsPageHeader title="出站 Webhook" desc="把站内事件（评论/会员/订阅/文章发布等）以 HMAC 签名 POST 推送到外部系统；失败自动重试。" />
      <div className="rounded-xl border bg-white p-4 grid grid-cols-1 md:grid-cols-3 gap-3 items-end">
        <div className="space-y-1.5"><Label>事件 (event)</Label><Input value={event} onChange={(e) => setEvent(e.target.value)} placeholder="comment.created / member.registered / *" /></div>
        <div className="space-y-1.5"><Label>目标 URL</Label><Input value={url} onChange={(e) => setUrl(e.target.value)} placeholder="https://example.com/hook" /></div>
        <div className="flex gap-2">
          <Button variant="primary" isDisabled={creating} onPress={() => void create()}>+ 添加</Button>
          <Button variant="ghost" onPress={() => void test()}>测试 ping</Button>
        </div>
      </div>
      <CmsToolbar searchPlaceholder="搜索事件 / URL…" searchValue={t.search} onSearchChange={t.setSearch} />
      <CmsDataTable
        columns={columns}
        rows={t.paged}
        rowKey={(r) => r.id}
        isLoading={t.isLoading}
        emptyIcon="🔗"
        emptyTitle="还没有 Webhook 订阅"
        actions={(row) => (
          <Auth perm={P.webhooksDelete}>
            <Button variant="ghost" size="sm" className="text-red-500" onPress={() => t.remove.mutateAsync(row.id)}>删除</Button>
          </Auth>
        )}
      />
    </div>
  )
}

export const Route = createFileRoute('/content/webhooks')({ component: WebhooksPage })
