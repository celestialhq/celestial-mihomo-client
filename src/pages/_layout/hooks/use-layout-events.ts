import { listen } from '@tauri-apps/api/event'
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow'
import { useEffect } from 'react'

import { useListen } from '@/hooks/use-listen'
import { RUN_STATE_QUERY_KEY } from '@/hooks/use-system-state'
import { queryClient } from '@/services/query-client'

export const useLayoutEvents = (
  handleNotice: (payload: [string, string]) => void,
) => {
  const { addListener } = useListen()

  useEffect(() => {
    const unlisteners: Array<() => void> = []
    let disposed = false
    const revalidateKeys = (keys: readonly string[]) => {
      keys.forEach((key) => {
        queryClient.invalidateQueries({ queryKey: [key] })
      })
    }

    const register = (
      maybeUnlisten: void | (() => void) | Promise<void | (() => void)>,
    ) => {
      if (!maybeUnlisten) return

      if (typeof maybeUnlisten === 'function') {
        unlisteners.push(maybeUnlisten)
        return
      }

      maybeUnlisten
        .then((unlisten) => {
          if (!unlisten) return
          if (disposed) {
            unlisten()
          } else {
            unlisteners.push(unlisten)
          }
        })
        .catch((error) =>
          console.error('[Event Listener] Registration failed:', error),
        )
    }

    register(
      addListener('verge://refresh-clash-config', async () => {
        revalidateKeys([
          'getProxies',
          'getVersion',
          'getClashConfig',
          'getProxyProviders',
          'getRules',
          'getRuleProviders',
        ])
      }),
    )

    register(
      addListener('verge://refresh-verge-config', () => {
        revalidateKeys([
          'getVergeConfig',
          'getSystemProxy',
          'getAutotemProxy',
          'getRunningMode',
          'isServiceAvailable',
        ])
      }),
    )

    register(
      // The payload is the whole state, so write it straight in rather than
      // invalidating and asking the backend for what it just told us.
      addListener('verge://run-state-changed', ({ payload }) => {
        queryClient.setQueryData(RUN_STATE_QUERY_KEY, payload as IRunState)
      }),
    )

    register(
      addListener('verge://notice-message', ({ payload }) =>
        handleNotice(payload as [string, string]),
      ),
    )

    const appWindow = getCurrentWebviewWindow()
    register(
      (async () => {
        const [hideUnlisten, showUnlisten] = await Promise.all([
          listen('verge://hide-window', () => appWindow.hide()),
          listen('verge://show-window', () => appWindow.show()),
        ])
        return () => {
          hideUnlisten()
          showUnlisten()
        }
      })(),
    )

    return () => {
      disposed = true
      const errors: Error[] = []

      unlisteners.forEach((unlisten) => {
        try {
          unlisten()
        } catch (error) {
          errors.push(error instanceof Error ? error : new Error(String(error)))
        }
      })

      if (errors.length > 0) {
        console.error(
          `[Event Listener] Encountered ${errors.length} errors during cleanup:`,
          errors,
        )
      }

      unlisteners.length = 0
    }
  }, [addListener, handleNotice])
}
