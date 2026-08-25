import { useState, useEffect } from 'react'
import { Outlet, useRouterState, Link } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'
import { Sidebar } from './Sidebar'
import { Header } from './Header'
import { TabBar } from '../navigation/TabBar'
import { useTabOpener } from '../../hooks/useTabOpener'
import { useConfigStore } from '../../store/config'
import { flattenNavLeaves } from '../../config/nav'
import { usePermission } from '../../hooks/usePermission'
import {
  DashboardIcon,
  UsersIcon,
  ShieldIcon,
  MenuMgmtIcon,
  SettingsGearIcon,
  ProfileIcon,
} from '../icons'

/** 菜单图标映射（水平模式用，取自现有 icons 集） */
const hIconMap: Record<string, React.FC<{ size?: number; className?: string }>> = {
  home: DashboardIcon,
  dashboard: DashboardIcon,
  users: UsersIcon,
  user: UsersIcon,
  role: ShieldIcon,
  roles: ShieldIcon,
  menu: MenuMgmtIcon,
  menus: MenuMgmtIcon,
  settings: SettingsGearIcon,
  config: SettingsGearIcon,
  profile: ProfileIcon,
}

function getHMenuIcon(iconName?: string | null) {
  if (!iconName) return DashboardIcon
  return hIconMap[iconName.toLowerCase()] ?? DashboardIcon
}

function HorizontalMenuBar() {
  const { t } = useTranslation()
  const { has } = usePermission()
  const matches = useRouterState().matches
  const currentPath = matches[matches.length - 1]?.pathname ?? '/'

  // 产品导航（前端持有）：Home 常驻首位 + 各叶子页面
  const allItems = [
    { id: -1, name: t('nav.home', '首页'), path: '/home', icon: 'home' },
    ...flattenNavLeaves()
      .filter((l) => l.path !== '/home' && (!l.perm || has(l.perm)))
      .map((l) => ({ id: 0, name: t(`nav.${l.key}`, l.label), path: l.path, icon: l.icon })),
  ]

  return (
    <div className="flex-shrink-0 z-30 bg-slate-900 border-b border-slate-700">
      <div className="flex items-center gap-1 px-4 h-12 overflow-x-auto">
        {allItems.map((item, idx) => {
          const Icon = getHMenuIcon(item.icon)
          const isActive = currentPath === item.path
          return (
            <Link
              key={`${item.path}-${idx}`}
              to={item.path ?? '#'}
              className={`flex items-center gap-1.5 px-3 py-1.5 rounded-md text-sm font-medium whitespace-nowrap transition-colors ${
                isActive ? 'bg-primary text-white' : 'text-slate-300 hover:bg-slate-800 hover:text-white'
              }`}
            >
              <Icon size={16} />
              {item.name}
            </Link>
          )
        })}
      </div>
    </div>
  )
}

export function MainLayout() {
  const config = useConfigStore()

  const [collapsed, setCollapsed] = useState(false)
  const [isMobile, setIsMobile] = useState(false)

  useTabOpener()

  useEffect(() => {
    const checkMobile = () => setIsMobile(window.innerWidth < 768)
    checkMobile()
    window.addEventListener('resize', checkMobile)
    return () => window.removeEventListener('resize', checkMobile)
  }, [])

  // 进入移动端时自动收起抽屉（仅在切换到移动端时触发，避免打开抽屉后被立刻关掉）
  useEffect(() => {
    if (isMobile) setCollapsed(true)
  }, [isMobile])

  const handleMaskClick = () => {
    if (isMobile && !collapsed) setCollapsed(true)
  }

  const tpl = config.layoutTemplate
  const isFullscreen = tpl === 'content-fullscreen'
  const isHorizontal = tpl === 'horizontal' || tpl === 'mixed-vertical'
  const isDoubleCol = tpl === 'double-column' || tpl === 'mixed-double-column'
  const isSideNav = tpl === 'side-nav'
  const showSidebar = !isFullscreen && !isHorizontal && !isMobile
  const showMobileSidebar = !isFullscreen && !isHorizontal && isMobile

  const effectiveSidebarWidth = isDoubleCol
    ? (collapsed ? 48 : 64)
    : isSideNav ? (collapsed ? 60 : 200)
    : (collapsed ? config.sidebarCollapsedWidth : config.sidebarWidth)
  const effectiveCollapsed = isDoubleCol ? true : collapsed

  const contentMaxWidth = config.contentLayout === 'fixed' ? '1024px' : '100%'

  return (
    <div className="flex h-screen overflow-hidden bg-default-50">
      {showMobileSidebar && !collapsed && (
        <div
          className="fixed inset-0 bg-black/50 z-40 md:hidden"
          role="button" tabIndex={0}
          onClick={handleMaskClick}
          onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') handleMaskClick() }}
        />
      )}

      {showSidebar || showMobileSidebar ? (
        <div
          className={`relative z-50 transition-all duration-300 flex-shrink-0 ${
            showMobileSidebar && !collapsed ? 'fixed inset-y-0 left-0' : ''
          } ${showMobileSidebar && collapsed ? '-translate-x-full' : 'translate-x-0'}`}
          style={{ width: effectiveSidebarWidth }}
        >
          <Sidebar collapsed={effectiveCollapsed} onToggle={() => setCollapsed(!collapsed)} />
        </div>
      ) : null}

      <div className="flex flex-col flex-1 overflow-hidden min-w-0">
        <Header onToggleSidebar={() => setCollapsed(!collapsed)} />
        {isHorizontal && <HorizontalMenuBar />}
        {config.tabbarEnabled && !isFullscreen && <TabBar />}
        <main
          className="flex-1 overflow-auto min-h-0"
          style={{ padding: isFullscreen ? 0 : config.contentCompact ? '0.25rem' : config.contentPadding }}
        >
          <div className="w-full h-full mx-auto" style={{ maxWidth: contentMaxWidth }}>
            <Outlet />
          </div>
        </main>
      </div>
    </div>
  )
}
