import { toast as herouiToast } from '@heroui/react'

/** 轻量提示封装：success / error / info，统一走 HeroUI v3 的 toast。 */
export const toast = {
  success(msg: string) {
    herouiToast.success(msg)
  },
  error(msg: string) {
    herouiToast.danger(msg)
  },
  info(msg: string) {
    herouiToast.info(msg)
  },
}
