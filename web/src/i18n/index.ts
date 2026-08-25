import i18n from 'i18next'
import { initReactI18next } from 'react-i18next'
import LanguageDetector from 'i18next-browser-languagedetector'
import zhCN from './locales/zh-CN.json'
import en from './locales/en.json'

// 从 localStorage 读取已保存的语言
function getStoredLang(): string {
  try {
    const raw = localStorage.getItem('app-config')
    if (raw) {
      const parsed = JSON.parse(raw)
      if (parsed?.state?.language) return parsed.state.language
    }
  } catch { /* ignore */ }
  return 'zh-CN'
}

const storedLang = getStoredLang()

i18n
  .use(LanguageDetector)
  .use(initReactI18next)
  .init({
    resources: {
      'zh-CN': { translation: zhCN },
      en: { translation: en },
    },
    lng: storedLang,
    fallbackLng: 'zh-CN',
    interpolation: {
      escapeValue: false, // React already escapes
    },
    detection: {
      order: ['localStorage', 'navigator'],
      lookupLocalStorage: 'i18nextLng',
      caches: ['localStorage'],
    },
  })

export default i18n
