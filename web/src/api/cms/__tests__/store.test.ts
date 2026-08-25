import { beforeEach, describe, expect, it } from 'vitest'

import { collection, resetCmsDemoData } from '../store'

interface Row {
  id: string
  title: string
  updatedAt?: string
}

const seed: Row[] = [
  { id: 'a', title: 'Alpha' },
  { id: 'b', title: 'Beta' },
]

describe('api/cms/store 本地 CRUD 适配器', () => {
  beforeEach(() => resetCmsDemoData())

  it('首次 list 写入种子数据', async () => {
    const api = collection<Row>('rows-test-a', seed)
    const rows = await api.list()
    expect(rows).toHaveLength(2)
    // 再次读取应从持久层恢复，而非重复播种
    expect(await collection<Row>('rows-test-a', seed).list()).toHaveLength(2)
  })

  it('create 分配 id 与时间戳并持久化', async () => {
    const api = collection<Row>('rows-test-b', seed)
    const created = await api.create({ title: 'Gamma' })
    expect(created.id).toBeTruthy()
    expect(created.createdAt).toBeTruthy()
    expect(created.updatedAt).toBeTruthy()

    const reread = await api.get(created.id)
    expect(reread?.title).toBe('Gamma')
  })

  it('update 打补丁并刷新 updatedAt', async () => {
    const api = collection<Row>('rows-test-c', seed)
    const created = await api.create({ title: 'Old' })
    await new Promise((r) => setTimeout(r, 5))
    const updated = await api.update(created.id, { title: 'New' })
    expect(updated.title).toBe('New')
    expect(new Date(updated.updatedAt!).getTime()).toBeGreaterThanOrEqual(
      new Date(created.updatedAt).getTime(),
    )
  })

  it('remove 删除记录；update 不存在的 id 抛错', async () => {
    const api = collection<Row>('rows-test-d', seed)
    const created = await api.create({ title: 'Temp' })
    await api.remove(created.id)
    expect(await api.get(created.id)).toBeUndefined()
    await expect(api.update('missing', { title: 'x' })).rejects.toThrow()
  })

  it('list 按 updatedAt 倒序（最近改动在前）', async () => {
    const api = collection<Row>('rows-test-e', [])
    await new Promise((r) => setTimeout(r, 5))
    const first = await api.create({ title: 'First' })
    await new Promise((r) => setTimeout(r, 10))
    const second = await api.create({ title: 'Second' })
    const rows = await api.list()
    expect(rows[0].id).toBe(second.id)
    expect(rows[rows.length - 1].id).toBe(first.id)
  })
})
