import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { useState, useEffect, useRef, useCallback } from 'react'
import { useTranslation } from 'react-i18next'
import { Moon, Sun } from 'lucide-react'
import { useAuthStore } from '../store/auth'

import { extractError } from '../lib/error'

// ── SVG 图标（内联，非 lucide） ──────────────────────────

const UserIcon = () => (
  <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="#7E858F" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
    <path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" />
    <circle cx="12" cy="7" r="4" />
  </svg>
)

const LockIcon = () => (
  <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="#7E858F" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
    <rect x="3" y="11" width="18" height="11" rx="2" ry="2" />
    <path d="M7 11V7a5 5 0 0 1 10 0v4" />
  </svg>
)

const EyeIcon = ({ open, color }: { open: boolean; color?: string }) => (
  open ? (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke={color || "#7E858F"} strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z" />
      <circle cx="12" cy="12" r="3" />
    </svg>
  ) : (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke={color || "#7E858F"} strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94" />
      <path d="M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19" />
      <line x1="1" y1="1" x2="23" y2="23" />
    </svg>
  )
)

const CheckIcon = () => (
  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="white" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
    <polyline points="20 6 9 17 4 12" />
  </svg>
)

// ── 拖动验证组件 ─────────────────────────────────────

function DragVerify({
  onSuccess, onReset, text, successText,
}: {
  onSuccess: () => void
  onReset?: () => void
  text?: string
  successText?: string
}) {
  const [pos, setPos] = useState(0)
  const [dragging, setDragging] = useState(false)
  const [success, setSuccess] = useState(false)
  const trackRef = useRef<HTMLDivElement>(null)
  const width = 320
  const height = 44
  const handleSize = 48

  const maxPos = width - handleSize

  const doSuccess = useCallback(() => {
    setDragging(false)
    setSuccess(true)
    setPos(maxPos)
    onSuccess()
  }, [maxPos, onSuccess])

  const handleMove = useCallback((cx: number) => {
    if (!dragging || success || !trackRef.current) return
    const rect = trackRef.current.getBoundingClientRect()
    let p = cx - rect.left - handleSize / 2
    p = Math.max(0, Math.min(p, maxPos))
    setPos(p)
    if (p >= maxPos - 3) doSuccess()
  }, [dragging, success, maxPos, doSuccess])

  const handleEnd = useCallback(() => {
    if (!success) setPos(0)
    setDragging(false)
    onReset?.()
  }, [success, onReset])

  const onMouseDown = () => { if (!success) setDragging(true) }

  useEffect(() => {
    if (!dragging) return
    const mm = (e: MouseEvent) => handleMove(e.clientX)
    const mu = () => handleEnd()
    window.addEventListener('mousemove', mm)
    window.addEventListener('mouseup', mu)
    return () => { window.removeEventListener('mousemove', mm); window.removeEventListener('mouseup', mu) }
  }, [dragging, handleMove, handleEnd])

  const onTouchStart = () => { if (!success) setDragging(true) }
  useEffect(() => {
    if (!dragging) return
    const tm = (e: TouchEvent) => { e.preventDefault(); handleMove(e.touches[0].clientX) }
    const te = () => handleEnd()
    window.addEventListener('touchmove', tm, { passive: false })
    window.addEventListener('touchend', te)
    return () => { window.removeEventListener('touchmove', tm); window.removeEventListener('touchend', te) }
  }, [dragging, handleMove, handleEnd])

  const pct = maxPos > 0 ? (pos / maxPos) * 100 : 0
  const trackBg = success ? '#1E8A5A' : '#F4F6F8'

  return (
    <div className="relative select-none mx-auto" style={{ width, height }}>
      <div ref={trackRef} className="relative w-full h-full rounded-full overflow-hidden" style={{ background: trackBg }}>
        {/* 进度 */}
        <div className="absolute inset-y-0 left-0 rounded-full transition-all duration-300" style={{
          width: success ? '100%' : `${pct}%`,
          background: success ? '#1E8A5A' : '#3C6FE3',
        }} />
        {/* 文字 */}
        {!success && !pos && (
          <div className="absolute inset-0 flex items-center justify-center pointer-events-none">
            <span className="text-sm text-gray-400 select-none">{text || '请按住滑块拖动到最右边'}</span>
          </div>
        )}
        {success && (
          <div className="absolute inset-0 flex items-center justify-center gap-1.5">
            <CheckIcon />
            <span className="text-sm font-medium text-white select-none">{successText || '验证通过'}</span>
          </div>
        )}
      </div>
      {/* 拖动手柄 */}
      <div
        className="absolute flex items-center justify-center rounded-full cursor-grab active:cursor-grabbing z-10 select-none"
        style={{
          width: handleSize, height: handleSize,
          left: pos, top: (height - handleSize) / 2 - 2,
          background: success ? '#1E8A5A' : '#FFFFFF',
          border: success ? '2px solid #1E8A5A' : dragging ? '2px solid #3C6FE3' : `2px solid #E4E7EB`,
          transition: dragging ? 'none' : 'all 0.35s cubic-bezier(0.34, 1.56, 0.64, 1)',
          boxShadow: success ? '0 0 0 4px rgba(30,138,90,0.15)' : '0 2px 6px rgba(16,20,28,0.06)',
        }}
        onMouseDown={onMouseDown}
        onTouchStart={onTouchStart}
      >
        {success ? <CheckIcon /> : (
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke={dragging ? '#3C6FE3' : '#7E858F'} strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <polyline points="9 18 15 12 9 6" />
          </svg>
        )}
      </div>
    </div>
  )
}

