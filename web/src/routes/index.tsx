import { createFileRoute, redirect } from '@tanstack/react-router'
import { getToken } from '../api/client'

export const Route = createFileRoute('/')({
  beforeLoad: () => {
    if (getToken()) throw redirect({ to: '/home' })
    throw redirect({ to: '/login' })
  },
})
