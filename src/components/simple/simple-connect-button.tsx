import {
  PauseRounded,
  PlayArrowRounded,
  PowerSettingsNewRounded,
} from '@mui/icons-material'
import { Box, CircularProgress, Stack, Typography } from '@mui/material'
import { useLockFn } from 'ahooks'
import { useMemo, useState } from 'react'

import { useSystemProxyState } from '@/hooks/use-system-proxy-state'
import { useVerge } from '@/hooks/use-verge'
import { useAppData } from '@/providers/app-data-context'

const formatUptime = (uptimeMs: number) => {
  const hours = Math.floor(uptimeMs / 3600000)
  const minutes = Math.floor((uptimeMs % 3600000) / 60000)
  const seconds = Math.floor((uptimeMs % 60000) / 1000)

  return `${hours.toString().padStart(2, '0')}:${minutes
    .toString()
    .padStart(2, '0')}:${seconds.toString().padStart(2, '0')}`
}

export const SimpleConnectButton = () => {
  const { uptime } = useAppData()
  const { verge, mutateVerge, patchVerge } = useVerge()
  const { indicator: systemProxyEnabled, toggleSystemProxy } =
    useSystemProxyState()
  const [pending, setPending] = useState(false)

  const tunEnabled = verge?.enable_tun_mode ?? false
  const connected = systemProxyEnabled || tunEnabled

  const formattedUptime = useMemo(
    () => (connected ? formatUptime(uptime) : '00:00:00'),
    [connected, uptime],
  )

  const toggleConnection = useLockFn(async () => {
    setPending(true)
    try {
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

  return (
    <Stack spacing={2.2} sx={{ alignItems: 'center' }}>
      <Typography
        variant="overline"
        sx={{ color: connected ? 'success.main' : 'text.secondary' }}
      >
        {connected ? 'Подключено' : 'Отключено'}
      </Typography>

      <Box
        component="button"
        className={`simple-connect-button${connected ? ' is-connected' : ''}`}
        onClick={toggleConnection}
        type="button"
        aria-label={connected ? 'Отключить VPN' : 'Подключить VPN'}
      >
        <span className="simple-connect-button__ring" />
        <span className="simple-connect-button__content">
          {pending ? (
            <CircularProgress size={54} color="inherit" />
          ) : connected ? (
            <PauseRounded />
          ) : (
            <PlayArrowRounded />
          )}
        </span>
      </Box>

      <Stack spacing={0.5} sx={{ alignItems: 'center' }}>
        <Typography variant="h5" sx={{ fontWeight: 900 }}>
          {formattedUptime}
        </Typography>
        <Stack direction="row" spacing={0.75} sx={{ alignItems: 'center' }}>
          <PowerSettingsNewRounded
            sx={{ fontSize: 16, color: 'text.secondary' }}
          />
          <Typography variant="body2" color="text.secondary">
            {connected ? 'Защита активна' : 'Нажмите для подключения'}
          </Typography>
        </Stack>
      </Stack>
    </Stack>
  )
}
