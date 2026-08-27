import { useState, useEffect, useCallback } from 'react'
import { createFileRoute } from '@tanstack/react-router'
import { useQuery, useMutation } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { Button, Switch, Input, Avatar, Card, Label, toast } from '@heroui/react'
import { PageContainer } from '../components/layout/PageContainer'
import { getCurrentUser, updateProfile, changePassword } from '../api/auth'
import { useAuthStore } from '../store/auth'
import { request } from '../api/client'
import { usePermission } from '../hooks/usePermission'
import { SITE_THEMES } from '../themes/siteThemes'
import { SITE_TEMPLATES } from '../themes/siteTemplates'

import { useConfigStore, type ThemeMode, type LayoutTemplate, type FontSize } from '../store/config'
import { useLanguageStore } from '../store/language'

// ── Inline icons ──────────────────────────────────────────────
const Icon = ({ children, size = 16 }: { children: React.ReactNode; size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor"
    strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="shrink-0">
    {children}
  </svg>
)

const LockIcon = () => <Icon><rect width="18" height="11" x="3" y="11" rx="2" ry="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/></Icon>
const ShieldCheckIcon = () => <Icon><path d="M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z"/><path d="m9 12 2 2 4-4"/></Icon>
const GlobeIcon = () => <Icon><circle cx="12" cy="12" r="10"/><path d="M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20"/><path d="M2 12h20"/></Icon>
const SunIcon2 = () => <Icon><circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M6.34 17.66l-1.41 1.41M19.07 4.93l-1.41 1.41"/></Icon>
const MoonIcon2 = () => <Icon><path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z"/></Icon>
const CameraIcon = () => <Icon><path d="M14.5 4h-5L7 7H4a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V9a2 2 0 0 0-2-2h-3l-2.5-3z"/><circle cx="12" cy="13" r="3"/></Icon>
const PaletteIcon = () => <Icon><path d="M12 3a9 9 0 1 0 0 18 2 2 0 0 0 2-2 2 2 0 0 1 2-2h1a4 4 0 0 0 4-4 9 9 0 0 0-9-8z"/><circle cx="7.5" cy="10.5" r="1"/><circle cx="12" cy="7.5" r="1"/><circle cx="16.5" cy="10.5" r="1"/></Icon>

// ── Validation helpers ────────────────────────────────────────
const emailRegex = /^[^\s@]+@[^\s@]+\.[^\s@]+$/

// ── Subsections ────────────────────────────────────────────────

function ProfileSection() {
  const { t } = useTranslation()
  const { data: user, isLoading, refetch } = useQuery({
    queryKey: ['currentUser'],
    queryFn: getCurrentUser,
    staleTime: 30_000,
  })

  const [nickname, setNickname] = useState('')
  const [email, setEmail] = useState('')
  const [, setEmailError] = useState('')
  const [dirty, setDirty] = useState(false)

  useEffect(() => {
    if (user) {
      setNickname(user.nickname || '')
      setEmail(user.email || '')
      setDirty(false)
    }
  }, [user])

  const validateEmail = (v: string) => (!v || emailRegex.test(v) ? '' : t('settings.emailInvalid'))

  const onNickname = useCallback((v: string) => { setNickname(v); setDirty(true) }, [])
  const onEmail = useCallback((v: string) => {
    setEmail(v)
    setEmailError(validateEmail(v))
    setDirty(true)
  }, [t])

  const saveMutation = useMutation({
    mutationFn: async (data: { nickname: string; email?: string }) => {
      const res = await updateProfile({ nickname: data.nickname, email: data.email })
      // 同步到全局 auth store（侧边栏昵称/邮箱展示）
      const cur = useAuthStore.getState().user
      if (cur) useAuthStore.setState({ user: { ...cur, nickname: res.nickname, email: res.email } })
      return res
    },
    onSuccess: () => {
      toast(t('settings.profileUpdated'), { variant: 'success' })
      setDirty(false)
      refetch()
    },
    onError: (err: Error) => {
      toast(err.message, { variant: 'danger' })
    },
  })

  const handleSave = () => {
    const err = validateEmail(email)
    if (err) { setEmailError(err); return }
    saveMutation.mutate({ nickname, email: email || undefined })
  }

  const handleCancel = () => {
    if (user) { setNickname(user.nickname || ''); setEmail(user.email || '') }
    setDirty(false)
    setEmailError('')
  }

  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-20">
        <div className="text-default-400 text-sm">{t('common.loading')}</div>
      </div>
    )
  }

  return (
    <div className="max-w-xl space-y-6 py-2">
      {/* Avatar */}
      <div className="flex flex-col items-center gap-3 sm:flex-row sm:items-start">
        <div className="relative">
          <Avatar
            className="w-20 h-20 text-xl"
            
          />
          <Button
            isIconOnly
            size="sm"
            className="absolute -bottom-1 -right-1 rounded-full shadow-md"
            onPress={() => toast(t('settings.avatarUploadHint'), { variant: 'default' })}
          >
            <CameraIcon />
          </Button>
        </div>
        <div className="text-center sm:text-left">
          <h3 className="font-semibold text-lg">{t('settings.profileTitle')}</h3>
          <p className="text-sm text-default-500">{t('settings.profileDesc')}</p>
        </div>
      </div>

      {/* Form fields */}
      <div className="space-y-4">
        <Input
          value={user?.username ?? ''}
          readOnly
        />
        <p className="text-xs text-default-400 -mt-3">{t('settings.usernameReadonly')}</p>
        <Input
          value={nickname}
          onChange={(e: any) => onNickname(e.target.value)}
          placeholder={t('settings.nicknamePlaceholder')}
        />
        <Input
          type="email"
          value={email}
          onChange={(e: any) => onEmail(e.target.value)}
          placeholder={t('settings.emailPlaceholder')}
          
          
          
        />
      </div>

      {/* Actions */}
      {dirty && (
        <div className="flex gap-3 pt-2">
          <Button  onPress={handleSave}
            >
            {t('common.save')}
          </Button>
          <Button variant="outline" onPress={handleCancel}>
            {t('common.cancel')}
          </Button>
        </div>
      )}
    </div>
  )
}

