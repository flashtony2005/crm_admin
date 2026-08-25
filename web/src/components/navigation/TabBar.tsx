import { useCallback, useRef, useState, useEffect } from 'react'
import { useNavigate, useLocation } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'
import { useTabStore } from '../../store/tabs'
import { useConfigStore } from '../../store/config'

/** 路径 → i18n 键 */
const TAB_TITLE_KEYS: Record<string, string> = {
  '/dashboard': 'nav.dashboard',
  '/users': 'nav.users',
  '/roles': 'nav.roles',
  '/profile': 'nav.profile',
  '/settings': 'nav.settings',
  '/sys/user': 'tableLabel.user',
  '/sys/role': 'tableLabel.role',
  '/sys/menu': 'tableLabel.menu',
  '/sys/tenant': 'tableLabel.tenant',
  '/sys/dict': 'tableLabel.dict',
  '/sys/dict_data': 'tableLabel.dict_data',
  '/sys/config': 'tableLabel.config',
  '/sys/notice': 'tableLabel.notice',
  '/sys/oper_log': 'tableLabel.oper_log',
  '/sys/login_log': 'tableLabel.login_log',
  '/sys/job': 'tableLabel.job',
  '/sys/job_log': 'tableLabel.job_log',
  '/sys/cache_store': 'tableLabel.cache_store',
}

/** 根据路径获取当前语言的 tab 标题 */
function tabTitle(path: string, t: (key: string) => string): string {
  const key = TAB_TITLE_KEYS[path]
  if (key) return t(key)
  return path.split('/').pop() ?? path
}

// ── 右键菜单 ─────────────────────────────────────

interface CtxMenuState {
  visible: boolean
  x: number
  y: number
  tabId: string
}

