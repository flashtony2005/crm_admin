import { useTranslation } from 'react-i18next'
import { Button, Popover, ListBox } from '@heroui/react'

interface RoleOption {
  role_key: string
  role_name?: string
}

interface RoleMultiSelectProps {
  value: string[]
  onChange: (v: string[]) => void
  options: RoleOption[]
  placeholder?: string
  label?: string
}

export function RoleMultiSelect({
  value,
  onChange,
  options,
  placeholder,
  label,
}: RoleMultiSelectProps) {
  const { t } = useTranslation()
  const cur = value ?? []
  const nameOf = (k: string) => options.find((r) => r.role_key === k)?.role_name || k

  const remove = (k: string) => {
    onChange(cur.filter((x) => x !== k))
  }

  return (
    <div className="space-y-2">
      <Popover>
        <Popover.Trigger>
          <Button
            variant="outline"
            fullWidth
            data-testid="role-select-trigger"
            className="h-auto min-h-[2.75rem] py-2 px-3 justify-start items-center gap-2 text-left font-normal"
          >
            {cur.length === 0 ? (
              <span className="text-os-text-muted">
                {placeholder ?? t('table.selectRoles', '请选择角色')}
              </span>
            ) : (
              <div
                className="flex flex-wrap gap-2 items-center"
                onClick={(e) => e.stopPropagation()}
              >
                {cur.map((k) => (
                  <span
                    key={k}
                    className="inline-flex items-center gap-1 pl-2.5 pr-1 py-1 rounded-full text-xs bg-indigo-50 border border-indigo-200 text-indigo-700 shadow-sm"
                  >
                    <span className="max-w-[140px] truncate">{nameOf(k)}</span>
                    <button
                      type="button"
                      aria-label={t('table.removeRole', '移除角色')}
                      title={t('table.removeRole', '移除角色')}
                      className="flex items-center justify-center w-5 h-5 rounded-full hover:bg-indigo-100 text-indigo-500 hover:text-indigo-900 transition-colors"
                      onClick={(e) => {
                        e.stopPropagation()
                        remove(k)
                      }}
                    >
                      <svg
                        width="12"
                        height="12"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        strokeWidth="2.5"
                        strokeLinecap="round"
                      >
                        <path d="M18 6 6 18M6 6l12 12" />
                      </svg>
                    </button>
                  </span>
                ))}
              </div>
            )}
            <span className="text-xs opacity-60 ml-auto shrink-0">▾</span>
          </Button>
        </Popover.Trigger>
        <Popover.Content
          placement="bottom start"
          className="w-72 p-1 max-h-72 overflow-auto"
        >
          <ListBox
            aria-label={label ?? t('table.roles', '角色')}
            selectionMode="multiple"
            selectedKeys={new Set(cur) as any}
            onSelectionChange={(keys: any) => {
              const next =
                keys === 'all'
                  ? options.map((r) => r.role_key)
                  : Array.from(keys as Set<string>)
              onChange(next)
            }}
          >
            {options.map((r) => {
              const selected = cur.includes(r.role_key)
              return (
                <ListBox.Item
                  key={r.role_key}
                  id={r.role_key}
                  data-testid={`role-opt-${r.role_key}`}
                  textValue={r.role_name || r.role_key}
                  className="flex items-center justify-between"
                >
                  <div className="flex items-center justify-between w-full gap-3">
                    <span className={selected ? 'font-medium text-indigo-700' : ''}>
                      {r.role_name || r.role_key}
                    </span>
                    {selected && (
                      <svg
                        width="16"
                        height="16"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        strokeWidth="2.5"
                        strokeLinecap="round"
                        strokeLinejoin="round"
                        className="text-indigo-600 shrink-0"
                      >
                        <path d="M20 6 9 17l-5-5" />
                      </svg>
                    )}
                  </div>
                </ListBox.Item>
              )
            })}
          </ListBox>
        </Popover.Content>
      </Popover>

      {cur.length > 0 && (
        <p className="text-xs text-os-text-muted">
          {t('table.selectedRoles', { count: cur.length })}
        </p>
      )}
    </div>
  )
}
