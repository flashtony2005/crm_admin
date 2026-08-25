/**
 * 公开站点主题系统（P3）
 * - 以 CSS 自定义属性驱动整站换肤：页面根容器挂载 --st-* 变量，
 *   所有区块用 bg-[var(--st-bg)] 等任意值类名引用；
 * - 内置 4 套对标 Ghost 阅读体验的主题：纸白 / 墨黑 / 羊皮纸 / 海蓝；
 * - 选择持久化在 localStorage('lp-site-theme')，刷新保持。
 */
import type { SiteTheme } from './site-theme-types'

/** 全部内置主题（顺序即切换器展示顺序） */
export const SITE_THEMES: SiteTheme[] = [
  {
    key: 'paper',
    name: '纸白',
    dark: false,
    vars: {
      bg: '#faf9f7',
      surface: '#ffffff',
      surfaceAlt: '#f1efe9',
      text: '#191714',
      muted: '#79726a',
      accent: '#b45309',
      accentText: '#ffffff',
      border: '#e7e3db',
      heroFrom: '#fdf3e3',
      heroTo: '#faf9f7',
    },
  },
  {
    key: 'ink',
    name: '墨黑',
    dark: true,
    vars: {
      bg: '#0e0f11',
      surface: '#17191d',
      surfaceAlt: '#1e2126',
      text: '#eceae7',
      muted: '#98a0a8',
      accent: '#7cc4ff',
      accentText: '#0b1016',
      border: '#262a30',
      heroFrom: '#141821',
      heroTo: '#0e0f11',
    },
  },
  {
    key: 'sepia',
    name: '羊皮纸',
    dark: false,
    vars: {
      bg: '#f4ecd8',
      surface: '#fbf6ea',
      surfaceAlt: '#ede2c8',
      text: '#43331f',
      muted: '#8a7550',
      accent: '#a0522d',
      accentText: '#fff8ee',
      border: '#e0d3b4',
      heroFrom: '#f8f1de',
      heroTo: '#f4ecd8',
    },
  },
  {
    key: 'ocean',
    name: '海蓝',
    dark: false,
    vars: {
      bg: '#f0f7fa',
      surface: '#ffffff',
      surfaceAlt: '#e3eff4',
      text: '#12303b',
      muted: '#5b7a86',
      accent: '#0e7490',
      accentText: '#f0fbff',
      border: '#d3e5ec',
      heroFrom: '#e2f3f8',
      heroTo: '#f0f7fa',
    },
  },
]

const LS_KEY = 'lp-site-theme'

/** 读取已保存的主题；无效或缺省回退到第一套 */
export function loadSiteTheme(): SiteTheme {
  try {
    const k = localStorage.getItem(LS_KEY)
    const hit = SITE_THEMES.find((t) => t.key === k)
    if (hit) return hit
  } catch {
    /* localStorage 不可用时静默回退 */
  }
  return SITE_THEMES[0]
}

export function saveSiteTheme(key: string): void {
  try {
    localStorage.setItem(LS_KEY, key)
  } catch {
    /* ignore */
  }
}

/** 转成可挂在根容器 style 上的变量表（消费侧用 as React.CSSProperties 断言） */
export function themeCssVars(t: SiteTheme): Record<string, string> {
  return {
    '--st-bg': t.vars.bg,
    '--st-surface': t.vars.surface,
    '--st-surface-alt': t.vars.surfaceAlt,
    '--st-text': t.vars.text,
    '--st-muted': t.vars.muted,
    '--st-accent': t.vars.accent,
    '--st-accent-text': t.vars.accentText,
    '--st-border': t.vars.border,
    '--st-hero-from': t.vars.heroFrom,
    '--st-hero-to': t.vars.heroTo,
  }
}
