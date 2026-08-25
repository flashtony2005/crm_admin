import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Button, toast } from '@heroui/react'

import { integrationsApi, type Integration } from '../../api/cms'
import { useState } from 'react'
import { CmsPageHeader } from '../../components/cms/CmsPageHeader'
import { CMS_MODE } from '../../api/cms'
import { api } from '../../api/client'
import { Auth } from '../../components/cms/Auth'
import { P } from '../../config/permissions'
import { createFileRoute } from '@tanstack/react-router'

/** 集成分类 → 中文标签与图标 */
const CATEGORY_META: Record<Integration['category'], { label: string; icon: string }> = {
  seo: { label: 'SEO', icon: '🔍' },
  analytics: { label: '分析', icon: '📊' },
  message: { label: '消息', icon: '✉️' },
  commerce: { label: '电商', icon: '🛒' },
  crm: { label: 'CRM', icon: '🗂️' },
}

const CATEGORY_ORDER = Object.keys(CATEGORY_META) as Integration['category'][]

function IntegrationsPage() {
  const qc = useQueryClient()
  const [keyTarget, setKeyTarget] = useState<Integration | null>(null)
  const [oauthTarget, setOauthTarget] = useState<Integration | null>(null)
  const { data: list = [], isLoading } = useQuery({
    queryKey: ['cms-integrations'],
    queryFn: () => integrationsApi.list(),
  })

  // 连接 / 断开
  const toggle = useMutation({
    mutationFn: ({ item, apiKey }: { item: Integration; apiKey?: string }) => {
      if (item.connected) {
        // 断开：清 key
        return integrationsApi.update(item.id, { connected: false, apiKey: '' } as Partial<Integration>)
      }
      return integrationsApi.update(item.id, { connected: true, ...(apiKey ? { apiKey } : {}) } as Partial<Integration>)
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: ['cms-integrations'] }),
  })

  if (isLoading) {
    return (
      <div className="p-1 md:p-2">
        <CmsPageHeader title="Integrations" desc="把网站和常用工具连起来，数据自动同步。" />
        <p className="text-sm text-os-text-muted py-16 text-center">加载中…</p>
      </div>
    )
  }

  const connectedCount = list.filter((i) => i.connected).length

  return (
    <div className="p-1 md:p-2 flex flex-col gap-5">
      <div>
        <CmsPageHeader title="Integrations" desc="把网站和常用工具连起来，数据自动同步。" />
        <p className="text-xs text-os-text-muted -mt-2 mb-1">
          已连接 <b className="text-[#6366F1]">{connectedCount}</b> / {list.length} 个应用
        </p>
      </div>

      {CATEGORY_ORDER.map((cat) => {
        const items = list.filter((i) => i.category === cat)
        if (items.length === 0) return null
        return (
          <section key={cat}>
            <h2 className="text-sm font-medium text-os-text-secondary mb-2.5">
              {CATEGORY_META[cat].icon} {CATEGORY_META[cat].label}
            </h2>
            <div className="grid md:grid-cols-2 xl:grid-cols-3 gap-3">
              {items.map((i) => (
                <IntegrationCard
                  key={i.key} item={i} busy={toggle.isPending}
                  onToggle={(apiKey) => {
                    if (CMS_MODE === 'real' && !i.connected) {
                      // OAuth 型集成走授权码流；否则 Key 弹窗
                      if (i.oauthProvider) {
                        setOauthTarget(i)
                      } else if (!apiKey) {
                        setKeyTarget(i)
                      } else {
                        toggle.mutate({ item: i, apiKey })
                      }
                      return
                    }
                    toggle.mutate({ item: i, apiKey })
                  }}
                />
              ))}
            </div>
          </section>
        )
      })}

      {/* OAuth 授权码流（B6）：填 client id/secret → 开授权页 → 轮询完成 */}
      {oauthTarget && (
        <OAuthModal
          target={oauthTarget}
          qc={qc}
          onDone={() => setOauthTarget(null)}
        />
      )}

      {/* 连接弹窗：real 模式收集 API Key（Phase 4 连接流） */}
      {keyTarget && (
        <KeyModal
          target={keyTarget}
          onCancel={() => setKeyTarget(null)}
          onConfirm={(apiKey) => {
            const t = keyTarget
            setKeyTarget(null)
            toggle.mutate({ item: t, apiKey })
          }}
        />
      )}
    </div>
  )
}

function IntegrationCard({
  item,
  onToggle,
  busy,
}: {
  item: Integration
  onToggle: (apiKey?: string) => void
  busy: boolean
}) {
  return (
    <div className="rounded-xl border border-os-border bg-white p-4 shadow-sm hover:shadow-md transition-shadow flex flex-col">
      <div className="flex items-start justify-between mb-2">
        <span className="w-9 h-9 rounded-lg bg-gradient-to-br from-gray-50 to-gray-100 border border-os-border-light flex items-center justify-center text-lg">
          {item.name[0]}
        </span>
        {item.connected && (
          <span className="text-xs px-2 py-0.5 rounded-md bg-green-50 text-green-600 font-medium">已连接</span>
        )}
      </div>
      <h3 className="text-sm font-medium text-os-text-primary">{item.name}</h3>
      <p className="text-xs text-os-text-muted mt-1 flex-1">{item.desc}</p>
      <Auth perm={P.automationIntegrationsToggle}>
        <Button
          size="sm"
          variant={item.connected ? 'ghost' : 'primary'}
          isDisabled={busy}
          onPress={() => onToggle()}
          className={`mt-3 ${item.connected ? 'text-os-danger-text hover:bg-os-danger-bg self-start' : 'self-start'}`}
        >
          {item.connected ? '断开连接' : '连接'}
        </Button>
      </Auth>
    </div>
  )
}

