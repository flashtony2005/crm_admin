/**
 * /read/{key} —— P3 可深链的文章详情页（key = slug 或 id，与 sitemap/RSS 约定一致）。
 * 使文章具备独立 URL，配合 OG/Twitter Card + JSON-LD，社交分享卡片完整。
 *
 * 付费墙（P4）：直接调用单篇详情端点 /api/public/articles/{key}。
 *  - 文章 visibility=public：始终返回正文
 *  - visibility=members / paid：未授权访问返回 HTTP 402（{ locked:true, preview, visibility }）
 *    → 本页据此渲染「解锁」区块（摘要预览 + 登录/订阅 CTA）
 *  - 会员登录后携带 localStorage 'member_token' 作为 Bearer 令牌，已订阅会员可越过付费门槛
 */
import { createFileRoute, useParams, Link } from '@tanstack/react-router'
import { useEffect, useRef, useState, type CSSProperties } from 'react'
import { useSiteTheme } from '../themes/useSiteTheme'
import { themeCssVars } from '../themes/siteThemes'
import type { SiteTheme } from '../themes/site-theme-types'
import { useSeo } from '@/components/Seo'
import { sanitizeHtml } from '../utils/sanitize'
import { getMemberToken } from '../api/cms'
import { buildSrcset } from '../utils/imgSrcset'

interface SiteArticle {
  id: string
  title?: string
  slug?: string
  summary?: string
  content?: string
  tags?: string | null
  featured_image?: string | null
  featured_image_srcset?: string | null
  published_at?: string | null
  updated_at?: string
  author?: string
  meta_title?: string | null
  meta_description?: string | null
  visibility?: string | null
  featured?: boolean | null
  scheduled_at?: string | null
  canonical_url?: string | null
}

interface LockInfo {
  visibility: string
  message: string
  preview?: SiteArticle
}

function firstImg(html?: string | null): string | null {
  if (!html) return null
  const m = html.match(/<img[^>]+src=["']([^"']+)["']/i)
  return m ? m[1] : null
}

function excerpt(a: { summary?: string; content?: string }, max = 160): string {
  const txt = (a.summary || a.content || '').replace(/<[^>]+>/g, ' ').replace(/\s+/g, ' ').trim()
  return txt.slice(0, max)
}

