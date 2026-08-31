/**
 * 共享动效令牌
 *
 * 移植自 beui.dev（MIT，作者 Saurabh），适配本项目技术栈：
 *   - 动画库由 `motion/react` 换成本项目已有的 `framer-motion@12`
 *   - 色彩令牌由 shadcn 的 CSS 变量（bg-card / text-foreground）换成
 *     本项目 Tailwind 色板（bg-white / text-gray-900）
 *
 * 说明：默认曲线刻意不用 ease-in / ease-out —— 它们太弱。
 * DRAWER 是完全阻尼的长曲线（无回弹），比 spring 更适合开合面板。
 */

/** 出场：快速冲出后长尾减速。 */
export const EASE_OUT = [0.16, 1, 0.3, 1] as const

/** 进出场对称，用于位置互换类动画。 */
export const EASE_IN_OUT = [0.77, 0, 0.175, 1] as const

/** 抽屉/底部弹层开合：单一干净减速，不回弹。 */
export const EASE_DRAWER = [0.32, 0.72, 0, 1] as const

/**
 * 手势面自身就是控件（滑块、拖拽手柄）时用：任何输入都禁止选中。
 */
export const TOUCH_GESTURE_CLASS = 'select-none [-webkit-touch-callout:none]'

/**
 * 手势面包裹的是业务内容（列表行、弹层头部）时用：
 * 只在粗指针（触摸）下禁止选中，鼠标仍可正常选中复制文本。
 *
 * `-webkit-touch-callout:none` 阻止 iOS 长按弹出菜单；
 * `pointer-coarse:select-none` 阻止触摸长按选中。
 */
export const TOUCH_GESTURE_CONTENT_CLASS =
  '[-webkit-touch-callout:none] pointer-coarse:select-none'

/** 按下反馈：按钮等可点击面。 */
export const SPRING_PRESS = {
  type: 'spring',
  stiffness: 500,
  damping: 30,
  mass: 0.6,
} as const

/** 内容互换：控件内的文字/图标换位。 */
export const SPRING_SWAP = {
  type: 'spring',
  stiffness: 460,
  damping: 30,
  mass: 0.55,
} as const

/** 浮层入场：弹窗、底部弹层。 */
export const SPRING_PANEL = {
  type: 'spring',
  stiffness: 420,
  damping: 40,
  mass: 0.5,
} as const

/** 共享布局滑动：指示条、药丸在位置间形变。 */
export const SPRING_LAYOUT = {
  type: 'spring',
  stiffness: 360,
  damping: 32,
  mass: 0.6,
} as const
