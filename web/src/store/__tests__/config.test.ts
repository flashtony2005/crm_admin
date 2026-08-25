import { describe, it, expect, beforeEach } from 'vitest'
import {
  useConfigStore,
  defaultConfig,
  sidebarWidthClass,
  contentPaddingStyle,
  type AppConfig,
} from '../config'

beforeEach(() => {
  useConfigStore.setState({ ...defaultConfig })
})

describe('updateConfig', () => {
  it('merges a partial update without dropping other fields', () => {
    useConfigStore.getState().updateConfig({ theme: 'dark' })
    const s = useConfigStore.getState()
    expect(s.theme).toBe('dark')
    expect(s.sidebarWidth).toBe(defaultConfig.sidebarWidth)
    expect(s.layoutTemplate).toBe(defaultConfig.layoutTemplate)
  })

  it('merges multiple keys at once', () => {
    useConfigStore.getState().updateConfig({ theme: 'system', fontSize: 'lg' })
    const s = useConfigStore.getState()
    expect(s.theme).toBe('system')
    expect(s.fontSize).toBe('lg')
  })
})

describe('resetConfig', () => {
  it('restores every field to its default after mutation', () => {
    useConfigStore.getState().updateConfig({ theme: 'dark', sidebarWidth: 999, fontSize: '2xl' })
    useConfigStore.getState().resetConfig()
    expect(useConfigStore.getState()).toMatchObject(defaultConfig)
  })
})

describe('sidebarWidthClass', () => {
  const cfg: AppConfig = { ...defaultConfig }

  it('returns the collapsed width class when collapsed', () => {
    expect(sidebarWidthClass(true, cfg)).toBe(`w-[${cfg.sidebarCollapsedWidth}px]`)
  })

  it('returns the full width class when expanded', () => {
    expect(sidebarWidthClass(false, cfg)).toBe(`w-[${cfg.sidebarWidth}px]`)
  })
})

describe('contentPaddingStyle', () => {
  it('uses contentPadding when not compact', () => {
    const cfg: AppConfig = { ...defaultConfig, contentPadding: '2rem' }
    expect(contentPaddingStyle(cfg)).toEqual({ padding: '2rem' })
  })

  it('overrides to a tight padding when contentCompact is set', () => {
    const cfg: AppConfig = { ...defaultConfig, contentCompact: true, contentPadding: '2rem' }
    expect(contentPaddingStyle(cfg)).toEqual({ padding: '0.25rem' })
  })
})
