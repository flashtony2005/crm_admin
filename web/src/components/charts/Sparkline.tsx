import { useId } from 'react'

interface SparklineProps {
  data: number[]
  width?: number
  height?: number
  color?: string
  className?: string
  showArea?: boolean
}

/**
 * 纯 SVG 迷你折线图（sparkline），零依赖。
 * 用 viewBox + preserveAspectRatio="none" 横向铺满，stroke 用 non-scaling-stroke 保持线宽恒定。
 */
export function Sparkline({
  data,
  width = 120,
  height = 36,
  color = '#6366f1',
  className = '',
  showArea = true,
}: SparklineProps) {
  const id = useId().replace(/:/g, '')
  if (!data || data.length === 0) return null

  const max = Math.max(...data)
  const min = Math.min(...data)
  const range = max - min || 1
  const pad = 3

  const pts = data.map((v, i) => {
    const x = (i / (data.length - 1)) * (width - pad * 2) + pad
    const y = height - pad - ((v - min) / range) * (height - pad * 2)
    return [x, y] as const
  })

  const line = pts
    .map((p, i) => `${i === 0 ? 'M' : 'L'}${p[0].toFixed(2)} ${p[1].toFixed(2)}`)
    .join(' ')
  const area = `${line} L${(width - pad).toFixed(2)} ${(height - pad).toFixed(2)} L${pad.toFixed(2)} ${(height - pad).toFixed(2)} Z`

  return (
    <svg
      width={width}
      height={height}
      viewBox={`0 0 ${width} ${height}`}
      preserveAspectRatio="none"
      className={className}
      aria-hidden
    >
      <defs>
        <linearGradient id={`spark-${id}`} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor={color} stopOpacity="0.08" />
          <stop offset="100%" stopColor={color} stopOpacity="0" />
        </linearGradient>
      </defs>
      {showArea && <path d={area} fill={`url(#spark-${id})`} stroke="none" />}
      <path
        d={line}
        fill="none"
        stroke={color}
        strokeWidth={2}
        strokeLinecap="round"
        strokeLinejoin="round"
        vectorEffect="non-scaling-stroke"
      />
    </svg>
  )
}
