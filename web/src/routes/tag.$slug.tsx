/**
 * /tag/{slug} —— P3 独立语义化标签路由（替代原站内的 ?tag= 页内筛选）。
 * 已发布文章按标签过滤；带 OG/Twitter Card meta 注入。
 */
import { createFileRoute, useParams, Link } from '@tanstack/react-router'
import { useEffect, useState } from 'react'
import type { CSSProperties } from 'react'
import { useSeo } from '@/components/Seo'
import { useSiteTheme } from '../themes/useSiteTheme'
import { themeCssVars } from '../themes/siteThemes'

interface SiteArticle {
  id: string
  title?: string
  slug?: string
  summary?: string
  content?: string
  tags?: string | null
  featured_image?: string | null
  published_at?: string | null
  updated_at?: string
  author?: string
}

function parseTags(t?: string | null): string[] {
  if (!t) return []
  return t.split(/[,，、;]/).map((x) => x.trim()).filter(Boolean).slice(0, 6)
}

async function fetchArticles(): Promise<SiteArticle[]> {
  const res = await fetch('/api/public/articles')
  const body = (await res.json().catch(() => ({}))) as { data?: SiteArticle[] }
  return Array.isArray(body.data) ? body.data : []
}

function keyOf(a: SiteArticle) {
  return a.slug && a.slug.trim() ? a.slug : a.id
}

function TagPage() {
  const { slug } = useParams({ from: '/tag/$slug' })
  const tag = decodeURIComponent(slug || '')
  const [all, setAll] = useState<SiteArticle[]>([])
  const [phase, setPhase] = useState<'loading' | 'ready'>('loading')
  const { theme, template } = useSiteTheme()

  useEffect(() => {
    let alive = true
    setPhase('loading')
    fetchArticles()
      .then((list) => {
        if (!alive) return
        setAll(list)
        setPhase('ready')
      })
      .catch(() => alive && setPhase('ready'))
    return () => {
      alive = false
    }
  }, [tag])

  const list = all.filter((a) => parseTags(a.tags).includes(tag))

  useSeo({
    title: `#${tag} · LightPress`,
    description: `标签「${tag}」下的全部已发布文章。`,
    url: typeof window !== 'undefined' ? window.location.href : undefined,
    type: 'website',
  })

  return (
    <div
      className="min-h-screen antialiased"
      style={{ ...(themeCssVars(theme) as CSSProperties), background: theme.vars.bg, color: theme.vars.text }}
    >
      <div className="mx-auto max-w-5xl px-5 sm:px-8 py-10">
        <Link to="/site" className="text-sm hover:underline" style={{ color: theme.vars.muted }}>
          ← 返回首页
        </Link>
        <h1 className="text-3xl font-extrabold mt-4">
          标签 <span style={{ color: theme.vars.accent }}># {tag}</span>
        </h1>
        <p className="text-sm mt-1" style={{ color: theme.vars.muted }}>{list.length} 篇文章</p>

        {phase === 'loading' ? (
          <p className="text-sm py-20 text-center" style={{ color: theme.vars.muted }}>加载中…</p>
        ) : list.length === 0 ? (
          <p className="text-sm py-20 text-center" style={{ color: theme.vars.muted }}>
            标签「{tag}」下还没有已发布的文章。
          </p>
        ) : (
          <div className={`${template.gridCols} mt-8`}>
            {list.map((a) => {
              const cover = a.featured_image || null
              return (
                <Link
                  key={a.id}
                  to="/read/$key"
                  params={{ key: keyOf(a) }}
                  className="block rounded-2xl border overflow-hidden hover:shadow-md transition-shadow"
                  style={{ background: theme.vars.surface, borderColor: theme.vars.border }}
                >
                  {template.card === 'cover' && cover && (
                    <img src={cover} alt={a.title || ''} className="h-40 w-full object-cover" loading="lazy" />
                  )}
                  <div className="p-4">
                    <h2 className="font-semibold leading-snug">{a.title || '无题'}</h2>
                    {a.summary && (
                      <p className="text-sm mt-2 line-clamp-3" style={{ color: theme.vars.muted }}>{a.summary}</p>
                    )}
                    <p className="text-xs mt-3" style={{ color: theme.vars.muted }}>
                      {a.author ? `${a.author} · ` : ''}
                      {a.published_at || a.updated_at}
                    </p>
                  </div>
                </Link>
              )
            })}
          </div>
        )}
      </div>
    </div>
  )
}

export const Route = createFileRoute('/tag/$slug')({ component: TagPage })
