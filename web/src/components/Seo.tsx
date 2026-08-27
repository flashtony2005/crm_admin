/**
 * SEO 头部注入工具（P3）：OG / Twitter Card / 基础 meta。
 *
 * 用法：
 *   useSeo({ title, description, image, type, url })
 * 或 <Seo meta={{...}} />（用于非 hook 上下文）。
 *
 * 说明：本 CMS 前端为 SPA（由 vite 提供，后端仅出 API），无服务端渲染，
 * 故 OG/Twitter meta 在客户端注入。对“会执行 JS 的爬虫”与“应用内分享预览”
 * 完整有效；若需让 Facebook/Twitter/Discord 等纯 HTML 爬虫拿到卡片，
 * 需在反向代理/CDN 层按路由预渲染——本模块已预留 type/url/image 字段。
 */
import { useEffect } from 'react'

/** 设置/更新一条 meta 标签（按 attribute+key 幂等） */
export function setMeta(attr: 'property' | 'name', key: string, value: string) {
  if (!value) return
  let el = document.head.querySelector(
    `meta[${attr}="${key}"]`,
  ) as HTMLMetaElement | null
  if (!el) {
    el = document.createElement('meta')
    el.setAttribute(attr, key)
    document.head.appendChild(el)
  }
  el.setAttribute('content', value)
}

export interface SeoMeta {
  title?: string
  description?: string
  image?: string | null
  /** og:type，默认 website */
  type?: string
  /** 规范链接 og:url */
  url?: string
  /** Twitter 卡片类型，缺省按是否有图自动选 summary_large_image / summary */
  twitterCard?: 'summary' | 'summary_large_image'
}

/** 注入 SEO meta 的 hook（组件卸载时恢复 document.title） */
export function useSeo(meta: SeoMeta) {
  const { title, description, image, type, url, twitterCard } = meta
  useEffect(() => {
    const prev = document.title
    if (title) document.title = title
    if (description) {
      setMeta('name', 'description', description)
      setMeta('property', 'og:description', description)
    }
    setMeta('property', 'og:title', title || document.title)
    setMeta('property', 'og:type', type || 'website')
    if (url) setMeta('property', 'og:url', url)
    const img = image && !image.startsWith('data:') ? image : null
    if (img) setMeta('property', 'og:image', img)
    const card = twitterCard || (img ? 'summary_large_image' : 'summary')
    setMeta('name', 'twitter:card', card)
    if (title) setMeta('name', 'twitter:title', title)
    if (description) setMeta('name', 'twitter:description', description)
    if (img) setMeta('name', 'twitter:image', img)
    return () => {
      document.title = prev
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [title, description, image, type, url, twitterCard])
}

/** 组件版封装 */
export function Seo({ meta }: { meta: SeoMeta }) {
  useSeo(meta)
  return null
}
