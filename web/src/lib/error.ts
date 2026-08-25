import { businessMessage } from './business'

/**
 * 统一错误提取工具（Phase 2.3）
 *
 * 替代散落的 `e.message` / `e?.message` 直接访问，避免 `catch (e: any)` 带来的
 * 类型隐患。所有 catch 统一为 `catch (e: unknown)`，需要错误消息时调用本函数。
 */
export function extractError(e: unknown, fallback = '操作失败'): string {
  if (e instanceof Error) return businessMessage(e.message)
  if (typeof e === 'string') return businessMessage(e)
  if (e && typeof e === 'object' && 'message' in e) {
    const m = (e as { message?: unknown }).message
    if (typeof m === 'string') return businessMessage(m)
  }
  return businessMessage(fallback)
}
