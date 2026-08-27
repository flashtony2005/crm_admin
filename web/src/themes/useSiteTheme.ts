/**
 * 站点级主题 + 模板 Hook（公开站点各页共用）。
 *
 * - 拉取 `/api/public/site` 拿发布者设定的默认主题/模板/品牌；
 * - 允许访客用 localStorage 本地覆盖主题（与既有 lp-site-theme 兼容），
 *   覆盖仅对该访客生效，不影响其他访客看到的「站点默认」。
 */
import { useEffect, useState } from 'react'
import { SITE_THEMES } from './siteThemes'
import { templateByKey, type SiteTemplate } from './siteTemplates'
import type { SiteTheme } from './site-theme-types'

const LS_THEME = 'lp-site-theme'

export interface SiteConfig {
  theme: string
  template: string
  siteTitle: string
  siteTagline: string
}

export interface UseSiteTheme {
  /** 最终生效主题（站点默认，或被访客本地覆盖） */
  theme: SiteTheme
  /** 站点默认主题 key（用于 UI 标注「站点默认」） */
  siteThemeKey: string
  /** 最终生效布局模板 */
  template: SiteTemplate
  /** 站点品牌名 */
  siteTitle: string
  /** 站点标语 */
  siteTagline: string
  /** 设置访客本地主题覆盖；传 null 恢复为站点默认 */
  setLocalTheme: (key: string | null) => void
}

export function useSiteTheme(): UseSiteTheme {
  const [cfg, setCfg] = useState<SiteConfig>({
    theme: 'paper',
    template: 'default',
    siteTitle: 'LightPress',
    siteTagline: '专注内容的现代发布平台',
  })
  const [localTheme, setLocalThemeState] = useState<string | null>(() => {
    try {
      return localStorage.getItem(LS_THEME) || null
    } catch {
      return null
    }
  })

  useEffect(() => {
    let alive = true
    fetch('/api/public/site')
      .then((r) => r.json())
      .then((b) => {
        if (alive && b && b.ok && b.data) {
          setCfg({
            theme: b.data.theme || 'paper',
            template: b.data.template || 'default',
            siteTitle: b.data.siteTitle || 'LightPress',
            siteTagline: b.data.siteTagline || '',
          })
        }
      })
      .catch(() => {})
    return () => {
      alive = false
    }
  }, [])

  const siteTheme = SITE_THEMES.find((t) => t.key === cfg.theme) ?? SITE_THEMES[0]
  const theme =
    localTheme
      ? SITE_THEMES.find((t) => t.key === localTheme) ?? siteTheme
      : siteTheme
  const template = templateByKey(cfg.template)

  const setLocalTheme = (key: string | null) => {
    try {
      if (key) localStorage.setItem(LS_THEME, key)
      else localStorage.removeItem(LS_THEME)
    } catch {
      /* ignore */
    }
    setLocalThemeState(key)
  }

  return {
    theme,
    siteThemeKey: cfg.theme,
    template,
    siteTitle: cfg.siteTitle,
    siteTagline: cfg.siteTagline,
    setLocalTheme,
  }
}
