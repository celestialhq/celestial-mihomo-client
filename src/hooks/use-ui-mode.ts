import { useCallback, useEffect, useState } from 'react'

export type UiMode = 'simple' | 'advanced'

const STORAGE_KEY = 'celestial-ui-mode'
const CHANGE_EVENT = 'celestial-ui-mode-change'

const readUiMode = (): UiMode => {
  if (typeof window === 'undefined') return 'advanced'

  window.localStorage.setItem(STORAGE_KEY, 'advanced')
  return 'advanced'
}

export const useUiMode = () => {
  const [uiMode, setUiMode] = useState<UiMode>(readUiMode)

  const updateMode = useCallback((_nextMode: UiMode) => {
    setUiMode('advanced')

    if (typeof window === 'undefined') return
    window.localStorage.setItem(STORAGE_KEY, 'advanced')
    window.dispatchEvent(
      new CustomEvent<UiMode>(CHANGE_EVENT, { detail: 'advanced' }),
    )
  }, [])

  useEffect(() => {
    if (typeof window === 'undefined') return

    const handleStorage = (event: StorageEvent) => {
      if (event.key === STORAGE_KEY) {
        setUiMode('advanced')
        window.localStorage.setItem(STORAGE_KEY, 'advanced')
      }
    }

    const handleLocalChange = (_event: Event) => {
      setUiMode('advanced')
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
    isSimpleMode: false,
  }
}
