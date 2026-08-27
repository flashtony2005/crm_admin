/**
 * 公开站点布局模板（站点级，发布者在后台统一选定，对所有访客生效）。
 * 与 siteThemes（配色）正交：theme 管「颜色」，template 管「版式」。
 */
export type SiteTemplateKey = 'default' | 'magazine' | 'minimal'

export interface SiteTemplate {
  key: SiteTemplateKey
  name: string
  desc: string
  /** 文章列表网格的 Tailwind 类名 */
  gridCols: string
  /** 卡片外观：cover=带封面大卡，text=纯文字列表 */
  card: 'cover' | 'text'
  /** 首页 Hero：compact=常规，large=大字头条，none=不展示 */
  hero: 'compact' | 'large' | 'none'
}

export const SITE_TEMPLATES: SiteTemplate[] = [
  {
    key: 'default',
    name: '默认',
    desc: '三栏卡片网格',
    gridCols: 'grid gap-5 sm:grid-cols-2 lg:grid-cols-3',
    card: 'cover',
    hero: 'compact',
  },
  {
    key: 'magazine',
    name: '杂志',
    desc: '双栏大卡 · 首篇头条',
    gridCols: 'grid gap-6 sm:grid-cols-2',
    card: 'cover',
    hero: 'large',
  },
  {
    key: 'minimal',
    name: '极简',
    desc: '单列文字列表 · 无封面',
    gridCols: 'grid gap-3',
    card: 'text',
    hero: 'none',
  },
]

export function templateByKey(k?: string | null): SiteTemplate {
  return SITE_TEMPLATES.find((t) => t.key === k) ?? SITE_TEMPLATES[0]
}
