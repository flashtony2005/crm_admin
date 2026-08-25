import { useState } from 'react'
import { createFileRoute, useNavigate, redirect } from '@tanstack/react-router'
import { api } from '../api/client'
import { useAuthStore } from '../store/auth'

/**
 * /change-password —— 首登/密码重置后的强制修改页。
 * 服务端 users.must_change_password=1 时，RootLayout 会把所有页面重定向到这里。
 */
function ChangePasswordPage() {
  const nav = useNavigate()
  const { user } = useAuthStore()
  const [oldPw, setOldPw] = useState('')
  const [newPw, setNewPw] = useState('')
  const [confirmPw, setConfirmPw] = useState('')
  const [err, setErr] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const submit = async () => {
    setErr(null)
    if (newPw.length < 8) return setErr('新密码至少 8 位')
    if (newPw !== confirmPw) return setErr('两次输入的新密码不一致')
    if (newPw === oldPw) return setErr('新密码不能与旧密码相同')
    setBusy(true)
    try {
      await api('/api/me/password', {
        method: 'POST',
        body: JSON.stringify({ old_password: oldPw, new_password: newPw }),
      })
      // 本地同步清除标记（服务端已置 0）
      useAuthStore.setState({
        user: user ? { ...user, mustChangePassword: false } : user,
      })
      nav({ to: '/home' })
    } catch (e) {
      setErr(e instanceof Error ? e.message : '修改失败')
    } finally {
      setBusy(false)
    }
  }

  // 未登录（token 缺失）时本页不可用 —— RootLayout 已拦截，这里兜底
  const hasToken = !!useAuthStore.getState().token

  return (
    <div className="min-h-screen flex items-center justify-center bg-os-bg px-4">
      <div className="rounded-2xl bg-white shadow-xl border border-os-border w-full max-w-sm p-6">
        <h1 className="text-lg font-semibold text-os-text-primary">设置新密码</h1>
        <p className="text-xs text-os-text-muted mt-1">
          {user ? `${user.nickname}，` : ''}为了账号安全，首次登录请先修改初始密码。
        </p>
        {!hasToken && (
          <p className="text-xs text-red-500 mt-2">登录状态缺失，请返回重新登录。</p>
        )}
        <div className="flex flex-col gap-3 mt-4">
          <input
            type="password" value={oldPw} onChange={(e) => setOldPw(e.target.value)}
            placeholder="当前密码（初始 demo1234）"
            className="h-10 px-3 text-sm rounded-lg border border-os-border outline-none focus:border-[#6366F1]"
          />
          <input
            type="password" value={newPw} onChange={(e) => setNewPw(e.target.value)}
            placeholder="新密码（至少 8 位）"
            className="h-10 px-3 text-sm rounded-lg border border-os-border outline-none focus:border-[#6366F1]"
          />
          <input
            type="password" value={confirmPw} onChange={(e) => setConfirmPw(e.target.value)}
            placeholder="确认新密码"
            className="h-10 px-3 text-sm rounded-lg border border-os-border outline-none focus:border-[#6366F1]"
          />
          {err && <p className="text-xs text-red-500">{err}</p>}
          <button
            onClick={() => void submit()}
            disabled={!oldPw || !newPw || !confirmPw || busy}
            className="h-10 rounded-lg bg-[#4F46E5] text-white text-sm font-medium disabled:opacity-40 hover:bg-[#4338CA] transition-colors"
          >
            {busy ? '提交中…' : '确认修改'}
          </button>
        </div>
      </div>
    </div>
  )
}

export const Route = createFileRoute('/change-password')({
  beforeLoad: () => {
    // 无需改密却直接访问 → 回工作台
    const raw = localStorage.getItem('auth-store')
    try {
      const u = JSON.parse(raw ?? '{}')?.state?.user
      if (!u?.mustChangePassword) throw redirect({ to: '/home' })
    } catch (e) {
      if (e instanceof Response) throw e
      throw redirect({ to: '/login' })
    }
  },
  component: ChangePasswordPage,
})
