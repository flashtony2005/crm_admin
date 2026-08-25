import { useState, useEffect } from 'react'
import { useNavigate, useMatches } from '@tanstack/react-router'
import { useQuery } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { Button, Dropdown, Tooltip } from '@heroui/react'
import { getCurrentUser } from '../../api/auth'
import { useAuthStore } from '../../store/auth'
import { useConfigStore } from '../../store/config'
import { flattenNavLeaves, findNavLabel } from '../../config/nav'
import {
  MenuIcon, BellIcon,
  MaximizeIcon, MoonIcon, SunIcon,
  SettingsIcon, UserIcon, LogOutIcon,
  CheckCircleIcon, ShieldIcon, SettingsGearIcon,
} from '../icons'

interface HeaderProps { onToggleSidebar: () => void }

export function Header({ onToggleSidebar }: HeaderProps) {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const matches = useMatches()
  const currentPath = matches[matches.length - 1]?.pathname ?? '/'
  const [fullscreen, setFullscreen] = useState(false)
  // 全局 AI 输入（产品主入口：✦ 让 AI 帮你完成工作……）
  const [aiPrompt, setAiPrompt] = useState('')
  const config = useConfigStore()
  const logout = useAuthStore((s) => s.logout)

  const { data: user } = useQuery({
    queryKey: ['currentUser'],
    queryFn: getCurrentUser,
  })

  /** 面包屑标题：产品导航配置优先（走 i18n 翻译），未命中回退到路径段 */
  const crumbKey = flattenNavLeaves().find((l) => l.path === currentPath)?.key
  const crumbTitle = crumbKey
    ? t(`nav.${crumbKey}`, findNavLabel(currentPath) ?? '')
    : (currentPath.split('/').pop() ?? '')

  const handleLogout = () => logout()
  const handleAction = (key: string | number) => {
    if (key === 'logout') handleLogout()
    else if (key === 'profile') navigate({ to: '/profile' })
    else if (key === 'settings') navigate({ to: '/settings' })
  }

  const submitAiPrompt = (e: React.FormEvent) => {
    e.preventDefault()
    const q = aiPrompt.trim()
    navigate({ to: '/ai/assistant', search: q ? { q } : {} })
    setAiPrompt('')
  }

  const toggleFullscreen = () => {
    if (!document.fullscreenElement) {
      document.documentElement.requestFullscreen().catch(() => {})
      setFullscreen(true)
    } else { document.exitFullscreen().catch(() => {}); setFullscreen(false) }
  }

  useEffect(() => {
    if (config.theme === 'dark') document.documentElement.classList.add('dark')
    else if (config.theme === 'light') document.documentElement.classList.remove('dark')
    else document.documentElement.classList.toggle('dark', window.matchMedia('(prefers-color-scheme: dark)').matches)
  }, [config.theme])

  const darkMode = config.theme === 'dark' || (config.theme === 'system' && window.matchMedia('(prefers-color-scheme: dark)').matches)
  const toggleTheme = () => useConfigStore.getState().updateConfig({ theme: darkMode ? 'light' : 'dark' })

  const breadcrumbs = [{ path: '/', title: t('nav.home') }]
  if (currentPath !== '/') breadcrumbs.push({
    path: currentPath,
    title: crumbTitle,
  })

  return (
    <div className="dash-header">
      <div className="dash-header-inner">
        {/* 左侧 */}
        <div className="dash-header-left">
          <Tooltip><Tooltip.Trigger>
            <Button variant="ghost" size="sm" isIconOnly onPress={onToggleSidebar}>
              <MenuIcon size={16} />
            </Button>
          </Tooltip.Trigger><Tooltip.Content>{t('header.toggleSidebar')}</Tooltip.Content></Tooltip>

          <div className="dash-crumb">
            <span className="dash-crumb-sep">首页</span>
            {currentPath !== '/' && <><span className="dash-crumb-divider">/</span><span className="dash-crumb-cur">{crumbTitle}</span></>}
          </div>
        </div>

        {/* 右侧 */}
        <div className="dash-header-right">
          {/* 全局 AI 入口：产品主入口（PRODUCT_VISION §7） */}
          <form
            className="dash-srch min-w-[220px] md:min-w-[280px]"
            onSubmit={submitAiPrompt}
            role="search"
          >
            <span aria-hidden>✦</span>
            <input
              value={aiPrompt}
              onChange={(e) => setAiPrompt(e.target.value)}
              placeholder="让 AI 帮你完成工作……"
              aria-label="向 AI 描述你想完成的工作"
            />
            {aiPrompt.trim() && (
              <button type="submit" className="text-[11px] text-white bg-[#6366f1] rounded px-1.5 py-0.5 hover:bg-[#4f46e5] transition-colors">
                执行
              </button>
            )}
          </form>

          <Dropdown>
            <Dropdown.Trigger className="dash-ibtn">
              <BellIcon size={15} />
              <span className="dash-notif-dot" />
            </Dropdown.Trigger>
            <Dropdown.Popover placement="bottom end" className="w-80 p-0">
              <div className="px-4 py-3 border-b border-default-200 flex items-center justify-between">
                <span className="text-sm font-semibold text-[#111827]">通知</span>
                <span className="text-xs text-[#9ca3af]">3 条未读</span>
              </div>
              <div className="max-h-80 overflow-y-auto">
                {[
                  { icon: <CheckCircleIcon size={14} />, title: '新用户注册', desc: '用户 "alice" 刚刚加入', time: '2 分钟前', color: '#22c55e' },
                  { icon: <ShieldIcon size={14} />, title: '权限变更', desc: '角色 "管理员" 已更新', time: '1 小时前', color: '#f97316' },
                  { icon: <SettingsGearIcon size={14} />, title: '系统配置', desc: '布局模板已切换为纵向', time: '今天 09:12', color: '#6366f1' },
                ].map((n, i) => (
                  <div key={i} className="flex gap-3 px-4 py-3 hover:bg-[#f9fafb] transition-colors cursor-pointer">
                    <span style={{ color: n.color, marginTop: 2 }}>{n.icon}</span>
                    <div className="min-w-0 flex-1">
                      <p className="text-sm font-medium text-[#111827]">{n.title}</p>
                      <p className="text-xs text-[#9ca3af] truncate">{n.desc}</p>
                    </div>
                    <span className="text-[11px] text-[#d1d5db] whitespace-nowrap">{n.time}</span>
                  </div>
                ))}
              </div>
              <div className="px-4 py-2 border-t border-default-200 text-center">
                <span className="text-xs text-[#6366f1] hover:underline cursor-pointer">查看全部通知</span>
              </div>
            </Dropdown.Popover>
          </Dropdown>

          <Tooltip><Tooltip.Trigger>
            <button className="dash-ibtn" onClick={toggleFullscreen}><MaximizeIcon size={15} /></button>
          </Tooltip.Trigger><Tooltip.Content>{fullscreen ? t('header.exitFullscreen') : t('header.fullscreen')}</Tooltip.Content></Tooltip>

          <Tooltip><Tooltip.Trigger>
            <button className="dash-ibtn" onClick={toggleTheme}>{darkMode ? <SunIcon size={15} /> : <MoonIcon size={15} />}</button>
          </Tooltip.Trigger><Tooltip.Content>{t('header.toggleTheme')}</Tooltip.Content></Tooltip>

          <Tooltip><Tooltip.Trigger>
            <button className="dash-ibtn" onClick={() => navigate({ to: '/settings' })}><SettingsIcon size={15} /></button>
          </Tooltip.Trigger><Tooltip.Content>{t('header.settings')}</Tooltip.Content></Tooltip>

          <Dropdown>
            <Dropdown.Trigger className="dash-ubadge">
              <div className="dash-uav">{user?.nickname?.[0] ?? 'U'}</div>
              <span className="dash-uname">{user?.nickname ?? t('common.user')}</span>
            </Dropdown.Trigger>
            <Dropdown.Popover placement="bottom end">
              <Dropdown.Menu onAction={handleAction}>
                <Dropdown.Item id="profile"><span className="flex items-center gap-2"><UserIcon size={15} /> {t('header.profile')}</span></Dropdown.Item>
                <Dropdown.Item id="settings"><span className="flex items-center gap-2"><SettingsIcon size={15} /> {t('header.settings')}</span></Dropdown.Item>
                <Dropdown.Item id="logout"><span className="flex items-center gap-2 text-[#ef4444]"><LogOutIcon size={15} /> {t('header.logout')}</span></Dropdown.Item>
              </Dropdown.Menu>
            </Dropdown.Popover>
          </Dropdown>
        </div>
      </div>
    </div>
  )
}