export function TabBar() {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const location = useLocation()
  const { tabs, activeTab, setActiveTab, closeTab, closeOtherTabs, closeAllTabs } =
    useTabStore()
  const tabbarHeight = useConfigStore((s) => s.tabbarHeight)
  const menuRef = useRef<HTMLDivElement>(null)

  const [ctxMenu, setCtxMenu] = useState<CtxMenuState>({ visible: false, x: 0, y: 0, tabId: '' })

  // 点击外部关闭右键菜单
  useEffect(() => {
    if (!ctxMenu.visible) return
    const handler = () => setCtxMenu((s) => ({ ...s, visible: false }))
    document.addEventListener('click', handler)
    return () => document.removeEventListener('click', handler)
  }, [ctxMenu.visible])

  // ESC 关闭菜单
  useEffect(() => {
    if (!ctxMenu.visible) return
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setCtxMenu((s) => ({ ...s, visible: false }))
    }
    document.addEventListener('keydown', handler)
    return () => document.removeEventListener('keydown', handler)
  }, [ctxMenu.visible])

  const handleClick = (tab: typeof activeTab) => {
    if (!tab) return
    setActiveTab(tab)
    navigate({ to: tab.path })
  }

  const handleClose = (e: React.MouseEvent, id: string) => {
    e.stopPropagation()
    closeTab(id)
    const remaining = useTabStore.getState().tabs
    if (remaining.length > 0) {
      const last = remaining[remaining.length - 1]
      navigate({ to: last.path })
    }
  }

  const handleContextMenu = (e: React.MouseEvent, tabId: string) => {
    e.preventDefault()
    e.stopPropagation()
    setCtxMenu({ visible: true, x: e.clientX, y: e.clientY, tabId })
  }

  const ctxAction = useCallback((action: string) => {
    const id = ctxMenu.tabId
    setCtxMenu((s) => ({ ...s, visible: false }))
    switch (action) {
      case 'close':
        closeTab(id)
        break
      case 'closeOthers':
        closeOtherTabs(id)
        break
      case 'closeRight': {
        const idx = tabs.findIndex((t) => t.id === id)
        if (idx >= 0) {
          const rightTabs = tabs.slice(idx + 1)
          rightTabs.forEach((t) => closeTab(t.id))
        }
        break
      }
      case 'closeAll':
        closeAllTabs()
        break
    }
    // 导航到最后一个剩余 tab
    setTimeout(() => {
      const remaining = useTabStore.getState().tabs
      if (remaining.length > 0) {
        navigate({ to: remaining[remaining.length - 1].path })
      } else {
        navigate({ to: '/home' })
      }
    }, 0)
  }, [ctxMenu.tabId, tabs, closeTab, closeOtherTabs, closeAllTabs, navigate])

  if (tabs.length === 0) return null

  return (
    <div
      className="flex items-center bg-background border-b border-default-200 px-2 gap-1 overflow-x-auto scrollbar-thin"
      style={{ height: tabbarHeight }}
    >
      {tabs.map((tab) => {
        const isActive = activeTab?.id === tab.id || location.pathname === tab.path
        return (
          <div
            key={tab.id}
            onClick={() => handleClick(tab)}
            onContextMenu={(e) => handleContextMenu(e, tab.id)}
            className={`group relative flex items-center gap-1.5 h-7 px-3 rounded-md cursor-pointer whitespace-nowrap transition-colors text-sm ${
              isActive
                ? 'tab-active-gradient text-yellow-900'
                : 'text-default-600 hover:bg-default-100'
            }`}
          >
            <span>{tabTitle(tab.path, t)}</span>
            {tab.closable && !tab.pinned && (
              <button
                type="button"
                onClick={(e) => handleClose(e, tab.id)}
                className={`inline-flex items-center justify-center w-4 h-4 rounded text-xs transition-opacity ${
                  isActive
                    ? 'opacity-70 hover:opacity-100'
                    : 'opacity-0 group-hover:opacity-70 hover:opacity-100'
                }`}
              >
                ✕
              </button>
            )}
            {tab.pinned && (
              <span className="text-xs">📌</span>
            )}
          </div>
        )
      })}

      <div className="flex items-center gap-1 ml-auto pl-2 border-l border-default-200 h-7">
        {tabs.length > 1 && (
          <button
            type="button"
            onClick={() => {
              if (activeTab) closeOtherTabs(activeTab.id)
            }}
            className="inline-flex items-center justify-center h-6 px-2 rounded text-xs text-default-500 hover:bg-default-100 transition-colors whitespace-nowrap"
            title={t('tab.closeOthers', '关闭其他')}
          >
            关闭其他
          </button>
        )}
        {tabs.length > 1 && (
          <button
            type="button"
            onClick={() => {
              closeAllTabs()
              const remaining = useTabStore.getState().tabs
              if (remaining.length > 0) {
                navigate({ to: remaining[0].path })
              } else {
                navigate({ to: '/home' })
              }
            }}
            className="inline-flex items-center justify-center h-6 px-2 rounded text-xs text-default-500 hover:bg-default-100 transition-colors whitespace-nowrap"
            title={t('tab.closeAll', '关闭全部')}
          >
            关闭全部
          </button>
        )}
      </div>

      {/* ── 右键浮动菜单 ── */}
      {ctxMenu.visible && (
        <div
          ref={menuRef}
          className="fixed z-[9999] min-w-[140px] bg-background border border-default-200 rounded-lg shadow-lg py-1 text-sm"
          style={{ left: ctxMenu.x, top: ctxMenu.y }}
          role="menu"
          onClick={(e) => e.stopPropagation()}
        >
          <button
            className="w-full text-left px-3 py-1.5 text-default-700 hover:bg-default-100 transition-colors"
            onClick={() => ctxAction('close')}
            role="menuitem"
          >
            关闭标签
          </button>
          <button
            className="w-full text-left px-3 py-1.5 text-default-700 hover:bg-default-100 transition-colors"
            onClick={() => ctxAction('closeOthers')}
            role="menuitem"
          >
            关闭其他
          </button>
          <button
            className="w-full text-left px-3 py-1.5 text-default-700 hover:bg-default-100 transition-colors"
            onClick={() => ctxAction('closeRight')}
            role="menuitem"
          >
            关闭右侧
          </button>
          <div className="border-t border-default-200 my-1" />
          <button
            className="w-full text-left px-3 py-1.5 text-danger hover:bg-danger-50 transition-colors"
            onClick={() => ctxAction('closeAll')}
            role="menuitem"
          >
            关闭全部
          </button>
        </div>
      )}
    </div>
  )
}
