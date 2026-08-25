import { describe, it, expect } from 'vitest'
import { cn, paginationRange } from '../utils'

describe('cn', () => {
  it('joins truthy class fragments', () => {
    expect(cn('a', false, 'b', null, undefined, 'c')).toBe('a b c')
  })

  it('returns empty string when all falsy', () => {
    expect(cn(false, null, undefined)).toBe('')
  })
})

describe('paginationRange', () => {
  it('returns full list when total <= maxVisible (sibling*2+5)', () => {
    expect(paginationRange(1, 5)).toEqual([1, 2, 3, 4, 5])
    expect(paginationRange(3, 7)).toEqual([1, 2, 3, 4, 5, 6, 7])
  })

  it('clamps current into [1, total]', () => {
    expect(paginationRange(0, 5)).toEqual([1, 2, 3, 4, 5])
    expect(paginationRange(99, 5)).toEqual([1, 2, 3, 4, 5])
  })

  it('shows right ellipsis near the start page (sibling=1)', () => {
    expect(paginationRange(1, 10, 1)).toEqual([1, 2, 'right', 10])
  })

  it('shows both ellipses for a middle page (sibling=1)', () => {
    expect(paginationRange(5, 20, 1)).toEqual([1, 'left', 4, 5, 6, 'right', 20])
  })

  it('expands window with larger sibling count', () => {
    expect(paginationRange(10, 20, 2)).toEqual([1, 'left', 8, 9, 10, 11, 12, 'right', 20])
  })

  it('treats total < 1 as a single page', () => {
    expect(paginationRange(1, 0)).toEqual([1])
  })
})
