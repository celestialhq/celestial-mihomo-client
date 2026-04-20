/* eslint-disable @eslint-react/set-state-in-effect */
import { PowerSettingsNewRounded } from '@mui/icons-material'
import { Box, CircularProgress, Stack, Typography } from '@mui/material'
import { useLockFn } from 'ahooks'
import { useEffect, useMemo, useRef, useState } from 'react'

import { useSystemProxyState } from '@/hooks/use-system-proxy-state'
import { useVerge } from '@/hooks/use-verge'

const formatUptime = (uptimeMs: number) => {
  const hours = Math.floor(uptimeMs / 3600000)
  const minutes = Math.floor((uptimeMs % 3600000) / 60000)
  const seconds = Math.floor((uptimeMs % 60000) / 1000)

  return `${hours.toString().padStart(2, '0')}:${minutes
    .toString()
    .padStart(2, '0')}:${seconds.toString().padStart(2, '0')}`
}

export const SimpleConnectButton = () => {
  const { verge, mutateVerge, patchVerge } = useVerge()
  const { indicator: systemProxyEnabled, toggleSystemProxy } =
    useSystemProxyState()
  const [pending, setPending] = useState(false)
  const [elapsedMs, setElapsedMs] = useState(0)
  const startedAtRef = useRef<number | null>(null)
  const wasConnectedRef = useRef(false)

  const tunEnabled = verge?.enable_tun_mode ?? false
  const connected = systemProxyEnabled || tunEnabled

  useEffect(() => {
    if (connected && !wasConnectedRef.current) {
      startedAtRef.current = Date.now()
      setElapsedMs(0)
    }

    if (!connected) {
      startedAtRef.current = null
      setElapsedMs(0)
    }

    wasConnectedRef.current = connected
  }, [connected])

  useEffect(() => {
    if (!connected || startedAtRef.current == null) return

    const tick = () => {
      if (startedAtRef.current != null) {
        setElapsedMs(Date.now() - startedAtRef.current)
      }
    }

    tick()
    const timer = window.setInterval(tick, 1000)
    return () => window.clearInterval(timer)
  }, [connected])

  const formattedUptime = useMemo(
    () => (connected ? formatUptime(elapsedMs) : '00:00:00'),
    [connected, elapsedMs],
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
    <Stack spacing={2.6} sx={{ alignItems: 'center' }}>
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
            <CircularProgress size={30} color="inherit" />
          ) : (
            <PowerSettingsNewRounded />
          )}
        </span>
      </Box>

      <Stack spacing={0.4} sx={{ alignItems: 'center' }}>
        <Typography className="simple-connect-button__state">
          {connected ? 'Защита включена' : 'Защита отключена'}
        </Typography>
        <Typography className="simple-connect-button__timer">
          {formattedUptime}
        </Typography>
      </Stack>
    </Stack>
  )
}
