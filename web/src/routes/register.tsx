import { createFileRoute, useNavigate, Link } from '@tanstack/react-router'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Button, Card, Input, Label } from '@heroui/react'
import { register } from '../api/auth'

import { extractError } from '../lib/error'

function RegisterPage() {
  const navigate = useNavigate()
  const { t } = useTranslation()
  const [form, setForm] = useState({ username: '', password: '', nickname: '' })
  const [error, setError] = useState('')
  const [loading, setLoading] = useState(false)

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    setError('')
    setLoading(true)
    try {
      await register(form)
      navigate({ to: '/login' })
    } catch (err: unknown) {
      setError(extractError(err, t('auth.registerFailed')))
    } finally {
      setLoading(false)
    }
  }

  const fieldLabels: Record<string, string> = {
    username: t('register.username'),
    nickname: t('register.nickname'),
    password: t('register.password'),
  }

  return (
    <div className="min-h-screen flex items-center justify-center bg-default-50 p-4">
      <Card className="w-full max-w-sm p-8">
        <Card.Header>
          <Card.Title className="text-2xl text-center w-full">{t('register.title')}</Card.Title>
        </Card.Header>
        <Card.Content>
          <form onSubmit={handleSubmit} className="space-y-4">
            {error && (
              <p className="text-danger text-sm text-center">{error}</p>
            )}
            {(['username', 'nickname', 'password'] as const).map((field) => (
              <div key={field} className="space-y-1.5">
                <Label>{fieldLabels[field]}</Label>
                <Input
                  type={field === 'password' ? 'password' : 'text'}
                  fullWidth
                  value={form[field]}
                  onChange={(e) => setForm({ ...form, [field]: e.target.value })}
                  required
                />
              </div>
            ))}
            <Button
              type="submit"
              variant="primary"
              fullWidth
              isDisabled={loading}
            >
              {loading ? t('auth.registering') : t('auth.register')}
            </Button>
            <p className="text-sm text-center text-default-500">
              {t('auth.hasAccount')}<Link to="/login" className="text-primary ml-1">{t('auth.goLogin')}</Link>
            </p>
          </form>
        </Card.Content>
      </Card>
    </div>
  )
}

export const Route = createFileRoute('/register')({
  component: RegisterPage,
})
