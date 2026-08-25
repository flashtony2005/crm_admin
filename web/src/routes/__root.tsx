import { createRootRoute, Outlet, useLocation, useNavigate } from '@tanstack/react-router'
import { useEffect } from 'react'
import { useAuthStore } from '../store/auth'
import { useLanguageStore } from '../store/language'
import { MainLayout } from '../components/layout/MainLayout'
import { getToken } from '../api/client'

/** 改密重定向（渲染期导航需放在组件 effect 里） */
function ChangeGate({ to }: { to: string }) {
  const nav = useNavigate()
  useEffect(() => { void nav({ to }) }, [nav, to])
  return null
}

function RootLayout() {
  const isAuthenticated = useAuthStore((s) => s.isAuthenticated)
  const restoreAuth = useAuthStore((s) => s.restoreAuth)
  const restoreLanguage = useLanguageStore((s) => s.restoreLanguage)

  // 应用启动时恢复认证状态和语言偏好
  useEffect(() => {
    restoreAuth().then(() => {
      restoreLanguage()
    })
  }, [])

  const pathname = useLocation().pathname
  // /m 前缀 = 移动快捷页；/f 前缀 = 公开表单页。均不套主布局
  if (pathname.startsWith('/m') || pathname.startsWith('/f')) {
    return <Outlet />
  }
  // 双重检查：Zustand store 标记 + localStorage token 二者都失效才视为未登录
  // 避免 persist store 残留导致已登出后仍包 MainLayout
  const hasToken = !!getToken()
  const reallyAuthenticated = isAuthenticated && hasToken

  // A1 强制改密：标记未清除时，除改密页外的一切页面都重定向
  if (reallyAuthenticated && useAuthStore.getState().user?.mustChangePassword && pathname !== '/change-password') {
    return <ChangeGate to='/change-password' />
  }

  // 未登录时直接渲染 Outlet（登录/注册页），不使用主布局
  if (!reallyAuthenticated) {
    return <Outlet />
  }

  return <MainLayout />
}

export const Route = createRootRoute({
  component: RootLayout,
})