// ── Layout icon ──
const LayoutIcon = () => <Icon><rect width="18" height="18" x="3" y="3" rx="2"/><line x1="9" y1="3" x2="9" y2="21"/><line x1="3" y1="9" x2="21" y2="9"/></Icon>
const TypeIcon = () => <Icon><polyline points="4 7 4 4 20 4 20 7"/><line x1="9" y1="20" x2="15" y2="20"/><line x1="12" y1="4" x2="12" y2="20"/></Icon>

function PreferencesSection() {
  const { t } = useTranslation()
  const config = useConfigStore()
  const updateConfig = useConfigStore((s) => s.updateConfig)
  const { language, setLanguage } = useLanguageStore()

  // ── Theme ──
  const themeModes: { id: ThemeMode; name: string; icon: React.ReactNode }[] = [
    { id: 'light', name: t('settings.themeLight'), icon: <SunIcon2 /> },
    { id: 'dark', name: t('settings.themeDark'), icon: <MoonIcon2 /> },
    { id: 'system', name: t('settings.themeSystem'), icon: <GlobeIcon /> },
  ]

  const handleTheme = (mode: ThemeMode) => {
    updateConfig({ theme: mode })
    if (mode === 'dark') document.documentElement.classList.add('dark')
    else if (mode === 'light') document.documentElement.classList.remove('dark')
    else {
      document.documentElement.classList.toggle(
        'dark',
        window.matchMedia('(prefers-color-scheme: dark)').matches,
      )
    }
    toast(t('settings.themeUpdated'), { variant: 'success' })
  }

  // ── Language ──
  const handleLang = (lang: 'zh-CN' | 'en') => {
    setLanguage(lang)
    toast(t('settings.langChanged'), { variant: 'success' })
  }

  // ── Layout ──
  const layoutTemplates: { id: LayoutTemplate; name: string; desc: string }[] = [
    { id: 'vertical', name: t('settings.layoutVertical'), desc: t('settings.layoutVerticalDesc') },
    { id: 'double-column', name: t('settings.layoutDoubleColumn'), desc: t('settings.layoutDoubleColumnDesc') },
    { id: 'horizontal', name: t('settings.layoutHorizontal'), desc: t('settings.layoutHorizontalDesc') },
    { id: 'side-nav', name: t('settings.layoutSideNav'), desc: t('settings.layoutSideNavDesc') },
    { id: 'mixed-vertical', name: t('settings.layoutMixedVertical'), desc: t('settings.layoutMixedVerticalDesc') },
    { id: 'mixed-double-column', name: t('settings.layoutMixedDoubleColumn'), desc: t('settings.layoutMixedDoubleColumnDesc') },
    { id: 'content-fullscreen', name: t('settings.layoutContentFullscreen'), desc: t('settings.layoutContentFullscreenDesc') },
  ]

  const contentLayouts: { id: typeof config.contentLayout; name: string }[] = [
    { id: 'fluid', name: t('settings.contentFluid') },
    { id: 'fixed', name: t('settings.contentFixed') },
  ]

  const handleLayout = (tpl: LayoutTemplate) => {
    updateConfig({ layoutTemplate: tpl })
    toast(t('settings.layoutUpdated'), { variant: 'success' })
  }

  const handleContentLayout = (v: typeof config.contentLayout) => {
    updateConfig({ contentLayout: v })
    toast(v === 'fluid' ? t('settings.contentLayoutFluid') : t('settings.contentLayoutFixed'), { variant: 'success' })
  }

  // ── Font size ──
  const fontSizes: { id: FontSize; label: string; preview: string }[] = [
    { id: 'sm', label: t('settings.fontSizeSmall'), preview: 'text-sm' },
    { id: 'base', label: t('settings.fontSizeMedium'), preview: 'text-base' },
    { id: 'lg', label: t('settings.fontSizeLarge'), preview: 'text-lg' },
    { id: 'xl', label: t('settings.fontSizeExtra'), preview: 'text-xl' },
    { id: '2xl', label: t('settings.fontSizeHuge'), preview: 'text-2xl' },
  ]

  const handleFontSize = (fs: FontSize) => {
    updateConfig({ fontSize: fs })
    const sizeMap: Record<FontSize, string> = {
      sm: '0.875rem',
      base: '1rem',
      lg: '1.125rem',
      xl: '1.25rem',
      '2xl': '1.5rem',
    }
    document.documentElement.style.setProperty('--app-font-size', sizeMap[fs])
    document.documentElement.style.fontSize = sizeMap[fs]
    toast(t('settings.fontSizeUpdated'), { variant: 'success' })
  }

  // Apply saved fontSize on mount
  useEffect(() => {
    const sizeMap: Record<FontSize, string> = {
      sm: '0.875rem',
      base: '1rem',
      lg: '1.125rem',
      xl: '1.25rem',
      '2xl': '1.5rem',
    }
    document.documentElement.style.setProperty('--app-font-size', sizeMap[config.fontSize])
    document.documentElement.style.fontSize = sizeMap[config.fontSize]
  }, [config.fontSize])

  return (
    <div className="max-w-xl space-y-8 py-2">
      {/* Theme */}
      <div>
        <h3 className="text-base font-semibold mb-4 flex items-center gap-2">
          {config.theme === 'dark' ? <MoonIcon2 /> : <SunIcon2 />} {t('settings.themeMode')}
        </h3>
        <div className="grid grid-cols-3 gap-3">
          {themeModes.map((m) => (
            <Button
              key={m.id}
              variant={config.theme === m.id ? 'outline' : 'ghost'}
              onPress={() => handleTheme(m.id)}
              className="flex flex-col items-center gap-2 py-5"
            >
              <span className={config.theme === m.id ? 'text-primary' : 'text-default-500'}>
                {m.icon}
              </span>
              <span className="text-sm font-medium">{m.name}</span>
            </Button>
          ))}
        </div>
      </div>

      {/* 界面模式：简单模式默认隐藏本体建模/系统管理等专业入口 */}
      <div>
        <h3 className="text-base font-semibold mb-4 flex items-center gap-2">
          <LayoutIcon /> {t('settings.interfaceMode')}
        </h3>
        <Card className="border border-default-200 overflow-hidden">
          <button
            type="button"
            role="switch"
            aria-checked={config.advancedMode}
            onClick={() => {
              updateConfig({ advancedMode: !config.advancedMode })
              toast(t('settings.advancedModeUpdated'), { variant: 'success' })
            }}
            className="flex w-full items-center justify-between gap-3 px-5 py-4 text-left cursor-pointer"
          >
            <div>
              <p className="font-medium text-sm">{t('settings.advancedMode')}</p>
              <p className="text-xs text-default-400 mt-0.5">
                {t('settings.advancedModeDesc')}
              </p>
            </div>
            <span
              aria-hidden
              className={`relative inline-flex h-6 w-11 shrink-0 items-center rounded-full transition-colors ${
                config.advancedMode ? 'bg-primary' : 'bg-default-300'
              }`}
            >
              <span
                className={`inline-block h-4 w-4 transform rounded-full bg-white shadow-sm transition-transform ${
                  config.advancedMode ? 'translate-x-6' : 'translate-x-1'
                }`}
              />
            </span>
          </button>
        </Card>
      </div>

      {/* Language */}
      <div>
        <h3 className="text-base font-semibold mb-4 flex items-center gap-2">
          <GlobeIcon /> {t('settings.languagePreference')}
        </h3>
        <div className="flex gap-3">
          {([
            ['zh-CN', t('settings.langZhCN')],
            ['en', t('settings.langEn')],
          ] as const).map(([code, label]) => (
            <Button
              key={code}
              variant={language === code ? 'outline' : 'ghost'}
              onPress={() => handleLang(code)}
              className="flex-1"
            >
              <span className="text-sm font-medium">{label}</span>
            </Button>
          ))}
        </div>
      </div>

      {/* Layout template */}
      <div>
        <h3 className="text-base font-semibold mb-4 flex items-center gap-2">
          <LayoutIcon /> {t('settings.layoutStyle')}
        </h3>
        <div className="grid grid-cols-2 sm:grid-cols-3 gap-3">
          {layoutTemplates.map((tpl) => (
            <Button
              key={tpl.id}
              variant={config.layoutTemplate === tpl.id ? 'outline' : 'ghost'}
              onPress={() => handleLayout(tpl.id)}
              className="flex flex-col gap-1 py-3 h-auto items-start text-left"
            >
              <span className="text-sm font-medium">{tpl.name}</span>
              <span className="text-xs text-default-400">{tpl.desc}</span>
            </Button>
          ))}
        </div>
      </div>

      {/* Content layout */}
      <div>
        <h3 className="text-base font-semibold mb-4">{t('settings.contentLayout')}</h3>
        <div className="flex gap-3">
          {contentLayouts.map((cl) => (
            <Button
              key={cl.id}
              variant={config.contentLayout === cl.id ? 'outline' : 'ghost'}
              onPress={() => handleContentLayout(cl.id)}
              className="flex-1"
            >
              <span className="text-sm font-medium">{cl.name}</span>
            </Button>
          ))}
        </div>
      </div>

      {/* Font size */}
      <div>
        <h3 className="text-base font-semibold mb-4 flex items-center gap-2">
          <TypeIcon /> {t('settings.fontSize')}
        </h3>
        <div className="flex gap-3">
          {fontSizes.map((fs) => (
            <Button
              key={fs.id}
              variant={config.fontSize === fs.id ? 'outline' : 'ghost'}
              onPress={() => handleFontSize(fs.id)}
              className="flex flex-col items-center gap-1 flex-1 py-3 h-auto"
            >
              <span className={`font-semibold ${fs.preview}`}>{fs.label}</span>
            </Button>
          ))}
        </div>
      </div>
    </div>
  )
}

