import { useState, useEffect } from 'react'
import { createFileRoute } from '@tanstack/react-router'
import { Button, Input, Label, toast } from '@heroui/react'

import {
  webhooksApi,
  triggerWebhookTest,
  getDeliveryLogs,
  clearDeliveryLogs,
  getDefaultSamplePayload,
  type WebhookSubscription,
  type WebhookDeliveryLog,
  type WebhookTestResult,
} from '../../api/cms'
import { CmsPageHeader } from '../../components/cms/CmsPageHeader'
import { useCmsCollection } from '../../components/cms/useCmsCollection'
import { Auth } from '../../components/cms/Auth'
import { P } from '../../config/permissions'
import { fmtRelative } from '../../components/cms/format'

function SendIcon() {
  return (
    <svg width={15} height={15} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <line x1="22" y1="2" x2="11" y2="13" />
      <polygon points="22 2 15 22 11 13 2 9 22 2" />
    </svg>
  )
}

function WebhookIcon() {
  return (
    <svg width={16} height={16} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <path d="M18 16.98h-5.99c-1.1 0-1.95.94-2.48 1.9A4 4 0 0 1 2 17c0-2.21 1.79-4 4-4h1" />
      <path d="M6 7.02h5.99c1.1 0 1.95-.94 2.48-1.9A4 4 0 0 1 22 7c0 2.21-1.79 4-4 4h-1" />
      <circle cx="6" cy="7" r="1" />
      <circle cx="18" cy="17" r="1" />
    </svg>
  )
}

function HistoryIcon() {
  return (
    <svg width={15} height={15} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <circle cx="12" cy="12" r="10" />
      <polyline points="12 6 12 12 16 14" />
    </svg>
  )
}

function KeyIcon() {
  return (
    <svg width={14} height={14} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <circle cx="7.5" cy="15.5" r="5.5" />
      <path d="m21 2-9.6 9.6 M15.5 7.5l3 3 M18.5 4.5l3 3" />
    </svg>
  )
}

function TrashIcon() {
  return (
    <svg width={14} height={14} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <polyline points="3 6 5 6 21 6" />
      <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
    </svg>
  )
}

function PlusIcon() {
  return (
    <svg width={15} height={15} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <line x1="12" y1="5" x2="12" y2="19" />
      <line x1="5" y1="12" x2="19" y2="12" />
    </svg>
  )
}

function CheckCircleIcon() {
  return (
    <svg width={16} height={16} viewBox="0 0 24 24" fill="none" stroke="#16a34a" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M22 11.08V12a10 10 0 1 1-5.93-9.14" />
      <polyline points="22 4 12 14.01 9 11.01" />
    </svg>
  )
}

const EVENT_PRESETS = [
  { event: 'ping', label: 'ping (心跳与握手测试)' },
  { event: 'order.paid', label: 'order.paid (订单收款成功)' },
  { event: 'member.registered', label: 'member.registered (新会员注册)' },
  { event: 'article.published', label: 'article.published (文章上线发布)' },
  { event: 'comment.created', label: 'comment.created (收到新评论)' },
  { event: 'lead.created', label: 'lead.created (销售意向线索)' },
  { event: '*', label: '* (订阅所有出站事件)' },
]

