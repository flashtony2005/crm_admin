import { create } from 'zustand'
import i18n from '../i18n'

type Language = 'zh-CN' | 'en'

const LANG_CACHE_KEY = 'user:lang'

interface LanguageState {
  language: Language
  setLanguage: (lang: Language) => void
  restoreLanguage: () => Promise<void>
}

// 本地 localStorage — 不依赖后端
async function syncLanguageToCache(lang: Language): Promise<void> {
  try {
    localStorage.setItem(LANG_CACHE_KEY, JSON.stringify({ lang }))
  } catch { /* silent */ }
}

async function loadLanguageFromCache(): Promise<Language | null> {
  try {
    const raw = localStorage.getItem(LANG_CACHE_KEY)
    if (raw) {
      const parsed = JSON.parse(raw)
      if (parsed?.lang && ['zh-CN', 'en'].includes(parsed.lang)) {
        return parsed.lang as Language
      }
    }
  } catch { /* silent */ }
  return null
}

export const useLanguageStore = create<LanguageState>()((set, _get) => ({
  language: (() => {
    try {
      const raw = localStorage.getItem('app-config')
      if (raw) {
        const parsed = JSON.parse(raw)
        if (parsed?.state?.language) return parsed.state.language
      }
    } catch { /* ignore */ }
    return 'zh-CN'
  })(),

  setLanguage: (lang: Language) => {
    i18n.changeLanguage(lang)
    set({ language: lang })
    // 同步到 app-config（持久化到 localStorage）
    try {
      const raw = localStorage.getItem('app-config')
      if (raw) {
        const parsed = JSON.parse(raw)
        parsed.state.language = lang
        localStorage.setItem('app-config', JSON.stringify(parsed))
      }
    } catch { /* ignore */ }
    // 同步到服务端 cache_store
    syncLanguageToCache(lang)
  },

  restoreLanguage: async () => {
    // 优先使用服务端缓存的语言偏好
    const cachedLang = await loadLanguageFromCache()
    if (cachedLang) {
      i18n.changeLanguage(cachedLang)
      set({ language: cachedLang })
      // 更新本地
      try {
        const raw = localStorage.getItem('app-config')
        if (raw) {
          const parsed = JSON.parse(raw)
          parsed.state.language = cachedLang
          localStorage.setItem('app-config', JSON.stringify(parsed))
        }
      } catch { /* ignore */ }
    }
  },
}))