function SecuritySection() {
  const { t } = useTranslation()
  // ── Change password ──
  const [oldPwd, setOldPwd] = useState('')
  const [newPwd, setNewPwd] = useState('')
  const [confirmPwd, setConfirmPwd] = useState('')
  const [, setNewPwdErr] = useState('')
  const [, setConfirmPwdErr] = useState('')
  const [pwdDirty, setPwdDirty] = useState(false)

  const validatePassword = (v: string) => {
    if (!v) return t('settings.passwordRequired')
    if (v.length < 8) return t('settings.passwordMinLength')
    return ''
  }

  const onNewPwd = useCallback((v: string) => {
    setNewPwd(v)
    setNewPwdErr(validatePassword(v))
    if (confirmPwd && v !== confirmPwd) setConfirmPwdErr(t('settings.passwordMismatch'))
    else setConfirmPwdErr('')
    setPwdDirty(true)
  }, [confirmPwd, t])

  const onConfirmPwd = useCallback((v: string) => {
    setConfirmPwd(v)
    setConfirmPwdErr(v && v !== newPwd ? t('settings.passwordMismatch') : '')
    setPwdDirty(true)
  }, [newPwd, t])

  const pwdMutation = useMutation({
    mutationFn: async (data: { oldPassword: string; newPassword: string }) => {
      return changePassword({ old_password: data.oldPassword, new_password: data.newPassword })
    },
    onSuccess: () => {
      toast(t('settings.passwordChanged'), { variant: 'success' })
      setOldPwd(''); setNewPwd(''); setConfirmPwd(''); setPwdDirty(false)
    },
    onError: (err: Error) => {
      toast(err.message, { variant: 'danger' })
    },
  })

  const handleChangePwd = () => {
    if (!oldPwd) { toast(t('settings.currentPasswordRequired'), { variant: 'warning' }); return }
    const err = validatePassword(newPwd)
    if (err) { setNewPwdErr(err); return }
    if (newPwd !== confirmPwd) { setConfirmPwdErr(t('settings.passwordMismatch')); return }
    pwdMutation.mutate({ oldPassword: oldPwd, newPassword: newPwd })
  }

  const handlePwdCancel = () => {
    setOldPwd(''); setNewPwd(''); setConfirmPwd('')
    setNewPwdErr(''); setConfirmPwdErr(''); setPwdDirty(false)
  }

  // ── 2FA ──
  const [twoFAEnabled, setTwoFAEnabled] = useState(false)

  const handleToggle2fa = useCallback((enabled: boolean) => {
    setTwoFAEnabled(enabled)
    toast(enabled ? t('settings.twoFactorEnabled') : t('settings.twoFactorDisabled'), {
      description: enabled ? t('settings.twoFactorEnableHint') : t('settings.twoFactorDisabledHint'),
      variant: enabled ? 'success' : 'warning',
    })
  }, [t])

  return (
    <div className="max-w-xl space-y-10 py-2">
      {/* ── Change Password ── */}
      <div>
        <h3 className="text-base font-semibold mb-5 flex items-center gap-2">
          <LockIcon /> {t('settings.changePassword')}
        </h3>
        <div className="space-y-4">
          <Input
            type="password"
            value={oldPwd}
            onChange={(e: any) => { setOldPwd(e.target.value); setPwdDirty(true) }}
            placeholder={t('settings.currentPasswordPlaceholder')}
          />
          <Input
            type="password"
            value={newPwd}
            onChange={(e: any) => onNewPwd(e.target.value)}
            placeholder={t('settings.newPasswordPlaceholder')}
          />
          <p className="text-xs text-default-400 -mt-2">{t('settings.passwordHint')}</p>
          <Input
            type="password"
            value={confirmPwd}
            onChange={(e: any) => onConfirmPwd(e.target.value)}
            placeholder={t('settings.confirmPasswordPlaceholder')}
            
            
          />
          {pwdDirty && (
            <div className="flex gap-3 pt-1">
              <Button  onPress={handleChangePwd}
                >
                {t('settings.updatePassword')}
              </Button>
              <Button variant="outline" onPress={handlePwdCancel}>
                {t('common.cancel')}
              </Button>
            </div>
          )}
        </div>
      </div>

      {/* ── 2FA ── */}
      <div>
        <h3 className="text-base font-semibold mb-4 flex items-center gap-2">
          <ShieldCheckIcon /> {t('settings.twoFactorAuth')}
        </h3>
        <Card className="border border-default-200">
          <div className="flex items-center justify-between px-5 py-4">
            <div>
              <p className="font-medium text-sm">{t('settings.twoFactorTitle')}</p>
              <p className="text-xs text-default-400 mt-0.5">
                {t('settings.twoFactorDesc')}
              </p>
            </div>
            <Switch isSelected={twoFAEnabled} onChange={handleToggle2fa} />
          </div>
        </Card>
      </div>
    </div>
  )
}