function WebhooksPage() {
  const [activeTab, setActiveTab] = useState<'list' | 'tester' | 'logs'>('list')
  const t = useCmsCollection(webhooksApi, ['cms-webhooks'], { searchFields: ['event', 'url'] })

  // 新增/编辑弹窗状态
  const [modalOpen, setModalOpen] = useState(false)
  const [editingItem, setEditingItem] = useState<WebhookSubscription | null>(null)
  const [form, setForm] = useState({ event: 'order.paid', url: '', secret: '', active: true })

  // 测试器状态
  const [testTargetSubId, setTestTargetSubId] = useState<string>('')
  const [testEvent, setTestEvent] = useState<string>('order.paid')
  const [testUrl, setTestUrl] = useState<string>('')
  const [testSecret, setTestSecret] = useState<string>('')
  const [testPayloadStr, setTestPayloadStr] = useState<string>('')
  const [testing, setTesting] = useState(false)
  const [lastTestResult, setLastTestResult] = useState<WebhookTestResult | null>(null)

  // 投递日志
  const [logs, setLogs] = useState<WebhookDeliveryLog[]>([])
  const [inspectingLog, setInspectingLog] = useState<WebhookDeliveryLog | null>(null)

  const reloadLogs = () => {
    setLogs(getDeliveryLogs())
  }

  useEffect(() => {
    reloadLogs()
  }, [])

  // 当切换测试事件时，同步更新默认载荷示例
  useEffect(() => {
    if (!testPayloadStr || testPayloadStr === JSON.stringify(getDefaultSamplePayload(testEvent), null, 2)) {
      setTestPayloadStr(JSON.stringify(getDefaultSamplePayload(testEvent), null, 2))
    }
  }, [testEvent])

  // 打开创建弹窗
  const handleOpenCreate = () => {
    setEditingItem(null)
    setForm({ event: 'order.paid', url: '', secret: 'whsec_' + Math.random().toString(36).slice(2, 10), active: true })
    setModalOpen(true)
  }

  // 打开编辑弹窗
  const handleOpenEdit = (item: WebhookSubscription) => {
    setEditingItem(item)
    setForm({ event: item.event, url: item.url, secret: item.secret || '', active: item.active })
    setModalOpen(true)
  }

  // 保存新增或编辑
  const handleSaveForm = async () => {
    if (!form.url.trim()) {
      toast.danger('目标 URL 不能为空')
      return
    }
    try {
      if (editingItem) {
        await webhooksApi.update(editingItem.id, {
          event: form.event,
          url: form.url.trim(),
          secret: form.secret.trim(),
          active: form.active,
        })
        toast.success('Webhook 订阅已更新')
      } else {
        await webhooksApi.create({
          event: form.event,
          url: form.url.trim(),
          secret: form.secret.trim(),
          active: form.active,
          deliveries: 0,
        })
        toast.success('已创建出站 Webhook 订阅')
      }
      setModalOpen(false)
      t.refetch()
    } catch (e: unknown) {
      toast.danger((e as Error).message || '保存失败')
    }
  }

  // 从行内快速打开测试器
  const handleTestItem = (item: WebhookSubscription) => {
    setTestTargetSubId(item.id)
    setTestEvent(item.event === '*' ? 'order.paid' : item.event)
    setTestUrl(item.url)
    setTestSecret(item.secret || '')
    setTestPayloadStr(JSON.stringify(getDefaultSamplePayload(item.event === '*' ? 'order.paid' : item.event), null, 2))
    setLastTestResult(null)
    setActiveTab('tester')
  }

  // 执行测试发送
  const handleRunTest = async () => {
    const targetUrl = testUrl.trim()
    if (!targetUrl) {
      toast.danger('请输入测试目标 Webhook URL')
      return
    }

    let parsedPayload: unknown
    try {
      parsedPayload = JSON.parse(testPayloadStr)
    } catch {
      toast.danger('请求体 JSON 格式不合法，请检查后再发送')
      return
    }

    setTesting(true)
    try {
      const res = await triggerWebhookTest(testEvent, {
        url: targetUrl,
        payload: parsedPayload,
        secret: testSecret.trim(),
        subscriptionId: testTargetSubId || undefined,
      })
      setLastTestResult(res)
      reloadLogs()
      t.refetch()
      toast.success(`Webhook 测试投递成功 [HTTP ${res.status}]，耗时 ${res.durationMs}ms`)
    } catch (e: unknown) {
      toast.danger((e as Error).message || '投递测试失败')
    } finally {
      setTesting(false)
    }
  }

  // 快捷发送 Ping 心跳
  const handleQuickPing = async () => {
    try {
      const res = await triggerWebhookTest('ping')
      reloadLogs()
      t.refetch()
      toast.success(`已触发 Ping 心跳测试 [耗时 ${res.durationMs}ms]`)
    } catch (e: unknown) {
      toast.danger((e as Error).message || '触发失败')
    }
  }

  return (
    <div className="p-2 md:p-4 max-w-7xl mx-auto space-y-6">
      {/* 顶部标题与主要操作 */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
        <CmsPageHeader
          title="出站 Webhook 与事件推送"
          desc="把站内事件（订单支付、新客注册、文章发布、评论提交等）以 HMAC-SHA256 签名 POST 推送到外部系统（企业微信、Slack、Zapier、自建服务等）。"
        />
        <div className="flex items-center gap-2 shrink-0">
          <Button
            variant="ghost"
            size="sm"
            onPress={() => void handleQuickPing()}
            className="text-xs text-indigo-700 bg-indigo-50 hover:bg-indigo-100"
          >
            ⚡ 快捷 Ping 测试
          </Button>
          <Button
            variant="outline"
            size="sm"
            onPress={() => {
              setTestTargetSubId('')
              setTestUrl('https://qyapi.weixin.qq.com/cgi-bin/webhook/send?key=demo-key-123')
              setTestEvent('order.paid')
              setTestPayloadStr(JSON.stringify(getDefaultSamplePayload('order.paid'), null, 2))
              setLastTestResult(null)
              setActiveTab('tester')
            }}
            className="flex items-center gap-1.5 text-xs"
          >
            <SendIcon /> 联调测试器
          </Button>
          <Auth perm={P.webhooksCreate}>
            <Button
              variant="primary"
              size="sm"
              onPress={handleOpenCreate}
              className="flex items-center gap-1.5 text-xs"
            >
              <PlusIcon /> 添加 Webhook
            </Button>
          </Auth>
        </div>
      </div>

      {/* 选项卡切换 */}
      <div className="flex items-center border-b border-os-border-light gap-2">
        <button
          type="button"
          onClick={() => setActiveTab('list')}
          className={`flex items-center gap-2 px-4 py-2.5 text-sm font-semibold border-b-2 transition-colors ${
            activeTab === 'list'
              ? 'border-indigo-600 text-indigo-600'
              : 'border-transparent text-os-text-secondary hover:text-os-text-primary'
          }`}
        >
          <WebhookIcon /> 订阅清单 ({t.paged.length})
        </button>

        <button
          type="button"
          onClick={() => setActiveTab('tester')}
          className={`flex items-center gap-2 px-4 py-2.5 text-sm font-semibold border-b-2 transition-colors ${
            activeTab === 'tester'
              ? 'border-indigo-600 text-indigo-600'
              : 'border-transparent text-os-text-secondary hover:text-os-text-primary'
          }`}
        >
          <SendIcon /> Webhook 模拟测试
        </button>

        <button
          type="button"
          onClick={() => {
            reloadLogs()
            setActiveTab('logs')
          }}
          className={`flex items-center gap-2 px-4 py-2.5 text-sm font-semibold border-b-2 transition-colors ${
            activeTab === 'logs'
              ? 'border-indigo-600 text-indigo-600'
              : 'border-transparent text-os-text-secondary hover:text-os-text-primary'
          }`}
        >
          <HistoryIcon /> 投递历史日志 ({logs.length})
        </button>
      </div>

      {/* ── 视图 1：订阅列表 ── */}
      {activeTab === 'list' && (
        <div className="space-y-4">
          <div className="flex flex-col sm:flex-row items-center justify-between gap-3 bg-white p-3.5 rounded-xl border border-os-border shadow-xs">
            <input
              type="text"
              placeholder="搜索事件名称或目标 URL…"
              value={t.search}
              onChange={(e) => t.setSearch(e.target.value)}
              className="w-full sm:w-80 h-9 px-3 text-xs rounded-lg border border-os-border outline-none focus:border-indigo-500"
            />
            <p className="text-xs text-os-text-muted">
              每个出站请求均携带 <code className="font-mono text-indigo-600">X-Webhook-Event</code>、时间戳和 HMAC SHA-256 签名防篡改。
            </p>
          </div>

          <div className="rounded-2xl border border-os-border bg-white shadow-xs overflow-hidden">
            {t.isLoading ? (
              <div className="py-16 text-center text-xs text-os-text-muted">加载订阅配置中…</div>
            ) : t.paged.length === 0 ? (
              <div className="py-16 text-center text-xs text-os-text-muted">
                暂无 Webhook 订阅，点击右上角「+ 添加 Webhook」快速接入外部系统。
              </div>
            ) : (
              <div className="divide-y divide-os-border-light">
                {t.paged.map((row) => (
                  <div
                    key={row.id}
                    className="p-4 sm:p-5 flex flex-col md:flex-row md:items-center justify-between gap-4 hover:bg-os-bg-hover transition-colors"
                  >
                    <div className="space-y-1.5 min-w-0 flex-1">
                      <div className="flex items-center gap-2.5 flex-wrap">
                        <span className="px-2.5 py-0.5 rounded-md text-xs font-mono font-bold bg-indigo-50 text-indigo-700 border border-indigo-200">
                          {row.event}
                        </span>
                        {row.active ? (
                          <span className="inline-flex items-center gap-1 text-[11px] px-2 py-0.5 rounded-full font-medium bg-emerald-50 text-emerald-700 border border-emerald-200">
                            <span className="w-1.5 h-1.5 rounded-full bg-emerald-500" /> 运行中
                          </span>
                        ) : (
                          <span className="inline-flex items-center gap-1 text-[11px] px-2 py-0.5 rounded-full font-medium bg-gray-100 text-gray-500">
                            已停用
                          </span>
                        )}
                        {row.secret ? (
                          <span className="inline-flex items-center gap-1 text-[10px] text-gray-500 font-mono">
                            <KeyIcon /> HMAC-SHA256 鉴权
                          </span>
                        ) : (
                          <span className="text-[10px] text-gray-400">无签名秘钥</span>
                        )}
                      </div>

                      <div className="text-sm font-mono text-os-text-primary break-all flex items-center gap-2">
                        <span>{row.url}</span>
                        <button
                          type="button"
                          onClick={() => {
                            void navigator.clipboard.writeText(row.url)
                            toast.success('已复制 URL')
                          }}
                          className="text-xs text-os-text-muted hover:text-indigo-600"
                          title="复制 URL"
                        >
                          📋
                        </button>
                      </div>

                      <div className="text-xs text-os-text-muted flex items-center gap-3">
                        <span>总投递: <strong>{row.deliveries ?? 0} 次</strong></span>
                        {row.lastDeliveryAt && (
                          <span>最近投递: {fmtRelative(row.lastDeliveryAt)}</span>
                        )}
                        {row.lastStatus && (
                          <span className={`px-1.5 py-0.2 rounded text-[10px] font-mono ${row.lastStatus === 200 ? 'bg-emerald-100 text-emerald-700' : 'bg-rose-100 text-rose-700'}`}>
                            HTTP {row.lastStatus}
                          </span>
                        )}
                      </div>
                    </div>

                    <div className="flex items-center gap-2 shrink-0">
                      <Button
                        variant="outline"
                        size="sm"
                        onPress={() => handleTestItem(row)}
                        className="text-xs flex items-center gap-1 text-indigo-700 hover:bg-indigo-50 border-indigo-200"
                      >
                        <SendIcon /> 测试发送
                      </Button>
                      <Button
                        variant="ghost"
                        size="sm"
                        onPress={() => handleOpenEdit(row)}
                        className="text-xs"
                      >
                        编辑
                      </Button>
                      <Auth perm={P.webhooksDelete}>
                        <button
                          type="button"
                          onClick={() => {
                            if (confirm('确定要删除此 Webhook 订阅吗？')) {
                              void t.remove.mutateAsync(row.id)
                              toast.success('已删除订阅')
                            }
                          }}
                          className="p-1.5 text-os-text-muted hover:text-rose-600 rounded-lg hover:bg-rose-50 transition-colors"
                          title="删除"
                        >
                          <TrashIcon />
                        </button>
                      </Auth>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>
      )}

      {/* ── 视图 2：出站 Webhook 测试与联调器 ── */}
      {activeTab === 'tester' && (
        <div className="grid grid-cols-1 lg:grid-cols-12 gap-6">
          {/* 左侧配置栏 */}
          <div className="lg:col-span-6 bg-white p-5 rounded-2xl border border-os-border shadow-xs space-y-4">
            <div className="flex items-center justify-between pb-3 border-b border-os-border-light">
              <h3 className="text-base font-bold text-os-text-primary flex items-center gap-2">
                <SendIcon /> Webhook 模拟测试器
              </h3>
              <span className="text-xs text-os-text-muted">实时模拟事件触发并检验接收端</span>
            </div>

            {/* 选择现有订阅或自定义 */}
            <div>
              <Label className="text-xs font-medium text-os-text-secondary">
                关联已有订阅 (可选)
              </Label>
              <select
                value={testTargetSubId}
                onChange={(e) => {
                  const subId = e.target.value
                  setTestTargetSubId(subId)
                  const sub = t.paged.find((s) => s.id === subId)
                  if (sub) {
                    setTestUrl(sub.url)
                    setTestSecret(sub.secret || '')
                    setTestEvent(sub.event === '*' ? 'order.paid' : sub.event)
                  }
                }}
                className="w-full h-9 px-3 mt-1 text-xs rounded-lg border border-os-border bg-white outline-none focus:border-indigo-500"
              >
                <option value="">-- 手动输入自定义接收 URL --</option>
                {t.paged.map((s) => (
                  <option key={s.id} value={s.id}>
                    [{s.event}] {s.url}
                  </option>
                ))}
              </select>
            </div>

            {/* 目标 URL */}
            <div>
              <Label className="text-xs font-medium text-os-text-secondary">
                接收端 URL (POST Endpoint) *
              </Label>
              <input
                type="text"
                placeholder="https://example.com/api/webhook"
                value={testUrl}
                onChange={(e) => setTestUrl(e.target.value)}
                className="w-full h-9 px-3 mt-1 text-xs font-mono rounded-lg border border-os-border outline-none focus:border-indigo-500"
              />
            </div>

            {/* 事件选择 */}
            <div className="grid grid-cols-2 gap-3">
              <div>
                <Label className="text-xs font-medium text-os-text-secondary">
                  触发事件类型 (X-Webhook-Event)
                </Label>
                <select
                  value={testEvent}
                  onChange={(e) => setTestEvent(e.target.value)}
                  className="w-full h-9 px-2 mt-1 text-xs rounded-lg border border-os-border bg-white outline-none focus:border-indigo-500 font-medium"
                >
                  {EVENT_PRESETS.map((p) => (
                    <option key={p.event} value={p.event}>
                      {p.label}
                    </option>
                  ))}
                </select>
              </div>

              <div>
                <Label className="text-xs font-medium text-os-text-secondary">
                  HMAC 签名秘钥 (Secret)
                </Label>
                <div className="flex gap-1.5 mt-1">
                  <input
                    type="text"
                    placeholder="whsec_..."
                    value={testSecret}
                    onChange={(e) => setTestSecret(e.target.value)}
                    className="flex-1 h-9 px-2.5 text-xs font-mono rounded-lg border border-os-border outline-none focus:border-indigo-500"
                  />
                  <Button
                    variant="ghost"
                    size="sm"
                    onPress={() => setTestSecret('whsec_' + Math.random().toString(36).slice(2, 10))}
                    className="text-[11px] px-2"
                  >
                    生成
                  </Button>
                </div>
              </div>
            </div>

            {/* 载荷编辑 */}
            <div>
              <div className="flex items-center justify-between mb-1">
                <Label className="text-xs font-medium text-os-text-secondary">
                  请求体数据载荷 (Payload JSON)
                </Label>
                <div className="flex items-center gap-2">
                  <button
                    type="button"
                    onClick={() => {
                      try {
                        const parsed = JSON.parse(testPayloadStr)
                        setTestPayloadStr(JSON.stringify(parsed, null, 2))
                        toast.success('JSON 已格式化')
                      } catch {
                        toast.danger('JSON 语法错误，无法格式化')
                      }
                    }}
                    className="text-[11px] text-indigo-600 hover:text-indigo-800"
                  >
                    格式化
                  </button>
                  <span className="text-gray-300">|</span>
                  <button
                    type="button"
                    onClick={() => setTestPayloadStr(JSON.stringify(getDefaultSamplePayload(testEvent), null, 2))}
                    className="text-[11px] text-gray-500 hover:text-gray-700"
                  >
                    重置为预设
                  </button>
                </div>
              </div>
              <textarea
                rows={10}
                value={testPayloadStr}
                onChange={(e) => setTestPayloadStr(e.target.value)}
                className="w-full p-3 font-mono text-xs rounded-xl border border-os-border bg-slate-900 text-emerald-400 outline-none focus:border-indigo-500"
              />
            </div>

            {/* 发送按钮 */}
            <div className="pt-2">
              <Button
                variant="primary"
                size="md"
                isDisabled={testing || !testUrl.trim()}
                onPress={() => void handleRunTest()}
                className="w-full flex items-center justify-center gap-2 font-bold"
              >
                <SendIcon /> {testing ? '正在投递请求中…' : '立即发送测试 Webhook 请求'}
              </Button>
            </div>
          </div>

          {/* 右侧测试结果检查板 */}
          <div className="lg:col-span-6 bg-white p-5 rounded-2xl border border-os-border shadow-xs space-y-4 flex flex-col justify-between">
            <div>
              <div className="flex items-center justify-between pb-3 border-b border-os-border-light">
                <h3 className="text-base font-bold text-os-text-primary">
                  投递响应与报文详情 (Inspector)
                </h3>
                {lastTestResult && (
                  <span className="text-xs text-os-text-muted">
                    投递 ID: <code className="font-mono text-indigo-600">{lastTestResult.deliveryId}</code>
                  </span>
                )}
              </div>

              {!lastTestResult ? (
                <div className="py-24 flex flex-col items-center justify-center text-center text-os-text-muted space-y-3">
                  <div className="w-12 h-12 rounded-full bg-indigo-50 text-indigo-600 flex items-center justify-center">
                    <SendIcon />
                  </div>
                  <p className="text-sm font-medium">尚未执行测试</p>
                  <p className="text-xs max-w-sm">
                    在左侧确认目标 URL、事件与 Payload 后，点击「立即发送测试 Webhook 请求」，即可在此处检查服务端响应与请求头明细。
                  </p>
                </div>
              ) : (
                <div className="space-y-4 mt-3">
                  {/* 状态指示栏 */}
                  <div className="p-3.5 rounded-xl border bg-slate-50 flex items-center justify-between flex-wrap gap-2">
                    <div className="flex items-center gap-2">
                      <CheckCircleIcon />
                      <span className="text-sm font-bold text-slate-800">
                        HTTP {lastTestResult.status} {lastTestResult.statusText}
                      </span>
                    </div>
                    <div className="flex items-center gap-2 text-xs font-mono">
                      <span className="px-2 py-0.5 rounded bg-white border border-gray-200 text-gray-700">
                        耗时: {lastTestResult.durationMs}ms
                      </span>
                      <span className="px-2 py-0.5 rounded bg-emerald-100 text-emerald-800 font-bold">
                        DELIVERED
                      </span>
                    </div>
                  </div>

                  {/* 请求头摘要 */}
                  <div>
                    <h4 className="text-xs font-bold text-os-text-secondary uppercase tracking-wider mb-1.5">
                      Request Headers (模拟发送头部)
                    </h4>
                    <div className="p-3 rounded-xl bg-slate-100 font-mono text-xs space-y-1 overflow-x-auto">
                      {Object.entries(lastTestResult.requestHeaders).map(([k, v]) => (
                        <div key={k} className="flex gap-2">
                          <span className="text-indigo-600 font-semibold">{k}:</span>
                          <span className="text-slate-700 break-all">{v}</span>
                        </div>
                      ))}
                    </div>
                  </div>

                  {/* 响应体 */}
                  <div>
                    <h4 className="text-xs font-bold text-os-text-secondary uppercase tracking-wider mb-1.5">
                      Response Body (接收端返回)
                    </h4>
                    <pre className="p-3 rounded-xl bg-slate-900 text-emerald-300 font-mono text-xs overflow-x-auto max-h-48">
                      {lastTestResult.responseBody}
                    </pre>
                  </div>
                </div>
              )}
            </div>

            <div className="pt-3 border-t border-os-border-light text-xs text-os-text-muted flex items-center justify-between">
              <span>测试记录已自动写入投递历史日志。</span>
              <button
                type="button"
                onClick={() => setActiveTab('logs')}
                className="text-indigo-600 hover:text-indigo-800 font-medium"
              >
                查看完整日志 →
              </button>
            </div>
          </div>
        </div>
      )}

      {/* ── 视图 3：投递历史日志 ── */}
      {activeTab === 'logs' && (
        <div className="space-y-4">
          <div className="flex items-center justify-between bg-white p-3.5 rounded-xl border border-os-border shadow-xs">
            <div className="flex items-center gap-2 text-xs text-os-text-muted">
              <span>共记录 <strong>{logs.length}</strong> 条历史投递，用于排查外部系统对接异常与重发验证。</span>
            </div>
            <div className="flex items-center gap-2">
              <Button variant="ghost" size="sm" onPress={reloadLogs} className="text-xs">
                刷新日志
              </Button>
              <Button
                variant="ghost"
                size="sm"
                onPress={() => {
                  if (confirm('确定要清空所有投递历史记录吗？')) {
                    clearDeliveryLogs()
                    reloadLogs()
                    toast.success('已清空投递日志')
                  }
                }}
                className="text-xs text-rose-600 hover:bg-rose-50"
              >
                清空日志
              </Button>
            </div>
          </div>

          <div className="rounded-2xl border border-os-border bg-white shadow-xs overflow-hidden">
            {logs.length === 0 ? (
              <div className="py-16 text-center text-xs text-os-text-muted">
                暂无历史投递记录，点击「Webhook 模拟测试」即可触发测试生成日志。
              </div>
            ) : (
              <div className="divide-y divide-os-border-light">
                {logs.map((log) => (
                  <div
                    key={log.id}
                    className="p-4 flex flex-col md:flex-row md:items-center justify-between gap-3 hover:bg-os-bg-hover transition-colors"
                  >
                    <div className="space-y-1 min-w-0 flex-1">
                      <div className="flex items-center gap-2.5 flex-wrap">
                        <span className="px-2 py-0.5 rounded text-xs font-mono font-bold bg-indigo-50 text-indigo-700">
                          {log.event}
                        </span>
                        <span className="px-2 py-0.5 rounded text-xs font-mono font-bold bg-emerald-100 text-emerald-800">
                          HTTP {log.status}
                        </span>
                        <span className="text-xs text-os-text-muted">
                          {fmtRelative(log.createdAt)}
                        </span>
                        <span className="text-xs text-os-text-muted font-mono">
                          {log.durationMs}ms
                        </span>
                      </div>
                      <p className="text-xs font-mono text-os-text-secondary truncate">
                        {log.url}
                      </p>
                    </div>

                    <div className="flex items-center gap-2 shrink-0">
                      <Button
                        variant="outline"
                        size="sm"
                        onPress={() => setInspectingLog(log)}
                        className="text-xs"
                      >
                        报文详情
                      </Button>
                      <Button
                        variant="ghost"
                        size="sm"
                        onPress={() => {
                          setTestUrl(log.url)
                          setTestEvent(log.event)
                          setTestPayloadStr(log.requestBody)
                          setActiveTab('tester')
                          toast.success('已将报文载入测试器')
                        }}
                        className="text-xs text-indigo-600"
                      >
                        重测
                      </Button>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>
      )}

      {/* ── 弹窗 1：创建 / 编辑 Webhook 订阅 ── */}
      {modalOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/40 backdrop-blur-xs">
          <div className="w-full max-w-lg bg-white rounded-2xl shadow-2xl border border-os-border overflow-hidden animate-in fade-in duration-150">
            <div className="p-5 border-b border-os-border bg-os-bg-base flex items-center justify-between">
              <h3 className="text-base font-bold text-os-text-primary flex items-center gap-2">
                <WebhookIcon /> {editingItem ? '编辑 Webhook 订阅' : '添加出站 Webhook'}
              </h3>
              <button
                type="button"
                onClick={() => setModalOpen(false)}
                className="w-8 h-8 rounded-lg hover:bg-gray-200 flex items-center justify-center text-gray-500"
              >
                ✕
              </button>
            </div>

            <div className="p-5 space-y-4">
              <div>
                <Label className="text-xs font-medium text-os-text-secondary">
                  订阅事件 (Event) *
                </Label>
                <select
                  value={form.event}
                  onChange={(e) => setForm({ ...form, event: e.target.value })}
                  className="w-full h-9 px-3 mt-1 text-xs rounded-lg border border-os-border bg-white outline-none focus:border-indigo-500 font-medium"
                >
                  {EVENT_PRESETS.map((p) => (
                    <option key={p.event} value={p.event}>
                      {p.label}
                    </option>
                  ))}
                </select>
                <p className="text-[11px] text-os-text-muted mt-1">
                  选择当系统发生何种业务事件时向目标地址推送数据。
                </p>
              </div>

              <div>
                <Label className="text-xs font-medium text-os-text-secondary">
                  接收端 URL (Endpoint) *
                </Label>
                <Input
                  value={form.url}
                  onChange={(e: React.ChangeEvent<HTMLInputElement>) => setForm({ ...form, url: e.target.value })}
                  placeholder="https://qyapi.weixin.qq.com/... 或 https://example.com/webhook"
                />
              </div>

              <div>
                <Label className="text-xs font-medium text-os-text-secondary">
                  签名秘钥 (HMAC Secret)
                </Label>
                <div className="flex gap-2 mt-1">
                  <input
                    type="text"
                    value={form.secret}
                    onChange={(e) => setForm({ ...form, secret: e.target.value })}
                    placeholder="whsec_..."
                    className="flex-1 h-9 px-3 text-xs font-mono rounded-lg border border-os-border outline-none focus:border-indigo-500"
                  />
                  <Button
                    variant="ghost"
                    size="sm"
                    onPress={() => setForm({ ...form, secret: 'whsec_' + Math.random().toString(36).slice(2, 10) })}
                    className="text-xs"
                  >
                    随机生成
                  </Button>
                </div>
                <p className="text-[11px] text-os-text-muted mt-1">
                  用于为 Payload 生成 <code className="font-mono text-indigo-600">X-Signature-SHA256</code> 签名头，接收方凭此校验请求真实性。
                </p>
              </div>

              <div className="flex items-center justify-between pt-2">
                <div>
                  <p className="text-xs font-bold text-os-text-primary">启用此订阅</p>
                  <p className="text-[11px] text-os-text-muted">停用后将暂停该端点的事件推送</p>
                </div>
                <input
                  type="checkbox"
                  checked={form.active}
                  onChange={(e) => setForm({ ...form, active: e.target.checked })}
                  className="w-5 h-5 rounded text-indigo-600 border-gray-300 focus:ring-indigo-500 cursor-pointer"
                />
              </div>
            </div>

            <div className="p-4 border-t border-os-border bg-os-bg-base flex items-center justify-end gap-2">
              <Button variant="ghost" size="sm" onPress={() => setModalOpen(false)}>
                取消
              </Button>
              <Button variant="primary" size="sm" onPress={() => void handleSaveForm()}>
                保存配置
              </Button>
            </div>
          </div>
        </div>
      )}

      {/* ── 弹窗 2：报文详情检查弹窗 ── */}
      {inspectingLog && (
        <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/40 backdrop-blur-xs">
          <div className="w-full max-w-2xl bg-white rounded-2xl shadow-2xl border border-os-border overflow-hidden animate-in fade-in duration-150">
            <div className="p-5 border-b border-os-border bg-os-bg-base flex items-center justify-between">
              <div>
                <h3 className="text-base font-bold text-os-text-primary flex items-center gap-2">
                  <HistoryIcon /> 投递报文详情
                </h3>
                <p className="text-xs text-os-text-muted mt-0.5">
                  投递 ID: <code className="font-mono text-indigo-600">{inspectingLog.id}</code> · {fmtRelative(inspectingLog.createdAt)}
                </p>
              </div>
              <button
                type="button"
                onClick={() => setInspectingLog(null)}
                className="w-8 h-8 rounded-lg hover:bg-gray-200 flex items-center justify-center text-gray-500"
              >
                ✕
              </button>
            </div>

            <div className="p-5 max-h-[70vh] overflow-y-auto space-y-4">
              <div>
                <h4 className="text-xs font-bold text-os-text-secondary uppercase mb-1">
                  目标 URL & 状态
                </h4>
                <div className="p-2.5 rounded-lg bg-gray-50 border text-xs font-mono break-all flex items-center justify-between">
                  <span>POST {inspectingLog.url}</span>
                  <span className="px-2 py-0.5 rounded bg-emerald-100 text-emerald-800 font-bold shrink-0">
                    HTTP {inspectingLog.status} ({inspectingLog.durationMs}ms)
                  </span>
                </div>
              </div>

              <div>
                <h4 className="text-xs font-bold text-os-text-secondary uppercase mb-1">
                  Request Headers
                </h4>
                <div className="p-3 rounded-lg bg-slate-900 text-slate-200 font-mono text-xs space-y-1 overflow-x-auto">
                  {Object.entries(inspectingLog.requestHeaders).map(([k, v]) => (
                    <div key={k} className="flex gap-2">
                      <span className="text-indigo-400">{k}:</span>
                      <span className="text-emerald-300 break-all">{v}</span>
                    </div>
                  ))}
                </div>
              </div>

              <div>
                <h4 className="text-xs font-bold text-os-text-secondary uppercase mb-1">
                  Request Payload (JSON)
                </h4>
                <pre className="p-3 rounded-lg bg-slate-900 text-emerald-300 font-mono text-xs overflow-x-auto max-h-56">
                  {inspectingLog.requestBody}
                </pre>
              </div>

              {inspectingLog.responseBody && (
                <div>
                  <h4 className="text-xs font-bold text-os-text-secondary uppercase mb-1">
                    Response Body
                  </h4>
                  <pre className="p-3 rounded-lg bg-slate-100 text-slate-800 font-mono text-xs overflow-x-auto max-h-36">
                    {inspectingLog.responseBody}
                  </pre>
                </div>
              )}
            </div>

            <div className="p-4 border-t border-os-border bg-os-bg-base flex items-center justify-between">
              <Button
                variant="ghost"
                size="sm"
                onPress={() => {
                  setTestUrl(inspectingLog.url)
                  setTestEvent(inspectingLog.event)
                  setTestPayloadStr(inspectingLog.requestBody)
                  setInspectingLog(null)
                  setActiveTab('tester')
                }}
                className="text-xs text-indigo-600"
              >
                在测试器中重试发送 →
              </Button>
              <Button variant="primary" size="sm" onPress={() => setInspectingLog(null)}>
                关闭
              </Button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}

export const Route = createFileRoute('/content/webhooks')({ component: WebhooksPage })
