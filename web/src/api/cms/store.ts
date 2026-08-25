/**
 * CMS 本地持久化适配器（Mock-first）。
 *
 * 设计：所有集合走同一个 collection<T>() 抽象 ——
 * - 现在：localStorage 持久化 + 模拟延迟，让 Phase 1 UI 完全可用、可演示；
 * - 将来：Axum + SeaORM + Turso 后端就绪后，仅需把本文件换成 HTTP 适配器，
 *   services 层与所有页面零改动。
 */

export interface CrudService<T extends { id: string }> {
  list(): Promise<T[]>
  get(id: string): Promise<T | undefined>
  create(input: Omit<T, 'id' | 'createdAt' | 'updatedAt'> & { id?: string }): Promise<T>
  update(id: string, patch: Partial<T>): Promise<T>
  remove(id: string): Promise<void>
}

const PREFIX = 'cms_demo_v1'
/** 模拟网络延迟（ms），让 loading 态真实可见 */
const LATENCY = 120

function nowIso(): string {
  return new Date().toISOString()
}

function genId(): string {
  return `${Date.now().toString(36)}${Math.random().toString(36).slice(2, 7)}`
}

function readCollection<T>(key: string): T[] {
  try {
    const raw = localStorage.getItem(`${PREFIX}:${key}`)
    return raw ? (JSON.parse(raw) as T[]) : []
  } catch {
    return []
  }
}

function writeCollection<T>(key: string, items: T[]): void {
  try {
    localStorage.setItem(`${PREFIX}:${key}`, JSON.stringify(items))
  } catch {
    // 存储满/隐私模式下降级为内存态，不阻断 UI
  }
}

/**
 * 创建一个基于 localStorage 的 CRUD 集合服务。
 * @param key   集合名（如 'articles'）
 * @param seed  首次访问时写入的种子数据
 */
export function collection<T extends { id: string }>(
  key: string,
  seed: T[],
): CrudService<T> {
  const storageKey = `${PREFIX}:${key}`

  const ensureSeeded = (): T[] => {
    if (!localStorage.getItem(storageKey)) writeCollection(key, seed)
    return readCollection<T>(key)
  }

  const delay = () => new Promise((r) => setTimeout(r, LATENCY))

  return {
    async list() {
      await delay()
      // updatedAt 倒序：最近改动在前
      return ensureSeeded().sort((a, b) =>
        String((b as { updatedAt?: string }).updatedAt ?? '').localeCompare(
          String((a as { updatedAt?: string }).updatedAt ?? ''),
        ),
      )
    },

    async get(id) {
      await delay()
      return ensureSeeded().find((x) => x.id === id)
    },

    async create(input) {
      await delay()
      const items = ensureSeeded()
      const record = {
        ...input,
        id: input.id ?? genId(),
        createdAt: nowIso(),
        updatedAt: nowIso(),
      } as unknown as T
      writeCollection(key, [record, ...items])
      return record
    },

    async update(id, patch) {
      await delay()
      const items = ensureSeeded()
      const idx = items.findIndex((x) => x.id === id)
      if (idx === -1) throw new Error(`记录不存在: ${id}`)
      const next = { ...items[idx], ...patch, updatedAt: nowIso() } as T
      items[idx] = next
      writeCollection(key, items)
      return next
    },

    async remove(id) {
      await delay()
      writeCollection(key, ensureSeeded().filter((x) => x.id !== id))
    },
  }
}

/** 清空全部本地演示数据（开发调试用；挂到 window 便于控制台调用） */
export function resetCmsDemoData(): void {
  try {
    Object.keys(localStorage)
      .filter((k) => k.startsWith(PREFIX))
      .forEach((k) => localStorage.removeItem(k))
  } catch {
    // ignore
  }
}