// ── Site appearance (Owner only: site.settings.update) ─────────
function SiteAppearanceSection() {
  const { t } = useTranslation()
  const [theme, setTheme] = useState<string>('paper')
  const [template, setTemplate] = useState<string>('default')
  const [title, setTitle] = useState<string>('LightPress')
  const [tagline, setTagline] = useState<string>('')
  const [dirty, setDirty] = useState(false)
  const [saving, setSaving] = useState(false)
  const [loaded, setLoaded] = useState(false)

  const load = useCallback(() => {
    let alive = true
    fetch('/api/public/site')
      .then((r) => r.json())
      .then((b) => {
        if (alive && b?.ok && b.data) {
          setTheme(b.data.theme || 'paper')
          setTemplate(b.data.template || 'default')
          setTitle(b.data.siteTitle || 'LightPress')
          setTagline(b.data.siteTagline || '')
          setLoaded(true)
        }
      })
      .catch(() => {})
    return () => {
      alive = false
    }
  }, [])

  useEffect(() => load(), [load])

  const patch = (fn: () => void) => {
    fn()
    setDirty(true)
  }

  const save = async () => {
    setSaving(true)
    try {
      await request('/api/admin/site', {
        method: 'PUT',
        body: JSON.stringify({
          theme,
          template,
          site_title: title,
          site_tagline: tagline,
        }),
      })
      toast(t('settings.appearanceSaved'), { variant: 'success' })
      setDirty(false)
    } catch (err) {
      toast((err as Error).message, { variant: 'danger' })
    } finally {
      setSaving(false)
    }
  }

  const reset = () => {
    load()
    setDirty(false)
  }

  if (!loaded) {
    return (
      <div className="py-8 text-default-400 text-sm">{t('common.loading')}</div>
    )
  }

  return (
    <div className="max-w-xl space-y-8 py-2">
      {/* Theme */}
      <div>
        <h3 className="text-base font-semibold mb-1">{t('settings.appearanceTheme')}</h3>
        <p className="text-xs text-default-400 mb-4">{t('settings.appearanceThemeHint')}</p>
        <div className="grid grid-cols-2 sm:grid-cols-4 gap-3">
          {SITE_THEMES.map((tm) => (
            <button
              key={tm.key}
              type="button"
              onClick={() => patch(() => setTheme(tm.key))}
              className={`rounded-xl border p-3 text-left transition ${
                theme === tm.key ? 'border-primary ring-2 ring-primary/30' : 'border-default-200'
              }`}
              style={{ background: tm.vars.surface }}
            >
              <div className="flex gap-1 mb-2">
                <span className="w-4 h-4 rounded-full border" style={{ background: tm.vars.bg }} />
                <span className="w-4 h-4 rounded-full border" style={{ background: tm.vars.accent }} />
                <span className="w-4 h-4 rounded-full border" style={{ background: tm.vars.text }} />
              </div>
              <p className="text-sm font-medium" style={{ color: tm.vars.text }}>{tm.name}</p>
            </button>
          ))}
        </div>
      </div>

      {/* Template */}
      <div>
        <h3 className="text-base font-semibold mb-1">{t('settings.appearanceTemplate')}</h3>
        <p className="text-xs text-default-400 mb-4">{t('settings.appearanceTemplateDesc')}</p>
        <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
          {SITE_TEMPLATES.map((tp) => (
            <button
              key={tp.key}
              type="button"
              onClick={() => patch(() => setTemplate(tp.key))}
              className={`rounded-xl border p-4 text-left transition ${
                template === tp.key ? 'border-primary ring-2 ring-primary/30' : 'border-default-200'
              }`}
            >
              <p className="text-sm font-medium">{tp.name}</p>
              <p className="text-xs text-default-400 mt-1">{tp.desc}</p>
            </button>
          ))}
        </div>
      </div>

      {/* Branding */}
      <div className="space-y-4">
        <div className="space-y-1.5">
          <Label>{t('settings.appearanceSiteTitle')}</Label>
          <Input
            value={title}
            onChange={(e: any) => patch(() => setTitle(e.target.value))}
            placeholder={t('settings.appearanceSiteTitlePlaceholder')}
          />
        </div>
        <div className="space-y-1.5">
          <Label>{t('settings.appearanceSiteTagline')}</Label>
          <Input
            value={tagline}
            onChange={(e: any) => patch(() => setTagline(e.target.value))}
            placeholder={t('settings.appearanceSiteTaglinePlaceholder')}
          />
        </div>
      </div>

      {dirty && (
        <div className="flex gap-3 pt-2">
          <Button onPress={save} isDisabled={saving}>
            {t('common.save')}
          </Button>
          <Button variant="outline" onPress={reset}>
            {t('common.cancel')}
          </Button>
        </div>
      )}
    </div>
  )
}

