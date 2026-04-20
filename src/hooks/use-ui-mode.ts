import { useCallback, useEffect, useState } from 'react'

export type UiMode = 'simple' | 'advanced'

const STORAGE_KEY = 'celestial-ui-mode'
const CHANGE_EVENT = 'celestial-ui-mode-change'

const isUiMode = (value: unknown): value is UiMode =>
  value === 'simple' || value === 'advanced'

const readUiMode = (): UiMode => {
  if (typeof window === 'undefined') return 'simple'

  const stored = window.localStorage.getItem(STORAGE_KEY)
  return isUiMode(stored) ? stored : 'simple'
}

export const useUiMode = () => {
  const [uiMode, setUiMode] = useState<UiMode>(readUiMode)

  const updateMode = useCallback((nextMode: UiMode) => {
    setUiMode(nextMode)

    if (typeof window === 'undefined') return
    window.localStorage.setItem(STORAGE_KEY, nextMode)
    window.dispatchEvent(
      new CustomEvent<UiMode>(CHANGE_EVENT, { detail: nextMode }),
    )
  }, [])

  useEffect(() => {
    if (typeof window === 'undefined') return

    const handleStorage = (event: StorageEvent) => {
      if (event.key === STORAGE_KEY) {
        setUiMode(isUiMode(event.newValue) ? event.newValue : 'simple')
      }
    }

    const handleLocalChange = (event: Event) => {
      const nextMode = (event as CustomEvent<UiMode>).detail
      if (isUiMode(nextMode)) {
        setUiMode(nextMode)
      }
    }

    window.addEventListener('storage', handleStorage)
    window.addEventListener(CHANGE_EVENT, handleLocalChange)

    return () => {
      window.removeEventListener('storage', handleStorage)
      window.removeEventListener(CHANGE_EVENT, handleLocalChange)
    }
  }, [])

  return {
    mode: uiMode,
    setMode: updateMode,
    isSimpleMode: uiMode === 'simple',
  }
}
