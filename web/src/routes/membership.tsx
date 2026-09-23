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
  /** 计费周期：月付 / 年付。**同时决定计价与会员天数**，必须显式选择 */
  const [cycle, setCycle] = useState<'monthly' | 'yearly'>('monthly')

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
      const r = await subscriptionsApi.checkout(tierId, cycle)
      if (r.url) window.location.href = r.url
    } catch (e: any) {
      // 必须用 toast：`err` 的内联错误位只在「未登录」表单里渲染，
      // 会员态下页面没有任何地方显示它 —— 点「在线支付订阅」失败会毫无反应，
      // 用户只会反复点，而不知道是通道没配还是网络问题。
      toast.danger(e?.message || '发起订阅失败')
    }
  }

  /**
   * 线下付款 → 站长后台确认开通。
   *
   * 为什么必须有这条路：在线支付通道（Stripe / 微信）**未配置时是硬拒绝**的，
   * 只有 `checkout` 一条路意味着站长没配通道时**任何会员都无法开通** ——
   * 内容→会员→收入的链路整条断在这里。人工确认订单走的仍是同一张 orders 表
   * 与同一套发货逻辑（后台订单页点确认），不引入第二条业务路径。
   */
  const manualSubscribe = async (tierId: string) => {
    if (!getMemberToken()) {
      setErr('请先登录或注册会员')
      setMode('login')
      return
    }
    setWalletBusy(true)
    try {
      const r = await communityApi.createOrder('plan', { tierId, interval: cycle, channel: 'manual' })
      setWalletHint(
        `订阅订单已创建：${r.orderNo}\n应付 ¥${(r.amountCents / 100).toFixed(2)}（${cycle === 'yearly' ? '年付' : '月付'}）\n${r.hint}`,
      )
      loadWallet()
    } catch (e) {
      toast.danger(e instanceof Error ? e.message : '创建订阅订单失败')
    } finally {
      setWalletBusy(false)
    }
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
              <div className="flex flex-wrap items-center gap-3 rounded-lg bg-os-bg-base p-3">
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

      <div className="mt-10 mb-4 flex flex-wrap items-center gap-3">
        <h2 className="text-lg font-semibold">套餐</h2>
        {/* 周期必须显式选：它同时决定计价与会员天数，不能靠默认值糊过去 */}
        <div className="flex rounded-lg border p-0.5">
          {(['monthly', 'yearly'] as const).map((c) => (
            <button
              key={c}
              onClick={() => setCycle(c)}
              className={`px-3 py-1 text-sm rounded-md transition-colors ${
                cycle === c ? 'bg-os-text-primary text-white' : 'text-os-text-muted hover:bg-os-bg-hover'
              }`}
            >
              {c === 'monthly' ? '月付' : '年付'}
            </button>
          ))}
        </div>
      </div>
      <div className="grid sm:grid-cols-2 gap-4">
        {tiers.map((tier) => {
          const price = cycle === 'yearly' ? tier.priceYearly : tier.priceMonthly
          const priced = price > 0
          /**
           * 该周期是否配了在线支付价格（服务端 /api/public/tiers 下发）。
           * `undefined` = 服务端未下发（旧版本）→ 不拦：在线通道能不能用由服务端
           * 守卫说了算，前端多拦一刀会把本来可用的通道一起禁掉。
           * 只有服务端**明确说"这个周期没配"**（false）时才禁用按钮并给出原因。
           */
          const onlineOk = (cycle === 'yearly' ? tier.onlineYearly : tier.onlineMonthly) !== false
          return (
            <div key={tier.id} className="rounded-xl border bg-white p-5 flex flex-col">
              <h3 className="font-semibold">{tier.name}</h3>
              <p className="text-sm text-os-text-muted mt-1">
                {priced ? `¥${price} / ${cycle === 'yearly' ? '年' : '月'}` : '该周期未定价'}
              </p>
              <p className="text-sm mt-2 flex-1">{tier.description}</p>
              <div className="mt-4 flex flex-wrap gap-2">
                {/* 月/年各有一个 Stripe Price（tiers.stripe_price_id /
                    stripe_price_yearly_id），服务端按所选周期取价并据此发货天数 ——
                    年付与月付同等对待，不再按周期禁用在线通道。 */}
                <Button
                  size="sm"
                  variant="primary"
                  isDisabled={!priced || !onlineOk}
                  onPress={() => void checkout(tier.id)}
                >
                  在线支付订阅
                </Button>
                {/* 通道未配置时的可用路径：线下付款 → 站长后台确认发货 */}
                <Button size="sm" variant="ghost" isDisabled={!priced} onPress={() => void manualSubscribe(tier.id)}>
                  线下付款 · 人工开通
                </Button>
              </div>
              {priced && !onlineOk && (
                <p className="mt-2 text-xs text-os-text-muted">
                  该周期未配置在线支付价格（后台「付费订阅」的{' '}
                  {cycle === 'yearly' ? '年付 Stripe Price ID' : '月付 Stripe Price ID'}），
                  可用「线下付款 · 人工开通」。
                </p>
              )}
            </div>
          )
        })}
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