// ── Main page ──────────────────────────────────────────────────
type SettingsTab = 'profile' | 'preferences' | 'security' | 'appearance'

function SettingsPage() {
  const { t } = useTranslation()
  const [activeTab, setActiveTab] = useState<SettingsTab>('profile')
  const { has } = usePermission()

  const tabs: { id: SettingsTab; label: string; icon: React.ReactNode }[] = [
    {
      id: 'profile',
      label: t('settings.tabProfile'),
      icon: <Icon size={16}><path d="M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2"/><circle cx="12" cy="7" r="4"/></Icon>,
    },
    {
      id: 'preferences',
      label: t('settings.tabPreferences'),
      icon: <Icon size={16}><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/></Icon>,
    },
    {
      id: 'security',
      label: t('settings.tabSecurity'),
      icon: <ShieldCheckIcon />,
    },
    ...(has('site.settings.update')
      ? [
          {
            id: 'appearance' as SettingsTab,
            label: t('settings.tabAppearance'),
            icon: <PaletteIcon />,
          },
        ]
      : []),
  ]

  return (
    <PageContainer title={t('settings.title')} subtitle={t('settings.subtitle')}>
      <div className="w-full">
        {/* Tab header */}
        <div className="flex gap-1 border-b border-zinc-200 dark:border-zinc-700">
          {tabs.map((tab) => (
            <button
              key={tab.id}
              onClick={() => setActiveTab(tab.id)}
              className={`pb-2 px-4 text-sm font-medium border-b-2 transition-colors ${
                activeTab === tab.id
                  ? 'border-primary text-primary'
                  : 'border-transparent text-zinc-500 hover:text-zinc-700 dark:hover:text-zinc-300'
              }`}
            >
              <span className="flex items-center gap-1.5">{tab.icon} {tab.label}</span>
            </button>
          ))}
        </div>

        {/* Tab content */}
        <div className="px-4 py-2">
          {activeTab === 'profile' && <ProfileSection />}
          {activeTab === 'preferences' && <PreferencesSection />}
          {activeTab === 'security' && <SecuritySection />}
          {activeTab === 'appearance' && <SiteAppearanceSection />}
        </div>
      </div>
    </PageContainer>
  )
}

export const Route = createFileRoute('/settings')({
  component: SettingsPage,
})
