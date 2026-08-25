/**
 * PWA Service Worker —— 移动审批台 /m
 * 策略：
 *  - 预缓存应用壳 + 图标（install）
 *  - /api/* 与鉴权请求：绝不缓存，始终走网络（保证审批数据实时）
 *  - Vite 开发模块（/@、/src/、.vite、?t=）：网络优先且不缓存，避免破坏 HMR
 *  - 页面导航：network-first，离线时回退到已缓存的 /m 壳
 *  - 其它同源静态资源：stale-while-revalidate（先返回缓存，后台更新）
 */
const CACHE = 'cms-pwa-v1'
const SHELL = [
  '/',
  '/m',
  '/index.html',
  '/manifest.webmanifest',
  '/icons/icon-192.png',
  '/icons/icon-512.png',
  '/icons/icon-maskable-512.png',
]

self.addEventListener('install', (event) => {
  event.waitUntil(
    caches.open(CACHE).then((cache) => cache.addAll(SHELL)).catch(() => {})
  )
  self.skipWaiting()
})

self.addEventListener('activate', (event) => {
  event.waitUntil(
    caches.keys().then((keys) =>
      Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k)))
    )
  )
  self.clients.claim()
})

function isDev(url) {
  return (
    url.pathname.startsWith('/@') ||
    url.pathname.startsWith('/src/') ||
    url.pathname.includes('/node_modules/.vite') ||
    url.searchParams.has('t')
  )
}

self.addEventListener('fetch', (event) => {
  const req = event.request
  if (req.method !== 'GET') return

  const url = new URL(req.url)
  if (url.origin !== self.location.origin) return

  // 实时数据接口：永远走网络，不缓存
  if (url.pathname.startsWith('/api/')) return

  // 开发期模块：网络优先、不缓存
  if (isDev(url)) return

  // 页面导航：network-first，离线回退到 /m 壳
  if (req.mode === 'navigate') {
    event.respondWith(
      fetch(req).catch(() =>
        caches.match('/m').then((r) => r || caches.match('/index.html'))
      )
    )
    return
  }

  // 静态资源：stale-while-revalidate
  event.respondWith(
    caches.match(req).then((cached) => {
      const network = fetch(req)
        .then((res) => {
          if (res && res.status === 200 && res.type === 'basic') {
            const copy = res.clone()
            caches.open(CACHE).then((c) => c.put(req, copy))
          }
          return res
        })
        .catch(() => cached)
      return cached || network
    })
  )
})
