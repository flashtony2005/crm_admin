import { Link, useMatches } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'

import {
  PRODUCT_NAV,
  filterNavByPerm,
  isSection,
  type NavLeaf,
  type NavNode,
} from '../../config/nav'
import { usePermission } from '../../hooks/usePermission'

/* ── 图标名 → SVG path(d) 映射 ─────────────────────────
 * 与 config/nav.ts 的 icon 字段对应，只负责把图标名渲染成 SVG。 */
const ICON_D: Record<string, string> = {
  home: 'M3 10.5 12 3l9 7.5V20a2 2 0 0 1-2 2h-4v-7h-6v7H5a2 2 0 0 1-2-2Z',
  settings:
    'M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6z M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z',
  users: 'M16 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2 M9 11a4 4 0 1 0 0-8 4 4 0 0 0 0 8z M22 21v-2a4 4 0 0 0-3-3.87',
  shield: 'M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z',
  book: 'M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z M14 2v6h6',
  page: 'M13 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9z M13 2v7h7 M9 13h6M9 17h6',
  article: 'M4 22h16a2 2 0 0 0 2-2V4a2 2 0 0 0-2-2H8a2 2 0 0 0-2 2v16a2 2 0 0 1-4 0V9 M18 14h-8M15 18h-5M10 6h8v4h-8z',
  tag: 'M12.586 2.586A2 2 0 0 0 11.172 2H4a2 2 0 0 0-2 2v7.172a2 2 0 0 0 .586 1.414l8.704 8.704a2.426 2.426 0 0 0 3.42 0l6.58-6.58a2.426 2.426 0 0 0 0-3.42z M7.5 7.5h.01',
  box: 'M21 8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16Z M3.3 7 12 12l8.7-5 M12 22V12',
  image: 'M19 3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V5a2 2 0 0 0-2-2z M8.5 10a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3z M21 15l-5-5L5 21',
  sparkles: 'M12 3l1.9 5.1L19 10l-5.1 1.9L12 17l-1.9-5.1L5 10l5.1-1.9zM19 15l.9 2.1L22 18l-2.1.9L19 21l-.9-2.1L16 18l2.1-.9z',
  check: 'M20 6 9 17l-5-5',
  clock: 'M12 8v4l3 3m6-3a9 9 0 1 1-18 0 9 9 0 0 1 18 0z',
  form: 'M9 3h6v3H9z M9 12h6M9 16h6 M4 5h16a1 1 0 0 1 1 1v14a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1z',
  target: 'M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20z M12 18a6 6 0 1 0 0-12 6 6 0 0 0 0 12z M12 14a2 2 0 1 0 0-4 2 2 0 0 0 0 4z',
  workflow: 'M3 3h6v6H3z M15 15h6v6h-6z M9 6h6a3 3 0 0 1 3 3v6 M12 15v3',
  plug: 'M9 2v6M15 2v6M6 8h12v3a6 6 0 0 1-6 6 6 6 0 0 1-6-6zM12 17v5',
  chart: 'M3 3v18h18 M7 14l4-4 3 3 5-6',
}
const DEFAULT_D = 'M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20z'

function iconD(name?: string): string {
  return (name && ICON_D[name]) || DEFAULT_D
}

function NavIcon({ d, size = 20 }: { d: string; size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none"
      stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
      <path d={d} />
    </svg>
  )
}

interface SidebarProps {
  collapsed: boolean
  className?: string
  onToggle?: () => void
}

/** 待审批数角标（Phase 2 接 AI 审批流后由 store 驱动；当前静态隐藏） */
import { useQuery } from '@tanstack/react-query'
import { request } from '../../api/client'
import { CMS_MODE } from '../../api/cms'

