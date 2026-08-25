/**
 * 主题元数据类型（独立文件避免 themes 模块引入 React 类型依赖）。
 */
export interface SiteThemeVars {
  /** 页面底色 */
  bg: string
  /** 卡片表面 */
  surface: string
  /** 次级表面（标签底、代码块等） */
  surfaceAlt: string
  /** 主文本 */
  text: string
  /** 弱化文本 */
  muted: string
  /** 强调色（链接/按钮/高亮） */
  accent: string
  /** 强调色之上的文字 */
  accentText: string
  /** 边框 */
  border: string
  /** 头图区渐变起 */
  heroFrom: string
  /** 头图区渐变止 */
  heroTo: string
}

export interface SiteTheme {
  key: string
  name: string
  dark: boolean
  vars: SiteThemeVars
}
