import { describe, it, expect, beforeEach } from 'vitest'
import { useTabStore } from '../tabs'
import type { TabItem } from '../tabs'

function reset() {
  useTabStore.setState({ tabs: [], activeTab: null })
}

beforeEach(reset)

function open(path: string, title = path): TabItem {
  useTabStore.getState().openTab({ title, path })
  return useTabStore.getState().tabs.slice(-1)[0]
}

describe('openTab', () => {
  it('creates a new tab with a derived id and activates it', () => {
    useTabStore.getState().openTab({ title: 'Users', path: '/users' })
    const { tabs, activeTab } = useTabStore.getState()
    expect(tabs).toHaveLength(1)
    expect(tabs[0].id).toBe('users')
    expect(tabs[0].closable).toBe(true)
    expect(tabs[0].pinned).toBe(false)
    expect(activeTab?.id).toBe('users')
  })

  it('derives id from a nested path by stripping slashes', () => {
    useTabStore.getState().openTab({ title: 'Edit Role', path: '/roles/1/edit' })
    expect(useTabStore.getState().tabs[0].id).toBe('roles-1-edit')
  })

  it('falls back to "root" for the index path', () => {
    useTabStore.getState().openTab({ title: 'Home', path: '/' })
    expect(useTabStore.getState().tabs[0].id).toBe('root')
  })

  it('activates an existing tab instead of duplicating', () => {
    open('/users')
    open('/users')
    const { tabs, activeTab } = useTabStore.getState()
    expect(tabs).toHaveLength(1)
    expect(activeTab?.id).toBe('users')
  })
})

describe('closeTab', () => {
  it('removes the tab and activates the previous one', () => {
    const a = open('/a')
    const b = open('/b')
    useTabStore.getState().closeTab(a.id)
    const { tabs, activeTab } = useTabStore.getState()
    expect(tabs.map((t) => t.id)).toEqual([b.id])
    expect(activeTab?.id).toBe(b.id)
  })

  it('refuses to close a pinned tab', () => {
    const t = open('/pinned')
    useTabStore.getState().pinTab(t.id)
    useTabStore.getState().closeTab(t.id)
    expect(useTabStore.getState().tabs.map((x) => x.id)).toEqual([t.id])
  })

  it('is a no-op for an unknown id', () => {
    open('/a')
    useTabStore.getState().closeTab('nope')
    expect(useTabStore.getState().tabs).toHaveLength(1)
  })
})

describe('closeOtherTabs', () => {
  it('keeps the target and any pinned tabs', () => {
    const a = open('/a')
    const b = open('/b')
    const c = open('/c')
    useTabStore.getState().pinTab(c.id)
    useTabStore.getState().closeOtherTabs(b.id)
    const ids = useTabStore.getState().tabs.map((t) => t.id)
    expect(ids).toEqual(expect.arrayContaining([b.id, c.id]))
    expect(ids).not.toContain(a.id)
  })
})

describe('closeAllTabs', () => {
  it('closes everything except pinned tabs', () => {
    const a = open('/a')
    const b = open('/b')
    useTabStore.getState().pinTab(b.id)
    useTabStore.getState().closeAllTabs()
    const { tabs, activeTab } = useTabStore.getState()
    expect(tabs.map((t) => t.id)).toEqual([b.id])
    expect(activeTab?.id).toBe(b.id)
  })
})

describe('closeLeftTabs / closeRightTabs', () => {
  it('closeLeftTabs keeps the target, its right neighbors, and pinned tabs', () => {
    const a = open('/a')
    const b = open('/b')
    const c = open('/c')
    useTabStore.getState().pinTab(a.id)
    useTabStore.getState().closeLeftTabs(b.id)
    const ids = useTabStore.getState().tabs.map((t) => t.id)
    expect(ids).toEqual(expect.arrayContaining([a.id, b.id, c.id]))
  })

  it('closeRightTabs keeps the target, its left neighbors, and pinned tabs', () => {
    const a = open('/a')
    const b = open('/b')
    const c = open('/c')
    useTabStore.getState().pinTab(c.id)
    useTabStore.getState().closeRightTabs(b.id)
    const ids = useTabStore.getState().tabs.map((t) => t.id)
    expect(ids).toEqual(expect.arrayContaining([a.id, b.id, c.id]))
  })

  it('is a no-op when the id is unknown', () => {
    open('/a')
    open('/b')
    useTabStore.getState().closeLeftTabs('nope')
    expect(useTabStore.getState().tabs).toHaveLength(2)
  })
})

describe('pin / unpin', () => {
  it('pinTab marks pinned and non-closable, and keeps it active', () => {
    const t = open('/p')
    useTabStore.getState().pinTab(t.id)
    const stored = useTabStore.getState().tabs[0]
    expect(stored.pinned).toBe(true)
    expect(stored.closable).toBe(false)
    expect(useTabStore.getState().activeTab?.id).toBe(t.id)
  })

  it('unpinTab restores closable and clears pinned', () => {
    const t = open('/p')
    useTabStore.getState().pinTab(t.id)
    useTabStore.getState().unpinTab(t.id)
    const stored = useTabStore.getState().tabs[0]
    expect(stored.pinned).toBe(false)
    expect(stored.closable).toBe(true)
  })
})

describe('setActiveTab', () => {
  it('switches the active tab', () => {
    const a = open('/a')
    const b = open('/b')
    useTabStore.getState().setActiveTab(a)
    expect(useTabStore.getState().activeTab?.id).toBe(a.id)
  })
})
