import { describe, it, expect } from 'vitest'
import { extractError } from '../error'

describe('extractError', () => {
  it('提取 Error 实例的 message', () => {
    expect(extractError(new Error('boom'))).toBe('boom')
  })

  it('直接返回字符串', () => {
    expect(extractError('纯字符串错误')).toBe('纯字符串错误')
  })

  it('从带 message 字段的对象中提取', () => {
    expect(extractError({ message: 'obj 错误' })).toBe('obj 错误')
  })

  it('忽略非字符串的 message 字段，回退默认值', () => {
    expect(extractError({ message: 123 })).toBe('操作失败')
  })

  it('null / undefined 回退默认', () => {
    expect(extractError(null)).toBe('操作失败')
    expect(extractError(undefined)).toBe('操作失败')
  })

  it('数字等非错误值回退默认', () => {
    expect(extractError(404)).toBe('操作失败')
  })

  it('支持自定义 fallback', () => {
    expect(extractError(undefined, '自定义提示')).toBe('自定义提示')
  })
})
