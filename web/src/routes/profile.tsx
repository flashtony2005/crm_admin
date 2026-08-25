import { createFileRoute } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'
import { Card } from '@heroui/react'
import { PageContainer } from '../components/layout/PageContainer'

function ProfilePage() {
  const { t } = useTranslation()

  return (
    <PageContainer title={t('profile.title')} subtitle={t('profile.subtitle')}>
      <Card className="p-8 text-center">
        <p className="text-default-500">{t('profile.placeholder')}</p>
      </Card>
    </PageContainer>
  )
}

export const Route = createFileRoute('/profile')({
  component: ProfilePage,
})
