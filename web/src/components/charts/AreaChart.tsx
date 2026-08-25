import { useId } from 'react'

interface AreaChartProps {
  data: number[]
  labels?: string[]
  height?: number
  color?: string
}

/**
 * 纯 SVG 面积折线图，零依赖。
 * 仅保留横向网格线，移除垂直网格线。
 * 填充区域透明度降低。
 */
export function AreaChart({
  data,
  labels,
  height = 220,
  color = '#2563EB',
}: AreaChartProps) {
  const id = useId().replace(/:/g, '')
  const W = 1000
  const H = 300

  if (!data || data.length === 0) return null

  const max = Math.max(...data)
  const min = Math.min(...data)
  const range = max - min || 1
  const padY = 16

  const pts = data.map((v, i) => {
    const x = (i / (data.length - 1)) * W
    const y = H - padY - ((v - min) / range) * (H - padY * 2)
    return [x, y] as const
  })

  const line = pts
    .map((p, i) => `${i === 0 ? 'M' : 'L'}${p[0].toFixed(1)} ${p[1].toFixed(1)}`)
    .join(' ')
  const area = `${line} L${W} ${H} L0 ${H} Z`
  const gridLines = [0.25, 0.5, 0.75].map((f) => H * f)

  return (
    <div>
      <svg
        width="100%"
        height={height}
        viewBox={`0 0 ${W} ${H}`}
        preserveAspectRatio="none"
        className="overflow-visible"
        style={{ color: '#D1D5DB' }}
      >
        <defs>
          <linearGradient id={`area-${id}`} x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor={color} stopOpacity="0.08" />
            <stop offset="100%" stopColor={color} stopOpacity="0" />
          </linearGradient>
        </defs>
        {/* 仅横向网格线，无垂直网格线 */}
        {gridLines.map((y, i) => (
          <line
            key={i}
            x1={0} y1={y} x2={W} y2={y}
            stroke="currentColor"
            strokeOpacity="0.12"
            strokeWidth={1}
            vectorEffect="non-scaling-stroke"
          />
        ))}
        <path d={area} fill={`url(#area-${id})`} />
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
      {labels && labels.length > 0 && (
        <div className="flex justify-between mt-2 px-1" style={{ color: '#9CA3AF', fontSize: '0.75rem' }}>
          {labels.map((l, i) => (
            <span key={i}>{l}</span>
          ))}
        </div>
      )}
    </div>
  )
}
