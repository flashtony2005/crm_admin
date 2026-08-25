// Global test setup for Vitest.
// Registers jest-dom matchers (toBeInTheDocument, toHaveClass, ...).
import '@testing-library/jest-dom/vitest'

// Silence React 19 act() warnings noise in CI-style output.
globalThis.IS_REACT_ACT_ENVIRONMENT = true
