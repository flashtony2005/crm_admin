/**
 * LightPress 公开站点（P3）—— Ghost 风格的对外阅读页。
 *
 * - 路由：/site（列表）↔ 文章详情（页内状态切换，无二级路由依赖）
 * - 数据：GET /api/articles 取已发布文章（带登录态）；待后端部署后可平滑
 *   切换到免认证的 GET /api/public/articles（headless 消费同构）
 * - 主题：web/src/themes/siteThemes.ts 的 CSS 变量驱动，右上角圆点即时换肤，
 *   localStorage('lp-site-theme') 持久化
 * - SEO：打开详情时注入 OG/Twitter meta 与 document.title
 */
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { CSSProperties } from 'react'
import { createFileRoute } from '@tanstack/react-router'
import { sanitizeHtml } from '../utils/sanitize'
import { ArrowLeft, Rss } from 'lucide-react'
import { SITE_THEMES, loadSiteTheme, saveSiteTheme, themeCssVars } from '../themes/siteThemes'

export const SITE_NAME = 'LightPress'
const SITE_TAGLINE = '专注内容的现代发布平台'

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
  status?: string
  /** 文章级独立 SEO 字段（公开 API 返回，snake_case） */
  meta_title?: string | null
  meta_description?: string | null
}

/** 带登录态的 GET（与 /m 的 mreq 同策略：自管 token，避免 401 全局跳转） */
async function req<T>(path: string): Promise<T & { ok?: boolean; error?: string }> {
  const token = localStorage.getItem('auth_token')
  const res = await fetch(path, {
    headers: token ? { Authorization: `Bearer ${token}` } : {},
  })
  const body = (await res.json().catch(() => ({}))) as Record<string, unknown>
  if (!res.ok || body.ok === false) {
    return { error: String(body.error || `请求失败 (${res.status})`) } as T & { error: string }
  }
  return body as T & { ok?: boolean }
}

// ── 纯工具 ──────────────────────────────────────────────────────────

function parseTags(t?: string | null): string[] {
  if (!t) return []
  return t.split(/[,，、;]/).map((x) => x.trim()).filter(Boolean).slice(0, 6)
}

function firstImg(html?: string | null): string | null {
  if (!html) return null
  const m = html.match(/<img[^>]+src=["']([^"']+)["']/i)
  return m ? m[1] : null
}

function stripHtml(html?: string | null): string {
  return (html || '').replace(/<[^>]+>/g, ' ').replace(/\s+/g, ' ').trim()
}

