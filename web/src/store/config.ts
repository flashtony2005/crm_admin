import { create } from 'zustand'
import { persist } from 'zustand/middleware'

export type ThemeMode = 'light' | 'dark' | 'system'
export type SidebarTheme = 'light' | 'dark'
export type LayoutTemplate =
  | 'vertical'
  | 'double-column'
  | 'horizontal'
  | 'side-nav'
  | 'mixed-vertical'
  | 'mixed-double-column'
  | 'content-fullscreen'
export type ContentLayout = 'fluid' | 'fixed'
export type FontSize = 'sm' | 'base' | 'lg' | 'xl' | '2xl'

export interface AppConfig {
  theme: ThemeMode
  sidebarTheme: SidebarTheme
  /** 简单模式（默认）：隐藏本体建模/系统管理/监控等专业入口 */
  advancedMode: boolean
  sidebarWidth: number
  sidebarCollapsedWidth: number
  contentCompact: boolean
  contentPadding: string
  fixedHeader: boolean
  tabbarEnabled: boolean
  tabbarHeight: string
  layoutTemplate: LayoutTemplate
  contentLayout: ContentLayout
  fontSize: FontSize
}

interface ConfigState extends AppConfig {
  updateConfig: (updates: Partial<AppConfig>) => void
  resetConfig: () => void
}

export const defaultConfig: AppConfig = {
  theme: 'light',
  sidebarTheme: 'dark',
  advancedMode: false,
  sidebarWidth: 240,
  sidebarCollapsedWidth: 64,
  contentCompact: false,
  contentPadding: '1rem',
  fixedHeader: true,
  tabbarEnabled: true,
  tabbarHeight: '40px',
  layoutTemplate: 'vertical',
  contentLayout: 'fluid',
  fontSize: 'base',
}

export const useConfigStore = create<ConfigState>()(
  persist(
    (set) => ({
      ...defaultConfig,

      updateConfig: (updates) =>
        set((state) => ({ ...state, ...updates })),

      resetConfig: () => set({ ...defaultConfig }),
    }),
    {
      name: 'app-config',
    },
  ),
)

// 辅助函数：根据配置获取侧边栏宽度类名
export function sidebarWidthClass(collapsed: boolean, config: AppConfig): string {
  if (collapsed) {
    return `w-[${config.sidebarCollapsedWidth}px]`
  }
  return `w-[${config.sidebarWidth}px]`
}

// 辅助函数：获取内容区内边距样式
export function contentPaddingStyle(config: AppConfig): React.CSSProperties {
  if (config.contentCompact) {
    return { padding: '0.25rem' }
  }
  return { padding: config.contentPadding }
}
