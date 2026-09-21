import { createFileRoute } from '@tanstack/react-router'
import { useEffect, useState } from 'react'
import { Button, toast } from '@heroui/react'
import {
  communityApi, ordersApi, payAdminApi,
  type FulfillTask, type Order, type PayConfigInfo, type ReconRow,
} from '../../api/cms'
import { CmsDataTable, type CmsColumn } from '../../components/cms/CmsDataTable'
import { CmsPageHeader } from '../../components/cms/CmsPageHeader'
import { CmsToolbar } from '../../components/cms/CmsToolbar'
import { useCmsCollection } from '../../components/cms/useCmsCollection'
import { fmtDate } from '../../components/cms/format'
import { Auth } from '../../components/cms/Auth'
import { P } from '../../config/permissions'

const BIZ_LABEL: Record<string, string> = {
  points_recharge: '充积分',
  plan: '开通订阅',
}

const CHANNEL_LABEL: Record<string, string> = {
  manual: '人工转账',
  wechat: '微信扫码',
  stripe: 'Stripe',
}

const yuan = (cents: number) => `¥${(cents / 100).toFixed(2)}`

/* ---------------- 发货异常卡（F2 outbox） ---------------- */
const STAGE_LABEL: Record<string, string> = {
  points_recharge: '积分发货',
  plan_extend: '订阅发货',
  invite_reward: '邀请奖励',
}

