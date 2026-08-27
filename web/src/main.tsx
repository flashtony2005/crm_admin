import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { RouterProvider, createRouter } from '@tanstack/react-router'
import { Toast, toast } from '@heroui/react'
import { setToastFunction } from './api/client'
import { routeTree } from './routeTree.generated'
import './i18n'
import './index.css'

const queryClient = new QueryClient()

const router = createRouter({
  routeTree,
  context: { queryClient },
  defaultPreload: 'intent',
})

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router
  }
}

function App() {
  // 设置 Toast 函数，用于 API 客户端显示权限错误
  setToastFunction((message: string) => {
    toast.danger(message)
  })

  return (
    <>
      <RouterProvider router={router} />
      <Toast.Provider placement="top" />
    </>
  )
}

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <App />
    </QueryClientProvider>
  </StrictMode>,
)

// PWA：注册 Service Worker（移动端「添加到主屏幕」后独立运行）
if ('serviceWorker' in navigator) {
  window.addEventListener('load', () => {
    navigator.serviceWorker.register('/sw.js').catch(() => {
      /* 注册失败不影响主流程 */
    })
  })
}
