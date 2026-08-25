import { create } from 'zustand'

export interface TabItem {
  id: string
  title: string
  path: string
  closable: boolean
  pinned: boolean
}

interface TabState {
  tabs: TabItem[]
  activeTab: TabItem | null
  openTab: (tab: Omit<TabItem, 'id' | 'closable' | 'pinned'>) => void
  closeTab: (id: string) => void
  closeOtherTabs: (id: string) => void
  closeAllTabs: () => void
  closeLeftTabs: (id: string) => void
  closeRightTabs: (id: string) => void
  setActiveTab: (tab: TabItem) => void
  pinTab: (id: string) => void
  unpinTab: (id: string) => void
}

export const useTabStore = create<TabState>((set, get) => ({
  tabs: [],
  activeTab: null,

  openTab: (tab) => {
    const { tabs } = get()
    const id = tab.path.replace(/\//g, '-').replace(/^-+|-+$/g, '') || 'root'
    const existing = tabs.find((t) => t.id === id)
    if (existing) {
      set({ activeTab: existing })
    } else {
      const newTab: TabItem = {
        ...tab,
        id,
        closable: true,
        pinned: false,
      }
      set({ tabs: [...tabs, newTab], activeTab: newTab })
    }
  },

  closeTab: (id) => {
    const { tabs, activeTab } = get()
    const tab = tabs.find((t) => t.id === id)
    if (tab?.pinned || !tab?.closable) return
    const newTabs = tabs.filter((t) => t.id !== id)
    const newActive =
      activeTab?.id === id
        ? newTabs[newTabs.length - 1] ?? null
        : activeTab
    set({ tabs: newTabs, activeTab: newActive })
  },

  closeOtherTabs: (id) => {
    const { tabs } = get()
    const newTabs = tabs.filter((t) => t.id === id || t.pinned)
    const active = newTabs.find((t) => t.id === id) ?? null
    set({ tabs: newTabs, activeTab: active })
  },

  closeAllTabs: () => {
    const { tabs } = get()
    const newTabs = tabs.filter((t) => t.pinned)
    set({ tabs: newTabs, activeTab: newTabs[0] ?? null })
  },

  closeLeftTabs: (id) => {
    const { tabs } = get()
    const idx = tabs.findIndex((t) => t.id === id)
    if (idx === -1) return
    const newTabs = tabs.filter((t, i) => i >= idx || t.pinned)
    set({ tabs: newTabs })
  },

  closeRightTabs: (id) => {
    const { tabs } = get()
    const idx = tabs.findIndex((t) => t.id === id)
    if (idx === -1) return
    const newTabs = tabs.filter((t, i) => i <= idx || t.pinned)
    set({ tabs: newTabs })
  },

  setActiveTab: (tab) => set({ activeTab: tab }),

  pinTab: (id) => {
    const { tabs, activeTab } = get()
    const newTabs = tabs.map((t) =>
      t.id === id ? { ...t, pinned: true, closable: false } : t,
    )
    const newActive =
      activeTab?.id === id ? { ...activeTab, pinned: true, closable: false } : activeTab
    set({ tabs: newTabs, activeTab: newActive })
  },

  unpinTab: (id) => {
    const { tabs, activeTab } = get()
    const newTabs = tabs.map((t) =>
      t.id === id ? { ...t, pinned: false, closable: true } : t,
    )
    const newActive =
      activeTab?.id === id ? { ...activeTab, pinned: false, closable: true } : activeTab
    set({ tabs: newTabs, activeTab: newActive })
  },
}))
