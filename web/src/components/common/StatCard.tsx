import type { ReactNode } from 'react'
import { Sparkline } from '../charts/Sparkline'

interface StatCardProps {
  title: string
  value: ReactNode
  icon: ReactNode
  /** 趋势百分比，正负决定颜色与箭头方向 */
  trend?: { value: number; label?: string }
  /** 底部迷你走势图数据 */
  spark?: number[]
  sparkColor?: string
}

function ArrowUp({ size = 10 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M12 19V5" />
      <path d="m5 12 7-7 7 7" />
    </svg>
  )
}

function ArrowDown({ size = 10 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M12 5v14" />
      <path d="m19 12-7 7-7-7" />
    </svg>
  )
}

export function StatCard({
  title,
  value,
  icon,
  trend,
  spark,
  sparkColor = '#2563EB',
}: StatCardProps) {
  const up = (trend?.value ?? 0) >= 0

  return (
    <div
      className="dash-card p-5 cursor-default"
      style={{
        borderRadius: 10,
        border: '1px solid #E5E7EB',
        boxShadow: '0 1px 3px rgba(0,0,0,0.06), 0 1px 2px rgba(0,0,0,0.04)',
        transition: 'all 0.2s ease-in-out',
        background: '#fff',
      }}
      onMouseEnter={(e) => {
        e.currentTarget.style.transform = 'translateY(-2px)';
        e.currentTarget.style.boxShadow = '0 4px 12px rgba(0,0,0,0.08)';
      }}
      onMouseLeave={(e) => {
        e.currentTarget.style.transform = 'translateY(0)';
        e.currentTarget.style.boxShadow = '0 1px 3px rgba(0,0,0,0.06), 0 1px 2px rgba(0,0,0,0.04)';
      }}
    >
      <div className="flex items-start justify-between">
        <div
          className="w-11 h-11 rounded-xl flex items-center justify-center"
          style={{ background: '#EFF6FF', color: '#2563EB' }}
        >
          {icon}
        </div>
        {trend && (
          <span className={up ? 'dash-trend-up' : 'dash-trend-down'}>
            {up ? <ArrowUp /> : <ArrowDown />}
            {Math.abs(trend.value)}%
          </span>
        )}
      </div>

      <p className="mt-4 dash-stat-label">{title}</p>
      <p className="dash-stat-value text-3xl mt-1">{value}</p>

      <div className="mt-3 h-8 flex items-end">
        {spark && spark.length > 0 ? (
          <Sparkline data={spark} color={sparkColor} width={200} height={32} className="w-full" />
        ) : (
          <span className="text-xs" style={{ color: '#9CA3AF' }}>{trend?.label}</span>
        )}
      </div>
    </div>
  )
}
