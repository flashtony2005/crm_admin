import { describe, it, expect } from 'vitest'
import { canDelete, canEdit, canCreate, WRITE_CAPABLE_ROLES } from '../permissions'

describe('permissions', () => {
  it('grants write to super_admin / admin / tenant_admin', () => {
    for (const r of WRITE_CAPABLE_ROLES) {
      expect(canDelete(r)).toBe(true)
      expect(canEdit(r)).toBe(true)
      expect(canCreate(r)).toBe(true)
    }
  })

  it('denies write to minimal-permission roles', () => {
    for (const r of ['editor', 'viewer', 'auditor', 'user', '']) {
      expect(canDelete(r)).toBe(false)
      expect(canEdit(r)).toBe(false)
      expect(canCreate(r)).toBe(false)
    }
  })

  it('treats null/undefined as no permission', () => {
    expect(canDelete(null)).toBe(false)
    expect(canDelete(undefined)).toBe(false)
  })
})
