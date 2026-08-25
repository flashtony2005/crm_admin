interface DonutSegment {
  label: string
  value: number
  color: string
}

interface DonutChartProps {
  data: DonutSegment[]
  size?: number
  thickness?: number
  centerLabel?: string
  centerValue?: string | number
}

/**
 * 纯 SVG 环形图（donut），零依赖。
 * 圆环宽度 16px，中间分两行展示总用户数 + 标签。
 */
export function DonutChart({
  data,
  size = 160,
  thickness = 16,
  centerLabel,
  centerValue,
}: DonutChartProps) {
  const total = data.reduce((s, d) => s + d.value, 0) || 1
  const r = (size - thickness) / 2
  const c = 2 * Math.PI * r
  let offset = 0

  return (
    <div>
      <div
        className="relative inline-flex items-center justify-center"
        style={{ width: size, height: size }}
      >
        <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`} className="-rotate-90">
          <circle
            cx={size / 2} cy={size / 2} r={r}
            fill="none"
            stroke="#E5E7EB"
            strokeWidth={thickness}
          />
          {data.map((seg, i) => {
            const len = (seg.value / total) * c
            const el = (
              <circle
                key={i}
                cx={size / 2} cy={size / 2} r={r}
                fill="none"
                stroke={seg.color}
                strokeWidth={thickness}
                strokeDasharray={`${len} ${c - len}`}
                strokeDashoffset={-offset}
                strokeLinecap="butt"
              />
            )
            offset += len
            return el
          })}
        </svg>
        {(centerValue !== undefined || centerLabel) && (
          <div className="absolute inset-0 flex flex-col items-center justify-center text-center">
            {centerValue !== undefined && (
              <span className="text-2xl font-bold" style={{ color: '#111827', lineHeight: 1.2 }}>{centerValue}</span>
            )}
            {centerLabel && (
              <span className="text-xs mt-1" style={{ color: '#9CA3AF' }}>{centerLabel}</span>
            )}
          </div>
        )}
      </div>

      <div className="mt-4 space-y-2">
        {data.map((seg, i) => (
          <div key={i} className="flex items-center justify-between text-sm">
            <span className="flex items-center gap-2" style={{ color: '#4B5563' }}>
              <span className="w-2.5 h-2.5 rounded-full" style={{ backgroundColor: seg.color }} />
              {seg.label}
            </span>
            <span className="font-medium" style={{ color: '#111827' }}>
              {Math.round((seg.value / total) * 100)}%
            </span>
          </div>
        ))}
      </div>
    </div>
  )
}