// ── 登录页 ────────────────────────────────────────────

const MAIN = '#3C6FE3'

function LoginPage() {
  const navigate = useNavigate()
  const { t } = useTranslation()
  const { login, error, clearError } = useAuthStore()

  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [showPw, setShowPw] = useState(false)
  const [remember, setRemember] = useState(false)
  const [dragOk, setDragOk] = useState(false)
  const [localErr, setLocalErr] = useState('')
  const [theme, setTheme] = useState<'light' | 'dark'>('light')
  const [btnPressed, setBtnPressed] = useState(false)
  const [eyeHover, setEyeHover] = useState(false)

  useEffect(() => {
    const saved = localStorage.getItem('app-config')
    if (saved) try { const c = JSON.parse(saved); if (c?.state?.theme) { const t2 = c.state.theme === 'dark' ? 'dark' : 'light'; setTheme(t2); document.documentElement.classList.toggle('dark', t2 === 'dark') } } catch { }
  }, [])

  const toggleTheme = () => { const n = theme === 'light' ? 'dark' : 'light'; setTheme(n); document.documentElement.classList.toggle('dark', n === 'dark') }

  const displayErr = localErr || error

  const validate = () => {
    if (!username.trim()) { setLocalErr(t('auth.usernameRequired')); return false }
    if (!password) { setLocalErr(t('auth.passwordRequired')); return false }
    if (!dragOk) { setLocalErr(t('auth.dragVerify')); return false }
    return true
  }

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    setLocalErr(''); clearError()
    if (!validate()) return
    try {
      const info = await login(username.trim(), password)
      navigate({ to: info.mustChangePassword ? '/change-password' : '/home' })
    } catch (err: unknown) { setLocalErr(extractError(err, t('auth.loginFailed'))) }
  }

  const onInput = (fn: (v: string) => void) => (e: React.ChangeEvent<HTMLInputElement>) => {
    fn(e.target.value); if (localErr) setLocalErr(''); if (error) clearError()
  }

  return (
    <div className="award-login min-h-screen flex">
      <style>{`
        @keyframes cardDrop {
          0% { opacity: 0; transform: translateY(-30px); }
          100% { opacity: 1; transform: translateY(0); }
        }
        .card-enter { animation: cardDrop 0.5s cubic-bezier(0.34, 1.56, 0.64, 1) forwards; }
        @keyframes fadeUp {
          0% { opacity: 0; transform: translateY(12px); }
          100% { opacity: 1; transform: translateY(0); }
        }
        .delayed-fade { animation: fadeUp 0.4s ease-out forwards; opacity: 0; }
        .delayed-fade:nth-child(1) { animation-delay: 0.1s; }
        .delayed-fade:nth-child(2) { animation-delay: 0.2s; }
        .delayed-fade:nth-child(3) { animation-delay: 0.3s; }
        .delayed-fade:nth-child(4) { animation-delay: 0.4s; }
        .delayed-fade:nth-child(5) { animation-delay: 0.5s; }
      `}</style>
      <div className="award-login-sheet">
      {/* ── 左侧品牌 ── */}
      <div className="award-cover hidden lg:flex relative overflow-hidden">
        <div className="relative z-10 flex flex-col justify-center items-center text-white p-16 w-full">
          <div className="award-cover-inner">
            <div className="mb-10">
              <div className="award-cover-mark w-28 h-28 mx-auto flex items-center justify-center">
                <svg viewBox="0 0 64 64" className="w-16 h-16" fill="none">
                  <rect x="8" y="20" width="48" height="36" rx="4" fill="white" fillOpacity="0.9" />
                  <circle cx="20" cy="38" r="6" fill="#3C6FE3" />
                  <circle cx="44" cy="38" r="6" fill="#6E9BF5" />
                  <path d="M32 14 L38 24 L32 20 L26 24 Z" fill="white" fillOpacity="0.9" />
                  <line x1="32" y1="24" x2="32" y2="30" stroke="white" strokeWidth="2" strokeOpacity="0.6" />
                </svg>
              </div>
            </div>
            <h1 className="award-cover-title">{t('common.appName')}</h1>
            <p className="award-cover-lede">{t('common.slogan')}</p>
            <div className="award-cover-list">
              {[t('features.userRoleManagement'), t('features.dataSecurity'), t('features.elegantExperience')].map((s, i) => (
                <div key={i} className="flex items-center justify-center gap-3 text-white/80">
                  <div className="w-2 h-2 bg-white rounded-full" />
                  <span className="text-lg">{s}</span>
                </div>
              ))}
            </div>
          </div>
        </div>
      </div>

      {/* ── 右侧表单 ── */}
      <div className="award-login-pane flex-1 flex items-center justify-center p-8 relative">
        <div className="absolute top-6 right-6 flex items-center gap-2">
          <button onClick={toggleTheme} className="w-9 h-9 flex items-center justify-center rounded-full hover:bg-gray-100 transition-colors">
            {theme === 'light' ? <Moon size={18} color="#787264" /> : <Sun size={18} color="#787264" />}
          </button>
        </div>

        <div className="w-full max-w-md">
          {/* 移动端标题 */}
          <div className="lg:hidden text-center mb-8">
            <h1 className="award-cover-title" style={{ fontSize: 28, color: '#17150F' }}>{t('common.appName')}</h1>
            <p className="text-sm mt-2" style={{ color: '#7E858F' }}>{t('common.welcomeBack')}</p>
          </div>

          <div className="award-login-card card-enter">
            <div className="text-center mb-8">
              <h2 className="award-login-title">{t('auth.accountLogin')}</h2>
              <p className="mt-2 text-sm">{t('auth.enterCredentials')}</p>
            </div>

            <form className="space-y-5" onSubmit={handleSubmit}>
              {/* 错误 */}
              {displayErr && (
                <div className="p-3 text-sm" style={{ borderLeft: '2px solid #C2412A', color: '#C2412A' }}>
                  {displayErr}
                </div>
              )}

              {/* 账号输入框 */}
              <div className="award-field relative delayed-fade">
                <div className="absolute left-4 top-1/2 -translate-y-1/2 z-10 pointer-events-none">
                  <UserIcon />
                </div>
                <input
                  required
                  placeholder={t('auth.usernamePlaceholder')}
                  value={username}
                  onChange={onInput(setUsername)}
                  className="w-full outline-none transition-all duration-200"
                />
              </div>

              {/* 密码输入框 */}
              <div className="award-field relative delayed-fade" style={{ animationDelay: '0.15s' }}>
                <div className="absolute left-4 top-1/2 -translate-y-1/2 z-10 pointer-events-none">
                  <LockIcon />
                </div>
                <input
                  required
                  placeholder={t('auth.passwordPlaceholder')}
                  type={showPw ? 'text' : 'password'}
                  value={password}
                  onChange={onInput(setPassword)}
                  className="w-full outline-none transition-all duration-200"
                />
                {/* 眼睛图标 */}
                <button
                  type="button"
                  className="absolute right-4 top-1/2 -translate-y-1/2 z-10 flex items-center justify-center transition-colors"
                  style={{ color: eyeHover ? MAIN : '#787264' }}
                  onMouseEnter={() => setEyeHover(true)}
                  onMouseLeave={() => setEyeHover(false)}
                  onClick={() => setShowPw(!showPw)}
                >
                  <EyeIcon open={showPw} color={eyeHover ? MAIN : undefined} />
                </button>
              </div>

              {/* 拖动验证 */}
              <div className="delayed-fade" style={{ animationDelay: '0.25s' }}>
                <DragVerify
                onSuccess={() => setDragOk(true)}
                onReset={() => setDragOk(false)}
                text={t('auth.dragHint')}
                successText={t('auth.dragSuccess')}
              />
              </div>

              {/* 记住我 + 忘记密码 */}
              <div className="flex items-center justify-between delayed-fade" style={{ animationDelay: '0.35s' }}>
                <label className="flex items-center gap-2 cursor-pointer select-none">
                  <input
                    type="checkbox"
                    checked={remember}
                    onChange={(e) => setRemember(e.target.checked)}
                    className="appearance-none w-4 h-4 rounded transition-all duration-200"
                    style={{
                      border: `2px solid ${remember ? MAIN : '#E4E7EB'}`,
                      background: remember ? MAIN : 'transparent',
                      borderRadius: 3,
                    }}
                  />
                  {remember && (
                    <svg className="absolute w-4 h-4 pointer-events-none" viewBox="0 0 24 24" fill="none" style={{ marginLeft: 0 }}>
                      <polyline points="20 6 9 17 4 12" stroke="white" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round" />
                    </svg>
                  )}
                  <span className="text-sm" style={{ color: '#7E858F' }}>{t('auth.rememberMe')}</span>
                </label>
                <a
                  href="#"
                  className="text-sm transition-all duration-200"
                  style={{ color: '#7E858F' }}
                  onClick={(e) => e.preventDefault()}
                  onMouseEnter={(e) => { e.currentTarget.style.color = MAIN; e.currentTarget.style.textDecoration = 'underline' }}
                  onMouseLeave={(e) => { e.currentTarget.style.color = '#787264'; e.currentTarget.style.textDecoration = 'none' }}
                >
                  {t('auth.forgotPassword')}
                </a>
              </div>

              {/* 登录按钮 */}
              <div className="delayed-fade" style={{ animationDelay: '0.45s' }}>
              <button
                type="submit"
                disabled={!dragOk || !username.trim() || !password}
                className="award-btn-login w-full font-bold text-white flex items-center justify-center"
                style={{ transform: btnPressed ? 'scale(0.99)' : undefined }}
                onMouseDown={() => (dragOk && username.trim() && password) && setBtnPressed(true)}
                onMouseUp={() => setBtnPressed(false)}
              >
                <span>{t('auth.login')}</span>
              </button>
              </div>

              {/* 注册 */}
              <p className="text-sm delayed-fade" style={{ color: '#7E858F', animationDelay: '0.55s' }}>
                {t('auth.noAccount')}
                <a
                  href="/register"
                  className="ml-1 font-bold transition-colors duration-200"
                  style={{ color: MAIN }}
                  onClick={(e) => { e.preventDefault(); navigate({ to: '/register' }) }}
                  onMouseEnter={(e) => { e.currentTarget.style.color = '#2E5BC4' }}
                  onMouseLeave={(e) => { e.currentTarget.style.color = MAIN }}
                >
                  {t('auth.registerNow')}
                </a>
              </p>
            </form>

            <div className="award-login-foot mt-8 pt-6 border-t border-gray-100">
              <p className="text-center text-xs">{t('common.copyright')}</p>
            </div>
          </div>
        </div>
      </div>
      </div>
    </div>
  )
}

export const Route = createFileRoute('/login')({
  component: LoginPage,
})
