import { useLockFn } from 'ahooks'
import { useState } from 'react'

import { useSystemProxyState } from '@/hooks/use-system-proxy-state'
import { useVerge } from '@/hooks/use-verge'
import { startVpn, stopVpn } from '@/services/cmds'
import getSystem from '@/utils/get-system'

// Android has no system-proxy concept — VpnService (TUN) is the only
// connection mode, so toggling drives start_vpn/stop_vpn directly instead
// of desktop's system-proxy toggle.
const IS_SINGLE_MODE_PLATFORM = getSystem() === 'android'

export const useSimpleConnection = () => {
  const { verge, mutateVerge, patchVerge } = useVerge()
  const { indicator: systemProxyEnabled, toggleSystemProxy } =
    useSystemProxyState()
  const [pending, setPending] = useState(false)

  const tunEnabled = verge?.enable_tun_mode ?? false
  const connected = IS_SINGLE_MODE_PLATFORM
    ? tunEnabled
    : systemProxyEnabled || tunEnabled

  const toggleConnection = useLockFn(async () => {
    setPending(true)
    try {
      if (IS_SINGLE_MODE_PLATFORM) {
        mutateVerge(
          (prev) => (prev ? { ...prev, enable_tun_mode: !connected } : prev),
          false,
        )
        if (connected) {
          await stopVpn()
        } else {
          await startVpn()
        }
        return
      }

      if (connected) {
        if (tunEnabled) {
          mutateVerge(
            (prev) => (prev ? { ...prev, enable_tun_mode: false } : prev),
            false,
          )
          await patchVerge({ enable_tun_mode: false })
        }

        if (systemProxyEnabled) {
          await toggleSystemProxy(false)
        }
        return
      }

      await toggleSystemProxy(true)
    } finally {
      setPending(false)
    }
  })

  return { connected, pending, toggleConnection }
}
