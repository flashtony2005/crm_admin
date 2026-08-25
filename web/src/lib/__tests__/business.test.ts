import { describe, it, expect } from 'vitest'
import { businessMessage, humanizeCapability } from '../business'

describe('businessMessage', () => {
  it('maps auth expiry to business language', () => {
    expect(businessMessage('未认证或 token 已失效，请先登录')).toBe('登录已过期，请重新登录')
  })

  it('maps missing params', () => {
    expect(businessMessage('内部错误: missing input.id')).toBe('缺少必要信息，请补充完整后重试')
  })

  it('maps unknown capability', () => {
    expect(businessMessage('capability User:Delete not found')).toBe(
      '系统还没有这项操作，换个说法试试',
    )
  })

  it('maps permission errors', () => {
    expect(businessMessage('not authorized')).toBe('你还没有执行此操作的权限')
    expect(businessMessage('权限不足')).toBe('你还没有执行此操作的权限')
  })

  it('passes through unknown messages', () => {
    expect(businessMessage('未知错误')).toBe('未知错误')
  })
})

describe('humanizeCapability', () => {
  it('renders business operation names', () => {
    expect(humanizeCapability('User:ListAll')).toBe('查看全部用户')
    expect(humanizeCapability('Article:Approve')).toBe('审核文章')
    expect(humanizeCapability('User:UpdatePassword')).toBe('修改密码')
  })

  it('falls back to raw name without colon', () => {
    expect(humanizeCapability('plain')).toBe('plain')
  })
})
