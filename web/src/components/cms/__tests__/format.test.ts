import { describe, expect, it, vi, afterEach } from 'vitest'
import { fmtDate, fmtRelative, fmtSize } from '../format'

afterEach(() => {
  vi.useRealTimers()
})

describe('fmtDate', () => {
  it('非法值与空值回退为 —', () => {
    expect(fmtDate()).toBe('—')
    expect(fmtDate('')).toBe('—')
    expect(fmtDate('not-a-date')).toBe('—')
  })

  it('同年只给月日与时分（列表保持紧凑）', () => {
    vi.useFakeTimers()
    vi.setSystemTime(new Date('2026-06-01T00:00:00'))
    // 用本地时间构造，避开时区差异
    expect(fmtDate(new Date(2026, 8, 23, 15, 42).toISOString())).toBe('9-23 15:42')
  })

  it('跨年必须带年份 —— 否则会员到期日会被读成今天', () => {
    vi.useFakeTimers()
    vi.setSystemTime(new Date('2026-09-23T09:00:00'))
    // 年付会员的到期时间就在明年。原实现会显示「9-23 15:42」，
    // 与今天同日同月，站长会以为今天到期。
    expect(fmtDate(new Date(2027, 8, 23, 15, 42).toISOString())).toBe('2027-9-23 15:42')
    // 去年的历史时间同样带年份
    expect(fmtDate(new Date(2025, 0, 5, 8, 3).toISOString())).toBe('2025-1-05 08:03')
  })
})

describe('fmtSize', () => {
  it('KB 与 MB 分界', () => {
    expect(fmtSize(512)).toBe('512 KB')
    expect(fmtSize(2048)).toBe('2.0 MB')
  })
})

describe('fmtRelative', () => {
  it('近期走相对文案', () => {
    vi.useFakeTimers()
    vi.setSystemTime(new Date('2026-09-23T12:00:00'))
    expect(fmtRelative(new Date(2026, 8, 23, 11, 30).toISOString())).toBe('30 分钟前')
  })
})
