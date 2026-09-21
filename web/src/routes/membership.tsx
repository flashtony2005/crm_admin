import { createFileRoute } from '@tanstack/react-router'
import { useEffect, useState } from 'react'
import { Button, Input, Label, toast } from '@heroui/react'
import {
  communityApi, memberAuth, getMemberToken, subscriptionsApi,
  type MemberProfile, type Tier, type WalletInfo,
} from '../api/cms'
import { useTranslation } from 'react-i18next'

const LEDGER_LABEL: Record<string, string> = {
  signin: '每日签到',
  redeem: '兑码充值',
  recharge: '充值到账',
  purchase: '积分解锁文章',
  invite_reward: '邀请奖励',
  adjust: '人工调整',
}

function MembershipPage() {
  const { t } = useTranslation()
  const [me, setMe] = useState<MemberProfile | null>(null)
  const [tiers, setTiers] = useState<Tier[]>([])
  const [mode, setMode] = useState<'login' | 'register'>('login')
  const [email, setEmail] = useState('')
  const [name, setName] = useState('')
  const [password, setPassword] = useState('')
  const [inviteCode, setInviteCode] = useState('')
  /** 邀请制开关（后台 site_settings.member_invite_required；读不到=关闭） */
  const [inviteRequired, setInviteRequired] = useState(false)
  const [busy, setBusy] = useState(false)
  const [err, setErr] = useState('')
  // 钱包
  const [wallet, setWallet] = useState<WalletInfo | null>(null)
  const [redeemCode, setRedeemCode] = useState('')
  const [rechargePoints, setRechargePoints] = useState('')
  const [signing, setSigning] = useState(false)
  const [walletBusy, setWalletBusy] = useState(false)
  const [walletHint, setWalletHint] = useState('')
  /** 微信扫码支付弹层（P2）：qrSvg + orderNo；到账后自动关闭并刷新 */
  const [payQr, setPayQr] = useState<{ qrSvg: string; orderNo: string; amountCents: number } | null>(null)

  const loadWallet = () => {
    if (!getMemberToken()) return
    communityApi.wallet().then(setWallet).catch(() => {})
  }

  useEffect(() => {
    if (getMemberToken()) memberAuth.me().then(setMe)
    subscriptionsApi.tiers().then(setTiers).catch(() => {})
    fetch('/api/public/site')
      .then((r) => r.json())
      .then((j) => setInviteRequired(j?.data?.memberInviteRequired === 'on'))
      .catch(() => {})
  }, [])

  useEffect(() => {
    if (me) loadWallet()
  }, [me?.id])

  const doSignin = async () => {
    setSigning(true)
    try {
      const r = await communityApi.signin()
      toast.success(`签到成功 +${r.delta} 积分`)
      loadWallet()
    } catch (e) {
      toast.danger(e instanceof Error ? e.message : '签到失败')
    } finally {
      setSigning(false)
    }
  }

  const doRedeem = async () => {
    setWalletBusy(true)
    try {
      const r = await communityApi.redeem(redeemCode.trim())
      toast.success(r.kind === 'points' ? `兑换成功 +${r.delta} 积分` : `兑换成功，会员延长 ${r.days} 天`)
      setRedeemCode('')
      loadWallet()
      memberAuth.me().then(setMe)
    } catch (e) {
      toast.danger(e instanceof Error ? e.message : '兑换失败')
    } finally {
      setWalletBusy(false)
    }
  }

  /** 微信扫码支付：弹二维码并轮询订单状态，到账后自动刷新钱包 */
  const pollWechatOrder = (orderNo: string) => {
    const started = Date.now()
    const h = window.setInterval(async () => {
      if (Date.now() - started > 10 * 60 * 1000) {
        window.clearInterval(h)
        return
      }
      try {
        const r = await communityApi.orderStatus(orderNo)
        if (r.status === 'paid') {
          window.clearInterval(h)
          setPayQr(null)
          toast.success('支付成功，积分已到账')
          loadWallet()
          memberAuth.me().then(setMe)
        }
      } catch {
        /* 轮询失败静默重试 */
      }
    }, 3000)
  }

  const doRecharge = async (channel: 'manual' | 'wechat') => {
    setWalletBusy(true)
    try {
      const r = await communityApi.createOrder('points_recharge', { points: Number(rechargePoints), channel })
      if (channel === 'wechat' && r.qrSvg) {
        setPayQr({ qrSvg: r.qrSvg, orderNo: r.orderNo, amountCents: r.amountCents })
        pollWechatOrder(r.orderNo)
      } else {
        setWalletHint(`订单已创建：${r.orderNo}\n${r.hint}`)
      }
      setRechargePoints('')
    } catch (e) {
      toast.danger(e instanceof Error ? e.message : '创建订单失败')
    } finally {
      setWalletBusy(false)
    }
  }

  const submit = async () => {
    setErr(''); setBusy(true)
    try {
      const m = mode === 'login'
        ? await memberAuth.login(email, password)
        : await memberAuth.register(email, name, password, inviteCode.trim() || undefined)
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
        <div className="mt-6 rounded-xl border bg-white p-5 space-y-4">
          <div className="flex items-center justify-between">
            <div>
              <p className="font-medium">{me.name || me.email}</p>
              <p className="text-sm text-os-text-muted">
                当前套餐：<b>{me.plan}</b>
                {wallet?.planExpiresAt ? ` · 有效期至 ${wallet.planExpiresAt.slice(0, 10)}` : ''}
              </p>
            </div>
            <div className="flex items-center gap-2">
              <a
                href="/t/member"
                target="_blank"
                rel="noreferrer"
                className="text-sm text-primary hover:underline"
              >
                会员中心（站点主题）↗
              </a>
              <Button variant="ghost" size="sm" onPress={() => { memberAuth.logout(); setMe(null) }}>退出</Button>
            </div>
          </div>

          {wallet?.planExpired && (
            <div className="rounded-lg border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-600">
              您的会员已于 {wallet.planExpiresAt.slice(0, 10)} 过期，续费后可继续享受会员权益。
            </div>
          )}
          {wallet?.planExpiring && !wallet.planExpired && (
            <div className="rounded-lg border border-amber-200 bg-amber-50 px-3 py-2 text-sm text-amber-600">
              您的会员将于 {wallet.planExpiresAt.slice(0, 10)} 到期（还剩 {wallet.planDaysLeft} 天），可提前续费避免中断。
            </div>
          )}

          {wallet && (
            <>
              <div className="flex flex-wrap items-center gap-3 rounded-lg bg-os-surface p-3">
                <div className="mr-auto">
                  <p className="text-xs text-os-text-muted">积分余额</p>
                  <p className="text-xl font-bold tabular-nums">{wallet.balance}</p>
                </div>
                <Button
                  variant="primary"
                  size="sm"
                  isDisabled={wallet.signedToday || signing}
                  onPress={() => void doSignin()}
                >
                  {wallet.signedToday ? '今日已签到' : `签到 +${wallet.signinPoints}`}
                </Button>
              </div>

              <div className="flex flex-wrap items-center gap-2">
                <Input value={redeemCode} onChange={(e) => setRedeemCode(e.target.value)} placeholder="兑换码（积分 / 会员天数）" className="max-w-[240px]" />
                <Button variant="ghost" size="sm" isDisabled={walletBusy || !redeemCode.trim()} onPress={() => void doRedeem()}>
                  兑换
                </Button>
                <Input
                  value={rechargePoints}
                  onChange={(e) => setRechargePoints(e.target.value.replace(/\D/g, ''))}
                  placeholder="充值积分"
                  className="max-w-[120px]"
                />
                <Button
                  variant="ghost"
                  size="sm"
                  isDisabled={walletBusy || !rechargePoints}
                  onPress={() => void doRecharge('manual')}
                >
                  创建充值订单（人工确认）
                </Button>
                <Button
                  variant="primary"
                  size="sm"
                  isDisabled={walletBusy || !rechargePoints}
                  onPress={() => void doRecharge('wechat')}
                >
                  微信扫码支付
                </Button>
              </div>
              {walletHint && <p className="text-xs text-os-text-muted whitespace-pre-line">{walletHint}</p>}

              {wallet.ledger.length > 0 && (
                <div className="max-h-56 overflow-auto rounded-lg border border-os-border">
                  <table className="w-full text-xs">
                    <tbody>
                      {wallet.ledger.map((l, i) => (
                        <tr key={i} className="border-b border-os-border last:border-0">
                          <td className="px-3 py-1.5">{LEDGER_LABEL[l.reason] || l.reason}</td>
                          <td className={`px-3 py-1.5 tabular-nums text-right ${l.delta >= 0 ? 'text-emerald-600' : 'text-red-500'}`}>
                            {l.delta >= 0 ? `+${l.delta}` : l.delta}
                          </td>
                          <td className="px-3 py-1.5 tabular-nums text-right text-os-text-muted">{l.balanceAfter}</td>
                          <td className="px-3 py-1.5 text-right text-os-text-muted whitespace-nowrap">{l.createdAt.slice(0, 10)}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </>
          )}
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
          {mode === 'register' && inviteRequired && (
            <div className="space-y-1.5">
              <Label>邀请码（本站为邀请制，必填）</Label>
              <Input value={inviteCode} onChange={(e) => setInviteCode(e.target.value)} placeholder="请向邀请人索取" />
            </div>
          )}
          {mode === 'register' && !inviteRequired && (
            <div className="space-y-1.5">
              <Label>邀请码（选填）</Label>
              <Input value={inviteCode} onChange={(e) => setInviteCode(e.target.value)} placeholder="有邀请码可填，无则留空" />
            </div>
          )}
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

      {payQr && (
        <div className="fixed inset-0 z-50 grid place-items-center bg-black/50 px-4" onClick={() => setPayQr(null)}>
          <div className="rounded-xl bg-white p-5 max-w-sm w-full" onClick={(e) => e.stopPropagation()}>
            <h3 className="text-base font-semibold mb-1">微信扫码支付</h3>
            <p className="text-sm text-os-text-muted mb-3">
              金额 <b className="tabular-nums">¥{(payQr.amountCents / 100).toFixed(2)}</b> · 订单号{' '}
              <span className="font-mono text-xs">{payQr.orderNo}</span>
            </p>
            <div className="grid place-items-center py-2" style={{ filter: 'drop-shadow(0 2px 8px rgba(0,0,0,.15))' }}>
              <div className="w-56 h-56 [&>svg]:w-full [&>svg]:h-full" dangerouslySetInnerHTML={{ __html: payQr.qrSvg }} />
            </div>
            <p className="text-xs text-os-text-muted text-center mt-2">支付成功后自动到账并刷新余额</p>
            <div className="grid place-items-center mt-3">
              <Button variant="ghost" size="sm" onPress={() => setPayQr(null)}>关闭</Button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}

export const Route = createFileRoute('/membership')({ component: MembershipPage })
