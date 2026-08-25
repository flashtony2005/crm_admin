import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import { getToken, setToken, clearToken, redirectToLogin } from '../client'

const ORIGINAL_LOCATION = window.location

beforeEach(() => {
  localStorage.clear()
  // jsdom's window.location is read-only; replace with a writable stub so we
  // can assert redirect behaviour without triggering real navigation.
  Object.defineProperty(window, 'location', {
    writable: true,
    configurable: true,
    value: { href: '', pathname: '/dashboard' },
  })
})

afterEach(() => {
  Object.defineProperty(window, 'location', {
    configurable: true,
    value: ORIGINAL_LOCATION,
  })
})

describe('token storage', () => {
  it('returns null when no token is stored', () => {
    expect(getToken()).toBeNull()
  })

  it('round-trips a token through setToken', () => {
    setToken('abc.def.ghi')
    expect(getToken()).toBe('abc.def.ghi')
  })

  it('clearToken removes the stored token', () => {
    setToken('temp')
    clearToken()
    expect(getToken()).toBeNull()
  })
})

describe('redirectToLogin', () => {
  it('clears the auth token', () => {
    setToken('should-be-cleared')
    redirectToLogin()
    expect(getToken()).toBeNull()
  })

  it('resets the persisted auth-store to logged-out state', () => {
    localStorage.setItem(
      'auth-store',
      JSON.stringify({ state: { token: 'x', user: { id: 1 }, isAuthenticated: true }, version: 0 }),
    )
    redirectToLogin()
    const stored = JSON.parse(localStorage.getItem('auth-store') || '{}')
    expect(stored.state.isAuthenticated).toBe(false)
    expect(stored.state.token).toBeNull()
  })

  it('navigates to /login when not already there', () => {
    redirectToLogin()
    expect((window.location as unknown as { href: string }).href).toBe('/login')
  })

  it('does not force a navigation when already on /login', () => {
    Object.defineProperty(window, 'location', {
      writable: true,
      configurable: true,
      value: { href: '/login', pathname: '/login' },
    })
    redirectToLogin()
    expect((window.location as unknown as { href: string }).href).toBe('/login')
  })
})