function escText(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

function excerpt(a: SiteArticle, max = 110): string {
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

function setMeta(attr: 'name' | 'property', key: string, content: string) {
  let el = document.head.querySelector<HTMLMetaElement>(`meta[${attr}="${key}"]`)
  if (!el) {
    el = document.createElement('meta')
    el.setAttribute(attr, key)
    document.head.appendChild(el)
  }
  el.setAttribute('content', content)
}

/** 正文排版样式（主题色经由 --st-* 变量，随换肤联动） */
const PROSE_CSS = `
.lp-prose a{color:var(--st-accent);}
.lp-prose a:hover{text-decoration:underline;}
.lp-prose img{max-width:100%;height:auto;border-radius:14px;margin:1em auto;display:block;}
.lp-prose h1,.lp-prose h2,.lp-prose h3{font-weight:700;line-height:1.35;margin:1.5em 0 .55em;}
.lp-prose h1{font-size:1.45rem}.lp-prose h2{font-size:1.28rem}.lp-prose h3{font-size:1.12rem}
.lp-prose p{margin:.85em 0;line-height:1.95;}
.lp-prose ul,.lp-prose ol{padding-left:1.45em;margin:.8em 0;line-height:1.9;}
.lp-prose ul{list-style:disc}.lp-prose ol{list-style:decimal}
.lp-prose li{margin:.3em 0;}
.lp-prose blockquote{border-left:3px solid var(--st-accent);padding:.1em 0 .1em 1em;color:var(--st-muted);margin:1.1em 0;}
.lp-prose code{background:var(--st-surface-alt);padding:.15em .45em;border-radius:6px;font-size:.88em;}
.lp-prose pre{background:var(--st-surface-alt);padding:1em;border-radius:12px;overflow:auto;}
.lp-prose pre code{background:none;padding:0;}
`

// ── 页面 ────────────────────────────────────────────────────────────

function SitePage() {
  // 主题
  const [themeKey, setThemeKey] = useState<string>(() => loadSiteTheme().key)
  const theme = useMemo(
    () => SITE_THEMES.find((t) => t.key === themeKey) ?? SITE_THEMES[0],
    [themeKey],
  )
  const pickTheme = (k: string) => {
    setThemeKey(k)
    saveSiteTheme(k)
  }

  // 数据
  const [list, setList] = useState<SiteArticle[]>([])
  const [phase, setPhase] = useState<'loading' | 'ready' | 'error'>('loading')
  const [errMsg, setErrMsg] = useState('')
  const [openId, setOpenId] = useState<string | null>(null)
  const [tagFilter, setTagFilter] = useState<string | null>(null)

  const load = useCallback(async () => {
    setPhase('loading')
    // 免认证公开 API：仅已发布文章，含 slug/meta_title/meta_description
    const r = await req<{ data?: SiteArticle[] }>('/api/public/articles')
    if (r.error !== undefined) {
      setErrMsg(r.error)
      setPhase('error')
      return
    }
    const all = Array.isArray(r.data) ? r.data : []
    setList(all)
    setPhase('ready')
  }, [])

  useEffect(() => {
    void load()
  }, [load])

  /** 文章公开 key：slug 优先，空则回退 id（与后端 /read/{key} 解析一致） */
  const keyOf = (a: SiteArticle) => (a.slug && a.slug.trim() ? a.slug! : a.id)

  const current = openId ? list.find((a) => keyOf(a) === openId) : undefined

  /** 标签筛选后的列表（tagFilter 为空则全部） */
  const filtered = tagFilter
    ? list.filter((a) => parseTags(a.tags).includes(tagFilter))
    : list

  /** 点击标签：进入该标签的筛选视图 */
  const pickTag = (tg: string) => {
    setOpenId(null)
    setTagFilter(tg)
  }

  // 详情页 SEO meta 注入 + JSON-LD（Article schema）
  const prevTitle = useRef(document.title)
  useEffect(() => {
    const ld = document.getElementById('lp-jsonld')
    if (!current) {
      ld?.remove()
      return
    }
    const cover = current.featured_image || firstImg(current.content)
    // 文章级 SEO 字段优先，缺失回退标题/摘要
    const seoTitle = (current.meta_title || current.title || '无题').trim()
    const seoDesc = (current.meta_description || excerpt(current, 160)).slice(0, 200)
    document.title = `${seoTitle} · ${SITE_NAME}`
    setMeta('property', 'og:title', `${seoTitle} · ${SITE_NAME}`)
    setMeta('property', 'og:description', seoDesc)
    setMeta('name', 'description', seoDesc)
    if (cover && !cover.startsWith('data:')) setMeta('property', 'og:image', cover)
    setMeta('name', 'twitter:card', cover ? 'summary_large_image' : 'summary')
    // JSON-LD Article（Google 富结果）
    let el = document.getElementById('lp-jsonld') as HTMLScriptElement | null
    if (!el) {
      el = document.createElement('script')
      el.id = 'lp-jsonld'
      el.setAttribute('type', 'application/ld+json')
      document.head.appendChild(el)
    }
    const articleLd = {
      '@context': 'https://schema.org',
      '@type': 'Article',
      headline: seoTitle,
      description: seoDesc,
      datePublished: current.published_at || current.updated_at || undefined,
      dateModified: current.updated_at || undefined,
      author: { '@type': 'Person', name: current.author || '佚名' },
      ...(cover && !cover.startsWith('data:')
        ? { image: cover }
        : {}),
      mainEntityOfPage: {
        '@type': 'WebPage',
        '@id': `${window.location.origin}/read/${keyOf(current)}`,
      },
    }
    el.textContent = JSON.stringify(articleLd)
    return () => {
      document.title = prevTitle.current
    }
  }, [current])

  const coverOf = (a: SiteArticle) => a.featured_image || firstImg(a.content)

  // ── 渲染 ──
  return (
    <div
      style={{ ...(themeCssVars(theme) as CSSProperties), background: theme.vars.bg }}
      className="min-h-screen antialiased"
    >
      <style>{PROSE_CSS}</style>

      {/* 顶栏 */}
      <header
        className="sticky top-0 z-20 backdrop-blur-md border-b"
        style={{
          background: `color-mix(in srgb, ${theme.vars.bg} 84%, transparent)`,
          borderColor: theme.vars.border,
        }}
      >
        <div className="mx-auto max-w-5xl px-5 sm:px-8 h-14 flex items-center gap-3">
          <button
            className="flex items-center gap-2 min-w-0"
            onClick={() => setOpenId(null)}
            title="回到首页"
          >
            <span
              className="h-6 w-6 rounded-lg grid place-items-center text-[13px] font-black shrink-0"
              style={{ background: theme.vars.accent, color: theme.vars.accentText }}
            >
              L
            </span>
            <span className="font-semibold tracking-tight truncate">{SITE_NAME}</span>
            <span
              className="hidden md:inline text-xs truncate pl-2 border-l"
              style={{ color: theme.vars.muted, borderColor: theme.vars.border }}
            >
              {SITE_TAGLINE}
            </span>
          </button>

          <div className="ml-auto flex items-center gap-3">
            {/* 主题切换器 */}
            <div
              className="flex items-center gap-1.5 px-2 py-1.5 rounded-full border"
              style={{ borderColor: theme.vars.border }}
              role="radiogroup"
              aria-label="站点主题"
            >
              {SITE_THEMES.map((t) => (
                <button
                  key={t.key}
                  role="radio"
                  aria-checked={t.key === theme.key}
                  title={`${t.name}主题`}
                  onClick={() => pickTheme(t.key)}
                  className="h-4 w-4 rounded-full transition-transform hover:scale-110"
                  style={{
                    background: t.vars.accent,
                    outline: t.key === theme.key ? `2px solid ${t.vars.text}` : 'none',
                    outlineOffset: 2,
                  }}
                />
              ))}
            </div>
            <a
              href="/rss.xml"
              title="RSS 订阅（后端部署后生效）"
              className="h-8 w-8 grid place-items-center rounded-full border transition-colors hover:bg-black/5"
              style={{ borderColor: theme.vars.border, color: theme.vars.muted }}
            >
              <Rss size={15} />
            </a>
          </div>
        </div>
      </header>

      {/* 加载 / 错误 / 空 */}
      {phase === 'loading' && (
        <div className="mx-auto max-w-5xl px-5 sm:px-8 py-24 space-y-4">
          {[0, 1, 2].map((i) => (
            <div
              key={i}
              className="h-28 rounded-2xl animate-pulse"
              style={{ background: theme.vars.surfaceAlt }}
            />
          ))}
        </div>
      )}
      {phase === 'error' && (
        <div className="mx-auto max-w-md px-5 py-28 text-center space-y-3">
          <p className="text-sm" style={{ color: theme.vars.muted }}>
            加载失败：{errMsg}
          </p>
          <button
            onClick={() => void load()}
            className="px-4 py-2 rounded-full text-sm font-medium"
            style={{ background: theme.vars.accent, color: theme.vars.accentText }}
          >
            重试
          </button>
        </div>
      )}

      {phase === 'ready' && !current && filtered.length === 0 && (
        <div className="mx-auto max-w-md px-5 py-28 text-center space-y-3">
          <p className="text-4xl">🕊️</p>
          <p className="text-sm" style={{ color: theme.vars.muted }}>
            {tagFilter
              ? `标签「${tagFilter}」下还没有已发布的文章。`
              : '还没有已发布的文章。去「内容 → 文章」写一篇并发布吧。'}
          </p>
          {tagFilter ? (
            <button
              onClick={() => setTagFilter(null)}
              className="inline-block px-4 py-2 rounded-full text-sm font-medium"
              style={{ background: theme.vars.accent, color: theme.vars.accentText }}
            >
              查看全部文章
            </button>
          ) : (
            <a
              href="/content/articles"
              className="inline-block px-4 py-2 rounded-full text-sm font-medium"
              style={{ background: theme.vars.accent, color: theme.vars.accentText }}
            >
              打开后台
            </a>
          )}
        </div>
      )}

      {/* 首页 */}
      {phase === 'ready' && !current && filtered.length > 0 && (
        <>
          {/* 标签筛选横幅 */}
          {tagFilter && (
            <div className="mx-auto max-w-5xl px-5 sm:px-8 pt-10">
              <div
                className="flex items-center gap-3 px-5 py-3.5 rounded-2xl border"
                style={{ background: theme.vars.surface, borderColor: theme.vars.border }}
              >
                <span className="text-sm font-semibold">
                  标签：<span style={{ color: theme.vars.accent }}># {tagFilter}</span>
                </span>
                <span className="text-xs" style={{ color: theme.vars.muted }}>
                  {filtered.length} 篇
                </span>
                <button
                  onClick={() => setTagFilter(null)}
                  className="ml-auto px-3 py-1 rounded-full text-xs font-medium hover:opacity-85"
                  style={{ background: theme.vars.surfaceAlt, color: theme.vars.accent }}
                >
                  ✕ 清除筛选
                </button>
              </div>
            </div>
          )}

          {/* Hero：当前视图最新一篇 */}
          <section
            className="px-5 sm:px-8 pt-12 pb-16"
            style={{
              background: `linear-gradient(180deg, ${theme.vars.heroFrom}, ${theme.vars.heroTo})`,
            }}
          >
            <div className="mx-auto max-w-5xl flex flex-col items-start gap-3">
              <p
                className="text-[11px] font-semibold tracking-[0.25em]"
                style={{ color: theme.vars.accent }}
              >
                FEATURED
              </p>
              <h1 className="text-3xl sm:text-[2.6rem] leading-tight font-bold tracking-tight max-w-3xl">
                {filtered[0].title}
              </h1>
              <p
                className="text-sm sm:text-base line-clamp-2 max-w-2xl"
                style={{ color: theme.vars.muted }}
              >
                {excerpt(filtered[0], 150)}
              </p>
              <div className="flex items-center gap-3 mt-2 text-xs" style={{ color: theme.vars.muted }}>
                <span>{filtered[0].author || '佚名'}</span>
                <span>·</span>
                <span>{fmtDate(filtered[0].published_at || filtered[0].updated_at)}</span>
              </div>
              <button
                onClick={() => setOpenId(keyOf(filtered[0]))}
                className="mt-3 px-5 py-2.5 rounded-full text-sm font-semibold shadow-sm hover:opacity-90 transition-opacity"
                style={{ background: theme.vars.accent, color: theme.vars.accentText }}
              >
                阅读全文 →
              </button>
            </div>
          </section>

          {/* 卡片网格 */}
          <main className="mx-auto max-w-5xl px-5 sm:px-8 pb-20 -mt-8 relative z-10">
            <div className="grid gap-5 sm:grid-cols-2 lg:grid-cols-3">
              {filtered.map((a) => {
                const cover = coverOf(a)
                const tags = parseTags(a.tags)
                return (
                  <article
                    key={a.id}
                    onClick={() => setOpenId(keyOf(a))}
                    className="group rounded-2xl overflow-hidden border shadow-sm hover:shadow-md hover:-translate-y-0.5 transition-all cursor-pointer flex flex-col"
                    style={{ background: theme.vars.surface, borderColor: theme.vars.border }}
                  >
                    <div
                      className="h-36 w-full overflow-hidden relative"
                      style={{ background: theme.vars.surfaceAlt }}
                    >
                      {cover ? (
                        <img
                          src={cover}
                          alt=""
                          loading="lazy"
                          className="h-full w-full object-cover group-hover:scale-[1.03] transition-transform duration-300"
                        />
                      ) : (
                        <div
                          className="h-full w-full grid place-items-center text-3xl font-black opacity-25"
                          style={{ color: theme.vars.accent }}
                        >
                          {(a.title || 'A').slice(0, 1)}
                        </div>
                      )}
                    </div>
                    <div className="p-4 flex flex-col gap-1.5 flex-1">
                      <h3 className="font-semibold leading-snug line-clamp-2">{a.title}</h3>
                      <p className="text-xs line-clamp-2 flex-1" style={{ color: theme.vars.muted }}>
                        {excerpt(a)}
                      </p>
                      <div
                        className="flex items-center justify-between gap-2 pt-2 mt-auto text-[11px]"
                        style={{ color: theme.vars.muted }}
                      >
                        <span>
                          {fmtDate(a.published_at || a.updated_at)}
                          {a.author ? ` · ${a.author}` : ''}
                        </span>
                        <span className="flex gap-1 overflow-hidden">
                          {tags.slice(0, 2).map((tg) => (
                            <button
                              key={tg}
                              onClick={(e) => {
                                e.stopPropagation()
                                pickTag(tg)
                              }}
                              className="px-1.5 py-0.5 rounded-full whitespace-nowrap hover:opacity-80"
                              style={{ background: theme.vars.surfaceAlt, color: theme.vars.accent }}
                            >
                              {tg}
                            </button>
                          ))}
                        </span>
                      </div>
                    </div>
                  </article>
                )
              })}
            </div>
          </main>
        </>
      )}

      {/* 详情页 */}
      {phase === 'ready' && current && (
        <article className="mx-auto max-w-[720px] px-5 sm:px-8 pt-8 pb-24">
          <button
            onClick={() => setOpenId(null)}
            className="inline-flex items-center gap-1.5 text-sm mb-8 hover:underline"
            style={{ color: theme.vars.accent }}
          >
            <ArrowLeft size={16} />
            返回列表
          </button>

          <h1 className="text-3xl sm:text-[2.5rem] font-bold leading-tight tracking-tight">
            {current.title}
          </h1>

          <div
            className="flex flex-wrap items-center gap-x-3 gap-y-2 mt-4 text-xs"
            style={{ color: theme.vars.muted }}
          >
            <span
              className="h-6 w-6 rounded-full grid place-items-center text-[11px] font-bold"
              style={{ background: theme.vars.accent, color: theme.vars.accentText }}
            >
              {(current.author || '匿').slice(0, 1)}
            </span>
            <span>{current.author || '佚名'}</span>
            <span>·</span>
            <span>{fmtDate(current.published_at || current.updated_at)}</span>
            {parseTags(current.tags).map((tg) => (
              <button
                key={tg}
                onClick={() => pickTag(tg)}
                className="px-2 py-0.5 rounded-full hover:opacity-80"
                style={{ background: theme.vars.surfaceAlt, color: theme.vars.accent }}
              >
                # {tg}
              </button>
            ))}
          </div>

          {coverOf(current) && (
            <img
              src={coverOf(current)!}
              alt={current.title || ''}
              className="w-full max-h-[440px] object-cover rounded-2xl mt-7 shadow-sm"
            />
          )}

          {current.content ? (
            <div
              className="lp-prose mt-8 text-[15.5px]"
              dangerouslySetInnerHTML={{ __html: sanitizeHtml(current.content) }}
            />
          ) : (
            <p className="mt-8 leading-relaxed" style={{ color: theme.vars.muted }}>
              {current.summary || '（正文为空）'}
            </p>
          )}
        </article>
      )}

      {/* 页脚 */}
      <footer className="border-t" style={{ borderColor: theme.vars.border }}>
        <div
          className="mx-auto max-w-5xl px-5 sm:px-8 h-20 flex flex-wrap items-center gap-x-4 gap-y-1 text-xs"
          style={{ color: theme.vars.muted }}
        >
          <span>
            © {new Date().getFullYear()} {SITE_NAME}
          </span>
          <span>·</span>
          <span>Rust CMS 驱动</span>
          <a href="/rss.xml" className="hover:underline ml-auto">
            RSS
          </a>
          <a href="/sitemap.xml" className="hover:underline">
            Sitemap
          </a>
          <a href="/content/articles" className="hover:underline">
            后台管理
          </a>
        </div>
      </footer>
    </div>
  )
}

export const Route = createFileRoute('/site')({
  component: SitePage,
})
