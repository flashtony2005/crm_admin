import { useEffect } from 'react'
import { useRouterState } from '@tanstack/react-router'
import { useTabStore } from '../store/tabs'

const menuTitleMap: Record<string, string> = {
  '/dashboard': '仪表盘',
  '/users': '用户管理',
  '/roles': '角色管理',
  '/profile': '个人中心',
  '/settings': '系统设置',
}

export function useTabOpener() {
  const location = useRouterState({ select: (s) => s.location })
  const { openTab } = useTabStore()

  useEffect(() => {
    const path = location.pathname
    // 白名单路径不打开 tab
    if (['/login', '/register', '/'].includes(path)) return

    const title = menuTitleMap[path] ?? path.split('/').pop() ?? path
    openTab({ title, path })
  }, [location.pathname, openTab])
}