/** 连接确认：输入 API Key（Phase 4 最小连接流；真实 OAuth 按应用接入） */
function KeyModal({
  target, onCancel, onConfirm,
}: {
  target: Integration
  onCancel: () => void
  onConfirm: (apiKey: string) => void
}) {
  const [apiKey, setApiKey] = useState('')
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 px-4" role="dialog" aria-modal>
      <div className="rounded-2xl bg-white shadow-xl w-full max-w-sm p-5">
        <h3 className="text-base font-semibold text-os-text-primary">连接 {target.name}</h3>
        <p className="text-xs text-os-text-muted mt-1">粘贴该应用的 API Key，仅保存在你的企业空间。</p>
        <input
          autoFocus
          value={apiKey}
          onChange={(e) => setApiKey(e.target.value)}
          placeholder="API Key"
          className="w-full h-10 mt-3 px-3 text-sm rounded-lg border border-os-border outline-none focus:border-[#6366F1] font-mono"
        />
        <div className="flex justify-end gap-2 mt-4">
          <Button size="sm" variant="ghost" onPress={onCancel}>取消</Button>
          <Button size="sm" variant="primary" isDisabled={!apiKey.trim()} onPress={() => onConfirm(apiKey.trim())}>
            连接
          </Button>
        </div>
      </div>
    </div>
  )
}


/** OAuth2 授权码连接（B6） */
function OAuthModal({
  target, qc, onDone,
}: {
  target: Integration
  qc: ReturnType<typeof useQueryClient>
  onDone: () => void
}) {
  const [clientId, setClientId] = useState('')
  const [clientSecret, setClientSecret] = useState('')
  const [busy, setBusy] = useState(false)
  const [phase, setPhase] = useState<'form' | 'waiting' | 'done' | 'error'>('form')
  const [err, setErr] = useState<string | null>(null)

  const start = async () => {
    if (!clientId.trim() || !clientSecret.trim()) return
    setBusy(true)
    try {
      // 1) 存凭据
      await api(`/api/integrations/${target.id}`, {
        method: 'PUT',
        body: JSON.stringify({ oauthClientId: clientId.trim(), oauthClientSecret: clientSecret.trim() }),
      })
      // 2) 拿授权 URL 并开窗
      const body = await api<{ authorizeUrl: string }>(`/api/integrations/${target.id}/oauth/start`)
      window.open(body.authorizeUrl, '_blank', 'noopener')
      setPhase('waiting')
      // 3) 轮询授权完成
      for (let i = 0; i < 40; i++) {
        await new Promise((r) => setTimeout(r, 2000))
        const st = await api<{ connected: boolean }>(`/api/integrations/${target.id}/oauth/status`)
        if (st.connected) {
          setPhase('done')
          toast.success(`${target.name} 已连接`)
          void qc.invalidateQueries({ queryKey: ['cms-integrations'] })
          setTimeout(onDone, 800)
          return
        }
      }
      setPhase('error')
      setErr('等待授权超时，请重试')
    } catch (e) {
      setPhase('error')
      setErr(e instanceof Error ? e.message : '启动授权失败')
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 px-4" role="dialog" aria-modal>
      <div className="rounded-2xl bg-white shadow-xl w-full max-w-sm p-5">
        <h3 className="text-base font-semibold text-os-text-primary">连接 {target.name}</h3>
        <p className="text-xs text-os-text-muted mt-1">
          OAuth2 授权码连接：填写你在{target.oauthProvider === 'google' ? ' Google Cloud Console' : target.oauthProvider === 'github' ? ' GitHub OAuth Apps' : ' 授权服务'}创建的客户端凭据。
        </p>
        {phase === 'form' && (
          <div className="flex flex-col gap-3 mt-3">
            <input value={clientId} onChange={(e) => setClientId(e.target.value)} placeholder="Client ID"
              className="h-10 px-3 text-sm rounded-lg border border-os-border outline-none focus:border-[#6366F1] font-mono" />
            <input value={clientSecret} onChange={(e) => setClientSecret(e.target.value)} placeholder="Client Secret" type="password"
              className="h-10 px-3 text-sm rounded-lg border border-os-border outline-none focus:border-[#6366F1] font-mono" />
            {err && <p className="text-xs text-red-500">{err}</p>}
            <div className="flex justify-end gap-2 mt-1">
              <Button size="sm" variant="ghost" onPress={onDone}>取消</Button>
              <Button size="sm" variant="primary" isDisabled={!clientId.trim() || !clientSecret.trim() || busy} onPress={() => void start()}>
                跳转授权
              </Button>
            </div>
          </div>
        )}
        {phase === 'waiting' && (
          <div className="py-6 text-center">
            <div className="text-3xl mb-2 animate-pulse">🔗</div>
            <p className="text-sm text-os-text-primary">已在新窗口打开授权页</p>
            <p className="text-xs text-os-text-muted mt-1">完成授权后本窗口会自动刷新（最长等待 80 秒）</p>
          </div>
        )}
        {phase === 'done' && <p className="py-6 text-center text-sm text-green-600">✅ 授权成功，已连接</p>}
        {phase === 'error' && (
          <div className="py-4 text-center">
            <p className="text-sm text-red-500">{err}</p>
            <Button size="sm" variant="ghost" className="mt-2" onPress={() => setPhase('form')}>重试</Button>
          </div>
        )}
      </div>
    </div>
  )
}

export const Route = createFileRoute('/automation/integrations')({
  component: IntegrationsPage,
})