export function Sidebar({ collapsed, className = '', onToggle }: SidebarProps) {
  const matches = useMatches()
  const currentPath = matches[matches.length - 1]?.pathname ?? '/'
  const { has } = usePermission()
  const navItems = filterNavByPerm(PRODUCT_NAV, has)

  // 实时徽标（real 模式）：审批待办数等。30s 轮询 + 窗口聚焦刷新。
  const { data: badges = {} as Record<string, number> } = useQuery({
    queryKey: ['nav-badges'],
    enabled: CMS_MODE === 'real',
    queryFn: async (): Promise<Record<string, number>> => {
      try {
        const body = await request<{ ok: boolean; total: number }>('/api/approvals?status=pending')
        return { '/ai/approvals': body.total ?? 0 }
      } catch {
        return {}
      }
    },
    refetchInterval: 30_000,
  })

  const renderNode = (item: NavNode) =>
    isSection(item) ? (
      <NavGroup key={item.key} section={item} currentPath={currentPath} collapsed={collapsed} badges={badges} />
    ) : (
      <NavItemComp key={item.key} item={item} currentPath={currentPath} collapsed={collapsed} badges={badges} />
    )

  return (
    <aside className={`h-screen z-20 sticky top-0 ${className}`}>
      <div className={['flex flex-col h-full overflow-y-auto', 'bg-background border-r border-divider', 'py-6 px-3 md:w-full'].join(' ')}>
        {/* Brand */}
        <div className="flex gap-8 items-center px-6 mb-4">
          <div className="flex items-center gap-3">
            <div className="w-8 h-8 rounded-xl bg-primary flex items-center justify-center text-white font-bold text-sm">
              ✦
            </div>
            {!collapsed && (
              <span className="text-lg font-semibold text-default-900 tracking-tight">AI 工作台</span>
            )}
          </div>
        </div>

        {/* 导航：前端持有产品 IA（config/nav.ts），不依赖后端菜单接口 */}
        <nav className="flex flex-col gap-5 mt-2 px-2 flex-1 overflow-y-auto sidebar-scroll">
          {navItems.map(renderNode)}
        </nav>

        {/* Footer */}
        <div className="flex items-center justify-between gap-2 pt-8 pb-4 px-4 border-t border-divider mt-4">
          {!collapsed && <span className="text-xs text-default-400">AI-Native CMS · v0.1</span>}
          {onToggle && (
            <button
              type="button"
              onClick={onToggle}
              aria-label="切换侧边栏"
              className="ml-auto inline-flex items-center justify-center w-8 h-8 rounded-lg text-default-500 hover:bg-default-100 dark:hover:bg-default-100/10 transition-colors"
            >
              <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className={collapsed ? 'rotate-180' : ''}>
                <path d="m15 18-6-6 6-6" />
              </svg>
            </button>
          )}
        </div>
      </div>
    </aside>
  )
}

/* ── 导航分组 ─────────────────────────────────────── */
function NavGroup({
  section,
  currentPath,
  collapsed,
  badges,
}: {
  section: { key: string; label: string; children: NavLeaf[] }
  currentPath: string
  collapsed: boolean
  badges: Record<string, number>
}) {
  const { t } = useTranslation()
  const sectionTitle = t(`nav.${section.key}`, section.label)
  if (collapsed) {
    return (
      <>
        {section.children.map((child) => (
          <NavItemComp key={child.key} item={child} currentPath={currentPath} collapsed={collapsed} badges={badges} />
        ))}
      </>
    )
  }
  return (
    <div className="flex gap-1.5 flex-col">
      <span className="text-[11px] font-medium uppercase tracking-wider text-default-400 px-3.5">
        {sectionTitle}
      </span>
      {section.children.map((child) => (
        <NavItemComp key={child.key} item={child} currentPath={currentPath} collapsed={collapsed} badges={badges} />
      ))}
    </div>
  )
}

/* ── 导航项（Linear 风格：柔和底色 + 左侧指示条，不用重渐变） ── */
function NavItemComp({
  item,
  currentPath,
  collapsed,
  badges,
}: {
  item: NavLeaf
  currentPath: string
  collapsed: boolean
  badges: Record<string, number>
}) {
  const { t } = useTranslation()
  const itemTitle = t(`nav.${item.key}`, item.label)
  const isActive = currentPath === item.path
  const badge = badges[item.path] ?? 0

  return (
    <Link to={item.path} className="text-gray-900 no-underline block active:scale-[0.98] transition-transform">
      <div
        aria-current={isActive ? 'page' : undefined}
        className={`relative flex gap-2 w-full min-h-[38px] h-full items-center px-3 rounded-lg cursor-pointer transition-colors duration-150 ${
          isActive
            ? 'bg-[#EEF2FF] text-[#4F46E5] font-medium'
            : 'text-gray-600 hover:bg-gray-100/80'
        }`}
      >
        {isActive && (
          <span className="absolute left-0 top-1/2 -translate-y-1/2 -ml-1.5 w-1 h-4 rounded-full bg-[#6366F1]" />
        )}
        <span className={`flex-shrink-0 ${isActive ? '' : 'opacity-70'}`}>
          <NavIcon d={iconD(item.icon)} size={18} />
        </span>
        {!collapsed && <span className="text-sm">{itemTitle}</span>}
        {!collapsed && !!badge && badge > 0 && (
          <span className="ml-auto inline-flex items-center justify-center min-w-5 h-5 px-1 rounded-full bg-[#6366F1] text-white text-[11px] font-semibold">
            {badge}
          </span>
        )}
      </div>
    </Link>
  )
}

/* ── 滚动条样式 ──────────────────────────────────── */
if (typeof document !== 'undefined') {
  const id = '__sidebar_scrollbar'
  if (!document.getElementById(id)) {
    const el = document.createElement('style')
    el.id = id
    el.textContent = [
      '.sidebar-scroll { scrollbar-width: thin; scrollbar-color: rgba(148,163,184,0.5) transparent; }',
      '.sidebar-scroll::-webkit-scrollbar { width: 4px; }',
      '.sidebar-scroll::-webkit-scrollbar-track { background: transparent; }',
      '.sidebar-scroll::-webkit-scrollbar-thumb { background: rgba(148,163,184,0.5); border-radius: 4px; }',
      '.sidebar-scroll::-webkit-scrollbar-thumb:hover { background: rgba(148,163,184,0.8); }',
    ].join('\n')
    document.head.appendChild(el)
  }
}
