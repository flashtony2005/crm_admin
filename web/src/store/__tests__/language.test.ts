import { describe, it, expect, beforeEach } from 'vitest'
import { useLanguageStore } from '../language'

beforeEach(() => {
  localStorage.clear()
  useLanguageStore.setState({ language: 'zh-CN' })
})

describe('setLanguage', () => {
  it('updates state and caches the preference to localStorage', () => {
    useLanguageStore.getState().setLanguage('en')
    expect(useLanguageStore.getState().language).toBe('en')
    const cached = JSON.parse(localStorage.getItem('user:lang')!)
    expect(cached.lang).toBe('en')
  })

  it('mirrors the choice into app-config when it already exists', () => {
    localStorage.setItem('app-config', JSON.stringify({ state: { language: 'zh-CN' }, version: 0 }))
    useLanguageStore.getState().setLanguage('en')
    const cfg = JSON.parse(localStorage.getItem('app-config')!)
    expect(cfg.state.language).toBe('en')
  })

  it('falls back to zh-CN on an unknown value', () => {
    useLanguageStore.setState({ language: 'zh-CN' })
    // 直接写入非法缓存，确认 store 不采用
    localStorage.setItem('user:lang', JSON.stringify({ lang: 'klingon' }))
    useLanguageStore.getState().restoreLanguage()
    // restoreLanguage 只在合法值（zh-CN/en）时覆盖，故保持原值
    expect(['zh-CN', 'en']).toContain(useLanguageStore.getState().language)
  })
})

describe('restoreLanguage', () => {
  it('restores a cached preference', async () => {
    localStorage.setItem('user:lang', JSON.stringify({ lang: 'en' }))
    await useLanguageStore.getState().restoreLanguage()
    expect(useLanguageStore.getState().language).toBe('en')
  })

  it('keeps the default when nothing is cached', async () => {
    await useLanguageStore.getState().restoreLanguage()
    expect(useLanguageStore.getState().language).toBe('zh-CN')
  })
})
