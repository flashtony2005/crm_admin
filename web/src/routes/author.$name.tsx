/**
 * /author/{name} —— 作者页（P3 内容组织）：按作者名聚合已发布文章。
 * 复用公开内容 API（/api/public/articles?author=），与 sitemap/RSS 同源。
 */
import { createFileRoute, useParams, Link } from '@tanstack/react-router'
import { useEffect, useState } from 'react'
import type { CSSProperties } from 'react'
import { useSeo } from '@/components/Seo'
import { ArrowLeft } from 'lucide-react'
import { SITE_NAME } from './site'
import { buildSrcset } from '../utils/imgSrcset'
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
  featured_image_srcset?: string | null
  published_at?: string | null
  updated_at?: string
  author?: string
  visibility?: string | null
}

function firstImg(html?: string | null): string | null {
  if (!html) return null
  const m = html.match(/<img[^>]+src=["']([^"']+)["']/i)
  return m ? m[1] : null
}
function stripHtml(html?: string | null): string {
  return (html || '').replace(/<[^>]+>/g, ' ').replace(/\s+/g, ' ').trim()
}
function excerpt(a: SiteArticle, max = 120): string {
  const base = (a.summary || '').trim() || stripHtml(a.content)
  return base.length > max ? base.slice(0, max) + '…' : base
}
function fmtDate(iso?: string | null): string {
  if (!iso) return ''
  const d = new Date(iso)
  if (Number.isNaN(d.getTime())) return iso.slice(0, 10)
  const p = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`
}
function keyOf(a: SiteArticle) {
  return a.slug && a.slug.trim() ? a.slug : a.id
}

function AuthorPage() {
  const { name } = useParams({ from: '/author/$name' })
  const author = decodeURIComponent(name)
  const [list, setList] = useState<SiteArticle[]>([])
  const [phase, setPhase] = useState<'loading' | 'ready' | 'error'>('loading')
  const { theme, template } = useSiteTheme()

  useSeo({
    title: `${author} · ${SITE_NAME}`,
    description: `由 ${author} 撰写的所有文章`,
    url: typeof window !== 'undefined' ? window.location.href : undefined,
    type: 'website',
  })

  useEffect(() => {
    let alive = true
    setPhase('loading')
    fetch(`/api/public/articles?author=${encodeURIComponent(author)}`)
      .then(async (res) => {
        const body = (await res.json().catch(() => ({}))) as { data?: SiteArticle[]; error?: string }
        if (!alive) return
        if (!res.ok || body.data === undefined) {
          setPhase('error')
          return
        }
        setList(body.data)
        setPhase('ready')
      })
      .catch(() => alive && setPhase('error'))
    return () => {
      alive = false
    }
  }, [author])

  return (
    <div
      className="min-h-screen antialiased"
      style={{ ...(themeCssVars(theme) as CSSProperties), background: theme.vars.bg, color: theme.vars.text }}
    >
      <header
        className="sticky top-0 z-20 backdrop-blur-md border-b"
        style={{ background: `color-mix(in srgb, ${theme.vars.bg} 84%, transparent)`, borderColor: theme.vars.border }}
      >
        <div className="mx-auto max-w-5xl px-5 sm:px-8 h-14 flex items-center gap-3">
          <Link to="/site" className="flex items-center gap-2 min-w-0">
            <span className="h-6 w-6 rounded-lg grid place-items-center text-[13px] font-black" style={{ background: theme.vars.accent, color: theme.vars.accentText }}>L</span>
            <span className="font-semibold tracking-tight truncate">{SITE_NAME}</span>
          </Link>
        </div>
      </header>

      <div className="mx-auto max-w-5xl px-5 sm:px-8 py-10">
        <Link to="/site" className="inline-flex items-center gap-1.5 text-sm hover:underline mb-6" style={{ color: theme.vars.muted }}>
          <ArrowLeft size={16} /> 返回首页
        </Link>
        <p className="text-[11px] font-semibold tracking-[0.25em] uppercase" style={{ color: theme.vars.accent }}>Author</p>
        <h1 className="text-3xl font-bold tracking-tight mt-1">{author}</h1>
        <p className="text-sm mt-1" style={{ color: theme.vars.muted }}>{list.length} 篇文章</p>

        {phase === 'loading' && (
          <div className="mt-8 space-y-4">
            {[0, 1, 2].map((i) => (
              <div key={i} className="h-28 rounded-2xl animate-pulse" style={{ background: theme.vars.surfaceAlt }} />
            ))}
          </div>
        )}
        {phase === 'error' && (
          <p className="mt-8 text-sm" style={{ color: theme.vars.muted }}>加载失败，请稍后重试。</p>
        )}

        {phase === 'ready' && list.length === 0 && (
          <p className="mt-8 text-sm" style={{ color: theme.vars.muted }}>该作者还没有已发布的文章。</p>
        )}

        {phase === 'ready' && list.length > 0 && (
          <div className={`${template.gridCols} mt-8`}>
            {list.map((a) => {
              const cover = a.featured_image || firstImg(a.content)
              return (
                <Link
                  key={a.id}
                  to="/read/$key"
                  params={{ key: keyOf(a) }}
                  className="group rounded-2xl overflow-hidden border shadow-sm hover:shadow-md hover:-translate-y-0.5 transition-all flex flex-col"
                  style={{ background: theme.vars.surface, borderColor: theme.vars.border }}
                >
                  {template.card === 'cover' ? (
                    <div className="h-36 w-full overflow-hidden relative" style={{ background: theme.vars.surfaceAlt }}>
                      {cover ? (
                        <img
                          src={cover}
                          alt=""
                          loading="lazy"
                          srcSet={buildSrcset(cover) || (a.featured_image_srcset ?? undefined) || undefined}
                          sizes="(max-width: 640px) 100vw, (max-width: 1024px) 50vw, 33vw"
                          className="h-full w-full object-cover group-hover:scale-[1.03] transition-transform duration-300"
                        />
                      ) : (
                        <div className="h-full w-full grid place-items-center text-3xl font-black opacity-25" style={{ color: theme.vars.accent }}>
                          {(a.title || 'A').slice(0, 1)}
                        </div>
                      )}
                    </div>
                  ) : null}
                  <div className="p-4 flex flex-col gap-1.5 flex-1">
                    <h3 className="font-semibold leading-snug line-clamp-2">{a.title}</h3>
                    <p className="text-xs line-clamp-2 flex-1" style={{ color: theme.vars.muted }}>{excerpt(a)}</p>
                    <span className="pt-2 mt-auto text-[11px]" style={{ color: theme.vars.muted }}>
                      {fmtDate(a.published_at || a.updated_at)}
                    </span>
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

export const Route = createFileRoute('/author/$name')({ component: AuthorPage })