function FulfillCard() {
  const [items, setItems] = useState<FulfillTask[]>([])
  const [loading, setLoading] = useState(false)
  const [retrying, setRetrying] = useState(false)

  const load = () => {
    setLoading(true)
    payAdminApi
      .fulfillPending()
      .then((r) => setItems(r.items))
      .catch(() => setItems([]))
      .finally(() => setLoading(false))
  }

  useEffect(() => {
    load()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  const retry = async () => {
    setRetrying(true)
    try {
      const r = await payAdminApi.retryFulfill()
      if (r.stillFailing.length === 0) {
        toast.success(`已补齐 ${r.recovered} 笔订单的发货`)
      } else {
        toast.danger(`已补齐 ${r.recovered} 笔，仍有 ${r.stillFailing.length} 笔失败：${r.stillFailing[0]}`)
      }
      load()
    } catch (e) {
      toast.danger(e instanceof Error ? e.message : '重试失败')
    } finally {
      setRetrying(false)
    }
  }

  if (loading || items.length === 0) return null

  return (
    <div className="rounded-xl border border-amber-300 bg-amber-50 p-4 mt-6">
      <div className="flex flex-wrap items-center gap-2 mb-2">
        <h3 className="text-sm font-semibold text-amber-900 mr-auto">
          发货异常 · {items.length} 项待处理
        </h3>
        <Auth perm={P.contentMembersUpdate} mode="disable">
          <Button variant="primary" size="sm" isDisabled={retrying} onPress={() => void retry()}>
            重试发货
          </Button>
        </Auth>
      </div>
      <p className="text-xs text-amber-800 mb-3">
        以下订单已收款但发货未全部完成（数据库抖动等导致）。重试发货幂等，已完成的阶段不会重复执行。
      </p>
      <div className="overflow-x-auto">
        <table className="w-full text-xs">
          <thead>
            <tr className="text-amber-900/70 border-b border-amber-200">
              <th className="text-left py-1.5 pr-3">订单号</th>
              <th className="text-left py-1.5 pr-3">阶段</th>
              <th className="text-right py-1.5 pr-3">重试次数</th>
              <th className="text-left py-1.5">最后错误</th>
            </tr>
          </thead>
          <tbody>
            {items.map((t) => (
              <tr key={t.stageKey} className="border-b border-amber-100 last:border-0">
                <td className="py-1.5 pr-3 font-mono">{t.orderNo}</td>
                <td className="py-1.5 pr-3">{STAGE_LABEL[t.stage] ?? t.stage}</td>
                <td className="py-1.5 pr-3 text-right tabular-nums">{t.attempts}</td>
                <td className="py-1.5 text-amber-800 break-all">{t.lastError || '—'}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  )
}

/* ---------------- 对账卡 ---------------- */
function ReconCard() {
  const [days, setDays] = useState(30)
  const [rows, setRows] = useState<ReconRow[]>([])
  const [summary, setSummary] = useState<Record<string, { count: number; amountCents: number }>>({})
  const [loading, setLoading] = useState(false)
  const [closing, setClosing] = useState(false)

  const load = (d: number) => {
    setLoading(true)
    payAdminApi
      .recon(d)
      .then((r) => {
        setRows(r.rows)
        setSummary(r.summary)
      })
      .catch((e) => toast.danger(e instanceof Error ? e.message : '对账加载失败'))
      .finally(() => setLoading(false))
  }

  useEffect(() => {
    load(days)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [days])

  const closeStale = async () => {
    if (!window.confirm('关闭所有创建超过 24 小时仍未支付的订单？')) return
    setClosing(true)
    try {
      const r = await payAdminApi.closeStale()
      toast.success(`已关闭 ${r.closed} 笔超时订单`)
      load(days)
    } catch (e) {
      toast.danger(e instanceof Error ? e.message : '关闭失败')
    } finally {
      setClosing(false)
    }
  }

  // 透视：date -> { channel -> { paid/pending 金额笔数 } }
  const byDate = new Map<string, Map<string, { paid: number; paidN: number; pending: number; pendingN: number }>>()
  for (const r of rows) {
    if (!byDate.has(r.date)) byDate.set(r.date, new Map())
    const m = byDate.get(r.date)!
    if (!m.has(r.channel)) m.set(r.channel, { paid: 0, paidN: 0, pending: 0, pendingN: 0 })
    const c = m.get(r.channel)!
    if (r.status === 'paid') {
      c.paid += r.amountCents
      c.paidN += r.count
    } else if (r.status === 'pending') {
      c.pending += r.amountCents
      c.pendingN += r.count
    }
  }
  const paidTotal = summary.paid?.amountCents ?? 0
  const paidCount = summary.paid?.count ?? 0
  const pendingCountAll = summary.pending?.count ?? 0

  return (
    <div className="rounded-xl border bg-white p-4 mt-6">
      <div className="flex flex-wrap items-center gap-2 mb-3">
        <h3 className="text-sm font-semibold mr-auto">对账（收入汇总）</h3>
        <select
          value={days}
          onChange={(e) => setDays(Number(e.target.value))}
          className="text-sm border rounded-lg px-2 py-1.5"
        >
          <option value={7}>近 7 天</option>
          <option value={30}>近 30 天</option>
          <option value={90}>近 90 天</option>
        </select>
        <Auth perm={P.contentMembersUpdate} mode="disable">
          <Button variant="ghost" size="sm" isDisabled={closing} onPress={() => void closeStale()}>
            关闭超时订单
          </Button>
        </Auth>
      </div>
      <p className="text-xs text-os-text-muted mb-3">
        已收 {paidCount} 笔 {yuan(paidTotal)} · 待确认 {pendingCountAll} 笔 {yuan(summary.pending?.amountCents ?? 0)}
      </p>
      {loading ? (
        <p className="text-xs text-os-text-muted py-4">加载中…</p>
      ) : byDate.size === 0 ? (
        <p className="text-xs text-os-text-muted py-4">窗口内暂无订单</p>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full text-xs">
            <thead>
              <tr className="text-os-text-muted border-b text-left">
                <th className="py-2 pr-3 font-medium">日期</th>
                <th className="py-2 pr-3 font-medium">渠道</th>
                <th className="py-2 pr-3 font-medium">已收</th>
                <th className="py-2 pr-3 font-medium">待确认</th>
              </tr>
            </thead>
            <tbody>
              {[...byDate.entries()].map(([date, channels]) =>
                [...channels.entries()].map(([ch, c]) => (
                  <tr key={`${date}-${ch}`} className="border-b border-os-border last:border-0">
                    <td className="py-1.5 pr-3 tabular-nums">{date}</td>
                    <td className="py-1.5 pr-3">{CHANNEL_LABEL[ch] || ch}</td>
                    <td className="py-1.5 pr-3 tabular-nums text-emerald-600">
                      {c.paidN ? `${yuan(c.paid)}（${c.paidN} 笔）` : '—'}
                    </td>
                    <td className="py-1.5 pr-3 tabular-nums text-amber-600">
                      {c.pendingN ? `${yuan(c.pending)}（${c.pendingN} 笔）` : '—'}
                    </td>
                  </tr>
                )),
              )}
            </tbody>
          </table>
        </div>
      )}
    </div>
  )
}

/* ---------------- 微信支付配置卡 ---------------- */
function PayConfigCard() {
  const [cfg, setCfg] = useState<PayConfigInfo | null>(null)
  const [form, setForm] = useState({
    appid: '', mchid: '', serialNo: '', apiV3Key: '', privateKey: '', platformPubKey: '',
  })
  const [saving, setSaving] = useState(false)

  useEffect(() => {
    payAdminApi
      .getConfig()
      .then((c) => {
        setCfg(c)
        setForm((f) => ({ ...f, appid: c.appid, mchid: c.mchid, serialNo: c.serialNo }))
      })
      .catch(() => {})
  }, [])

  const save = async () => {
    setSaving(true)
    try {
      await payAdminApi.saveConfig(form)
      toast.success('微信支付配置已保存')
      const c = await payAdminApi.getConfig()
      setCfg(c)
      setForm((f) => ({ ...f, apiV3Key: '', privateKey: '', platformPubKey: '' }))
    } catch (e) {
      toast.danger(e instanceof Error ? e.message : '保存失败')
    } finally {
      setSaving(false)
    }
  }

  const upd = (k: keyof typeof form) => (e: { target: { value: string } }) =>
    setForm((f) => ({ ...f, [k]: e.target.value }))

  return (
    <details className="rounded-xl border bg-white p-4 mt-4">
      <summary className="cursor-pointer text-sm font-semibold select-none">
        微信支付配置（Native 扫码）
        {cfg && (
          <span
            className={`ml-2 px-2 py-0.5 rounded-full text-xs font-medium ${
              cfg.ready ? 'bg-emerald-50 text-emerald-600' : 'bg-amber-50 text-amber-600'
            }`}
          >
            {cfg.ready ? '已就绪' : '未配置'}
          </span>
        )}
      </summary>
      <p className="text-xs text-os-text-muted mt-2 mb-3">
        填写微信支付商户平台（pay.weixin.qq.com）的 API 安全参数；回调地址为 PUBLIC_BASE_URL + /api/public/pay/wechat/notify。
        留空的密钥项不会覆盖已保存的值。
      </p>
      <div className="grid grid-cols-1 md:grid-cols-3 gap-3 mb-3">
        <label className="text-xs text-os-text-muted">
          AppID
          <input value={form.appid} onChange={upd('appid')} placeholder="wx..." className="mt-1 w-full text-sm border rounded-lg px-2 py-1.5" />
        </label>
        <label className="text-xs text-os-text-muted">
          商户号 mchid
          <input value={form.mchid} onChange={upd('mchid')} placeholder="16xxxxxxxx" className="mt-1 w-full text-sm border rounded-lg px-2 py-1.5" />
        </label>
        <label className="text-xs text-os-text-muted">
          证书序列号
          <input value={form.serialNo} onChange={upd('serialNo')} className="mt-1 w-full text-sm border rounded-lg px-2 py-1.5" />
        </label>
        <label className="text-xs text-os-text-muted">
          APIv3 密钥{cfg?.apiV3KeySet ? `（已设置 ${cfg.apiV3KeyMasked}）` : '（必填）'}
          <input type="password" value={form.apiV3Key} onChange={upd('apiV3Key')} placeholder="32 字节密钥" className="mt-1 w-full text-sm border rounded-lg px-2 py-1.5" />
        </label>
        <label className="text-xs text-os-text-muted">
          商户私钥 PEM{cfg?.privateKeySet ? '（已设置）' : '（必填）'}
          <textarea value={form.privateKey} onChange={upd('privateKey')} rows={3} placeholder="-----BEGIN PRIVATE KEY-----" className="mt-1 w-full text-xs border rounded-lg px-2 py-1.5 font-mono" />
        </label>
        <label className="text-xs text-os-text-muted">
          平台公钥 PEM{cfg?.platformPubKeySet ? '（已设置，回调强制验签）' : '（选填）'}
          <textarea value={form.platformPubKey} onChange={upd('platformPubKey')} rows={3} placeholder="-----BEGIN PUBLIC KEY-----" className="mt-1 w-full text-xs border rounded-lg px-2 py-1.5 font-mono" />
        </label>
      </div>
      <Auth perm="site.settings.update" mode="disable">
        <Button variant="primary" size="sm" isDisabled={saving} onPress={() => void save()}>
          {saving ? '保存中…' : '保存配置'}
        </Button>
      </Auth>
    </details>
  )
}

/* ---------------- 订单列表页 ---------------- */
function OrdersPage() {
  const [search, setSearch] = useState('')
  const t = useCmsCollection(ordersApi, ['cms-orders'], { searchFields: ['orderNo', 'memberId', 'status'], serverPaged: true })
  const [confirming, setConfirming] = useState<string | null>(null)

  const confirm = async (row: Order) => {
    if (!window.confirm(`确认已收到订单 ${row.orderNo} 的款项（¥${(row.amountCents / 100).toFixed(2)}）并发货？`)) return
    setConfirming(row.id)
    try {
      const r = await communityApi.confirmOrder(row.id)
      toast.success(`订单 ${r.orderNo} 已确认并发货`)
      if (r.inviteReward) toast.success('已给邀请人补发首次付费奖励积分')
      await t.refetch()
    } catch (e) {
      toast.danger(e instanceof Error ? e.message : '确认失败')
    } finally {
      setConfirming(null)
    }
  }

  const columns: CmsColumn<Order>[] = [
    { id: 'orderNo', header: '订单号', render: (r) => <span className="font-mono text-xs">{r.orderNo}</span> },
    {
      id: 'bizType',
      header: '业务',
      render: (r) => (
        <div className="min-w-0">
          <p className="text-sm">{BIZ_LABEL[r.bizType] || r.bizType}</p>
          <p className="text-xs text-os-text-muted">
            {r.bizType === 'points_recharge' ? `${r.points} 积分` : r.bizType === 'plan' ? `${r.planDays ?? 30} 天` : '—'}
          </p>
        </div>
      ),
    },
    {
      id: 'amountCents',
      header: '金额',
      render: (r) => <span className="tabular-nums">¥{(r.amountCents / 100).toFixed(2)}</span>,
    },
    {
      id: 'channel',
      header: '渠道',
      render: (r) => (
        <span className="px-2 py-0.5 rounded-full text-xs font-medium bg-gray-100 text-gray-600">
          {CHANNEL_LABEL[r.channel] || r.channel}
        </span>
      ),
    },
    {
      id: 'status',
      header: '状态',
      render: (r) => {
        const map: Record<string, { label: string; cls: string }> = {
          pending: { label: '待确认', cls: 'bg-amber-50 text-amber-600' },
          paid: { label: '已到账', cls: 'bg-emerald-50 text-emerald-600' },
          closed: { label: '已关闭', cls: 'bg-gray-100 text-gray-500' },
        }
        const m = map[r.status] || map.closed
        return <span className={`px-2 py-0.5 rounded-full text-xs font-medium ${m.cls}`}>{m.label}</span>
      },
    },
    { id: 'paidAt', header: '到账时间', render: (r) => <span className="text-xs text-os-text-muted">{r.paidAt ? fmtDate(r.paidAt) : '—'}</span> },
    { id: 'createdAt', header: '创建时间', render: (r) => <time className="text-xs text-os-text-muted">{fmtDate(r.createdAt)}</time> },
  ]

  const pendingCount = t.paged.filter((r) => r.status === 'pending').length

  return (
    <div className="p-1 md:p-2">
      <CmsPageHeader
        title="订单"
        desc="收款与发货：人工转账订单核对到账后点「确认收款」；微信扫码订单支付回调自动到账（对账与商户配置见下方）。"
      />
      <CmsToolbar searchPlaceholder="搜索订单号 / 会员 / 状态…" searchValue={search} onSearchChange={setSearch}>
        <span className="text-xs text-os-text-muted">待确认 {pendingCount} 笔</span>
      </CmsToolbar>
      <CmsDataTable
        columns={columns}
        rows={t.paged}
        rowKey={(r) => r.id}
        isLoading={t.isLoading}
        emptyIcon="🧾"
        emptyTitle="还没有订单"
        emptyHint="会员在公开站「会员」页创建充值/订阅订单后，会出现在这里"
        actions={(row) =>
          row.status === 'pending' ? (
            <Auth perm={P.contentMembersUpdate} mode="disable">
              <Button
                variant="primary"
                size="sm"
                isDisabled={confirming === row.id}
                onPress={() => void confirm(row)}
              >
                确认收款
              </Button>
            </Auth>
          ) : null
        }
      />
      <FulfillCard />
      <ReconCard />
      <PayConfigCard />
    </div>
  )
}

export const Route = createFileRoute('/content/orders')({ component: OrdersPage })
