import { createFileRoute } from '@tanstack/react-router'
import { useEffect, useState } from 'react'
import { Button, Input, Label } from '@heroui/react'
import {
  memberAuth, getMemberToken, subscriptionsApi, type MemberProfile, type Tier,
} from '../api/cms'
import { useTranslation } from 'react-i18next'

function MembershipPage() {
  const { t } = useTranslation()
  const [me, setMe] = useState<MemberProfile | null>(null)
  const [tiers, setTiers] = useState<Tier[]>([])
  const [mode, setMode] = useState<'login' | 'register'>('login')
  const [email, setEmail] = useState('')
  const [name, setName] = useState('')
  const [password, setPassword] = useState('')
  const [busy, setBusy] = useState(false)
  const [err, setErr] = useState('')

  useEffect(() => {
    if (getMemberToken()) memberAuth.me().then(setMe)
    subscriptionsApi.tiers().then(setTiers).catch(() => {})
  }, [])

  const submit = async () => {
    setErr(''); setBusy(true)
    try {
      const m = mode === 'login'
        ? await memberAuth.login(email, password)
        : await memberAuth.register(email, name, password)
      setMe(m)
    } catch (e: any) {
      setErr(e?.message || '操作失败')
    } finally { setBusy(false) }
  }

  const checkout = async (tierId: string) => {
    if (!getMemberToken()) {
      setErr('请先登录或注册会员')
      setMode('login')
      return
    }
    try {
      const r = await subscriptionsApi.checkout(tierId)
      if (r.url) window.location.href = r.url
    } catch (e: any) { setErr(e?.message || '发起订阅失败') }
  }

  return (
    <div className="max-w-3xl mx-auto px-4 py-10">
      <h1 className="text-2xl font-bold">{t('membership.title')}</h1>
      <p className="text-os-text-secondary mt-1">{t('membership.subtitle')}</p>

      {me ? (
        <div className="mt-6 rounded-xl border bg-white p-5">
          <div className="flex items-center justify-between">
            <div>
              <p className="font-medium">{me.name || me.email}</p>
              <p className="text-sm text-os-text-muted">当前套餐：<b>{me.plan}</b></p>
            </div>
            <Button variant="ghost" size="sm" onPress={() => { memberAuth.logout(); setMe(null) }}>退出</Button>
          </div>
        </div>
      ) : (
        <div className="mt-6 rounded-xl border bg-white p-5 space-y-3 max-w-md">
          <div className="flex gap-2">
            <Button variant={mode === 'login' ? 'primary' : 'ghost'} size="sm" onPress={() => setMode('login')}>登录</Button>
            <Button variant={mode === 'register' ? 'primary' : 'ghost'} size="sm" onPress={() => setMode('register')}>注册</Button>
          </div>
          {mode === 'register' && (
            <div className="space-y-1.5"><Label>昵称</Label><Input value={name} onChange={(e) => setName(e.target.value)} /></div>
          )}
          <div className="space-y-1.5"><Label>邮箱</Label><Input value={email} onChange={(e) => setEmail(e.target.value)} /></div>
          <div className="space-y-1.5"><Label>密码</Label><Input type="password" value={password} onChange={(e) => setPassword(e.target.value)} /></div>
          {err && <p className="text-sm text-red-500">{err}</p>}
          <Button variant="primary" isDisabled={busy} onPress={() => void submit()}>
            {mode === 'login' ? '登录' : '注册'}
          </Button>
        </div>
      )}

      <h2 className="text-lg font-semibold mt-10 mb-4">套餐</h2>
      <div className="grid sm:grid-cols-2 gap-4">
        {tiers.map((tier) => (
          <div key={tier.id} className="rounded-xl border bg-white p-5">
            <h3 className="font-semibold">{tier.name}</h3>
            <p className="text-sm text-os-text-muted mt-1">¥{tier.priceMonthly}/月 · ¥{tier.priceYearly}/年</p>
            <p className="text-sm mt-2">{tier.description}</p>
            <Button className="mt-4" size="sm" variant="primary" onPress={() => void checkout(tier.id)}>订阅</Button>
          </div>
        ))}
        {tiers.length === 0 && <p className="text-os-text-muted">暂未上架套餐。</p>}
      </div>
    </div>
  )
}

export const Route = createFileRoute('/membership')({ component: MembershipPage })