function ReadPage() {
  const { key } = useParams({ from: '/read/$key' })
  const [article, setArticle] = useState<SiteArticle | null>(null)
  const [lock, setLock] = useState<LockInfo | null>(null)
  const [phase, setPhase] = useState<'loading' | 'ready' | 'locked' | 'missing'>('loading')
  const ldRef = useRef<HTMLScriptElement | null>(null)
  const { theme, siteTitle } = useSiteTheme()
  const brand = siteTitle || 'LightPress'

  useEffect(() => {
    let alive = true
    setPhase('loading')
    setArticle(null)
    setLock(null)

    // 会员令牌（Bearer）可选携带：已登录会员可越过付费墙
    const memberToken = getMemberToken()
    const headers: Record<string, string> = {}
    if (memberToken) headers['Authorization'] = `Bearer ${memberToken}`

    fetch(`/api/public/articles/${encodeURIComponent(key)}`, { headers })
      .then(async (res) => {
        if (!alive) return
        const body = (await res.json().catch(() => ({}))) as {
          ok?: boolean
          locked?: boolean
          visibility?: string
          error?: string
          preview?: SiteArticle
          data?: SiteArticle
        }
        if (res.status === 404) {
          setPhase('missing')
          return
        }
        if (res.status === 402 && body.locked) {
          setLock({
            visibility: body.visibility || 'members',
            message: body.error || '该内容需要解锁',
            preview: body.preview,
          })
          setPhase('locked')
          // 预览已展示：仍记为一次阅读事件（便于统计热门内容）
          if (body.preview?.id) {
            void fetch('/api/public/track', {
              method: 'POST',
              headers: { 'Content-Type': 'application/json' },
              body: JSON.stringify({ type: 'article_view', refId: body.preview.id, refKey: key }),
            }).catch(() => {})
          }
          return
        }
        if (res.status === 200 && body.ok && body.data) {
          setArticle(body.data)
          setPhase('ready')
          // 阅读事件埋点（事件生态 / 统计看板数据源），静默失败不影响阅读
          void fetch('/api/public/track', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ type: 'article_view', refId: body.data.id, refKey: key }),
          }).catch(() => {})
          return
        }
        setPhase('missing')
      })
      .catch(() => {
        if (alive) setPhase('missing')
      })

    return () => {
      alive = false
    }
  }, [key])

  // 锁定态仍展示预览元数据（标题/封面），便于社交分享卡片与 SEO
  const meta: SiteArticle | null = article ?? (lock?.preview ?? null)
  const cover = meta ? meta.featured_image || firstImg(meta.content) : null
  const seoTitle = meta ? (meta.meta_title || meta.title || '无题').trim() : 'LightPress'
  const seoDesc = meta ? (meta.meta_description || excerpt(meta)).slice(0, 200) : ''

  // OG / Twitter Card 注入（顶层调用，依赖随文章加载更新）
  useSeo({
    title: `${seoTitle} · ${brand}`,
    description: seoDesc,
    image: cover,
    type: meta ? 'article' : 'website',
    url: typeof window !== 'undefined' ? window.location.href : undefined,
  })

  // 规范链接 canonicallink rel="canonical"：自定义优先，否则用当前 URL
  useEffect(() => {
    if (!meta) {
      document.head.querySelector('link[rel="canonical"]')?.remove()
      return
    }
    let el = document.head.querySelector<HTMLLinkElement>('link[rel="canonical"]')
    if (!el) {
      el = document.createElement('link')
      el.rel = 'canonical'
      document.head.appendChild(el)
    }
    el.href = (meta.canonical_url?.trim() || (typeof window !== 'undefined' ? window.location.href : '')) as string
  }, [meta])

  // JSON-LD Article（仅已解锁正文时输出，避免暴露付费正文结构）
  useEffect(() => {
    if (!article) return
    let el = document.getElementById('lp-jsonld') as HTMLScriptElement | null
    if (!el) {
      el = document.createElement('script')
      el.id = 'lp-jsonld'
      el.setAttribute('type', 'application/ld+json')
      document.head.appendChild(el)
    }
    ldRef.current = el
    el.textContent = JSON.stringify({
      '@context': 'https://schema.org',
      '@type': 'Article',
      headline: seoTitle,
      description: seoDesc,
      datePublished: article.published_at || article.updated_at || undefined,
      dateModified: article.updated_at || undefined,
      author: { '@type': 'Person', name: article.author || '佚名' },
      ...(cover && !cover.startsWith('data:') ? { image: cover } : {}),
      mainEntityOfPage: {
        '@type': 'WebPage',
        '@id': `${typeof window !== 'undefined' ? window.location.origin : ''}/read/${keyOf(article)}`,
      },
    })
    return () => {
      ldRef.current?.remove()
      ldRef.current = null
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [article, key, seoTitle, seoDesc, cover])

  if (phase === 'loading') {
    return (
      <div
        style={{ ...(themeCssVars(theme) as CSSProperties), background: theme.vars.bg, color: theme.vars.muted }}
        className="min-h-screen grid place-items-center"
      >
        加载中…
      </div>
    )
  }

  if (phase === 'missing' || (!article && !lock)) {
    return (
      <div
        style={{ ...(themeCssVars(theme) as CSSProperties), background: theme.vars.bg, color: theme.vars.muted }}
        className="min-h-screen grid place-items-center text-center"
      >
        <div>
          <p className="text-4xl mb-2">🕊️</p>
          <p>文章不存在或尚未发布。</p>
          <Link to="/site" style={{ color: theme.vars.accent }} className="hover:underline text-sm">
            返回首页
          </Link>
        </div>
      </div>
    )
  }

  if (phase === 'locked' && lock) {
    return <UnlockBlock lock={lock} theme={theme} />
  }

  const html = sanitizeHtml(article!.content || '')

  return (
    <div
      style={{ ...(themeCssVars(theme) as CSSProperties), background: theme.vars.bg, color: theme.vars.text }}
      className="min-h-screen antialiased"
    >
      <article className="mx-auto max-w-3xl px-5 sm:px-8 py-12">
        <Link to="/site" style={{ color: theme.vars.muted }} className="text-sm hover:underline">
          ← 返回首页
        </Link>
        <h1 className="text-4xl font-extrabold mt-4 leading-tight">
          {article!.meta_title || article!.title || '无题'}
        </h1>
        <p className="text-sm mt-3" style={{ color: theme.vars.muted }}>
          {article!.author ? (
            <>
              <Link to="/author/$name" params={{ name: encodeURIComponent(article!.author) }} className="hover:underline">
                {article!.author}
              </Link>
              {' · '}
            </>
          ) : null}
          {article!.published_at || article!.updated_at}
        </p>
        {cover && (
          <img
            src={cover}
            alt={article!.title || ''}
            srcSet={buildSrcset(cover) || (article!.featured_image_srcset ?? undefined) || undefined}
            sizes="(max-width: 768px) 100vw, 768px"
            className="w-full rounded-2xl my-6 object-cover"
          />
        )}
        <div
          className="prose max-w-none mt-4 leading-relaxed"
          dangerouslySetInnerHTML={{ __html: html }}
        />
      </article>
    </div>
  )
}

/** 付费墙解锁区块：展示免费摘要预览 + 登录/订阅 CTA */
function UnlockBlock({ lock, theme }: { lock: LockInfo; theme: SiteTheme }) {
  const p = lock.preview
  const isPaid = lock.visibility === 'paid'
  const cover = p?.featured_image || (p ? firstImg(p.content) : null)
  const title = p?.meta_title || p?.title || '会员专享内容'
  const teaser = p?.meta_description || excerpt(p ?? { summary: '' })

  return (
    <div
      style={{ ...(themeCssVars(theme) as CSSProperties), background: theme.vars.bg, color: theme.vars.text }}
      className="min-h-screen antialiased"
    >
      <div className="mx-auto max-w-2xl px-5 sm:px-8 py-16">
        <Link to="/site" style={{ color: theme.vars.muted }} className="text-sm hover:underline">
          ← 返回首页
        </Link>

        <div className="mt-8 rounded-3xl p-8 shadow-sm" style={{ border: `1px solid ${theme.vars.border}`, background: theme.vars.surface }}>
          <div className="flex items-center gap-3">
            <span className="grid place-items-center w-12 h-12 rounded-2xl text-2xl" style={{ background: theme.vars.surfaceAlt }}>
              {isPaid ? '💎' : '🔒'}
            </span>
            <div>
              <p className="text-xs font-medium tracking-wide uppercase" style={{ color: theme.vars.accent }}>
                {isPaid ? '付费会员专享' : '会员专享'}
              </p>
              <h1 className="text-2xl font-extrabold leading-tight mt-0.5">{title}</h1>
            </div>
          </div>

          {cover && (
            <img
              src={cover}
              alt={title}
              className="w-full rounded-2xl my-6 object-cover max-h-64"
            />
          )}

          {teaser && (
            <p className="leading-relaxed line-clamp-3" style={{ color: theme.vars.muted }}>{teaser}</p>
          )}

          <div className="my-6 border-t border-dashed" style={{ borderColor: theme.vars.border }} />

          <p className="text-sm" style={{ color: theme.vars.muted }}>{lock.message}</p>

          <div className="mt-5 flex flex-wrap gap-3">
            <Link
              to="/membership"
              className="inline-flex items-center justify-center rounded-xl px-5 py-2.5 text-sm font-semibold transition"
              style={{ background: theme.vars.accent, color: theme.vars.accentText }}
            >
              {isPaid ? '订阅解锁' : '登录 / 注册会员'}
            </Link>
            <Link
              to="/site"
              className="inline-flex items-center justify-center rounded-xl px-5 py-2.5 text-sm font-medium transition"
              style={{ border: `1px solid ${theme.vars.border}`, color: theme.vars.muted }}
            >
              返回首页
            </Link>
          </div>

          <p className="mt-4 text-xs" style={{ color: theme.vars.muted }}>
            已有会员？登录后刷新本页即可自动解锁。
          </p>
        </div>
      </div>
    </div>
  )
}

function keyOf(a: SiteArticle) {
  return a.slug && a.slug.trim() ? a.slug : a.id
}

export const Route = createFileRoute('/read/$key')({ component: ReadPage })
