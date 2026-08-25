import type { ReactNode } from 'react'
import { Card } from '@heroui/react'

interface PageContainerProps {
  title?: string
  subtitle?: string
  actions?: ReactNode
  children: ReactNode
  className?: string
  contentClassName?: string
  showHeader?: boolean
}

export function PageContainer({
  title,
  subtitle,
  actions,
  children,
  className = '',
  contentClassName = '',
  showHeader = true,
}: PageContainerProps) {
  return (
    <Card className={`w-full h-full ${className}`}>
      {showHeader && (title || actions) && (
        <Card.Header>
          <div className="flex items-center justify-between w-full border-b border-default-200 pb-4">
            <div className="flex flex-col gap-1">
              {title && (
                <h1 className="text-xl font-semibold text-foreground">{title}</h1>
              )}
              {subtitle && (
                <p className="text-sm text-default-500">{subtitle}</p>
              )}
            </div>
            {actions && <div className="flex items-center gap-2">{actions}</div>}
          </div>
        </Card.Header>
      )}
      <Card.Content className={contentClassName}>{children}</Card.Content>
    </Card>
  )
}
