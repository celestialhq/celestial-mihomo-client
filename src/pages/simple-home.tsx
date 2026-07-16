import { Box, Button, ButtonGroup, Typography } from '@mui/material'
import { useLockFn } from 'ahooks'
import { useMemo } from 'react'
import { useNavigate } from 'react-router'
import { closeAllConnections } from 'tauri-plugin-mihomo-api'

import { SimpleConnectButton } from '@/components/simple/simple-connect-button'
import { SimpleProxySelector } from '@/components/simple/simple-proxy-selector'
import { useProfiles } from '@/hooks/use-profiles'
import { useSimpleConnection } from '@/hooks/use-simple-connection'
import { useVerge } from '@/hooks/use-verge'
import { useAppData } from '@/providers/app-data-context'
import { patchClashMode } from '@/services/cmds'

const CLASH_MODES = ['rule', 'global', 'direct'] as const
type ClashMode = (typeof CLASH_MODES)[number]

const MODE_LABELS: Record<ClashMode, string> = {
  rule: 'Правила',
  global: 'Глобал',
  direct: 'Директ',
}

const isClashMode = (value?: string): value is ClashMode =>
  Boolean(value && CLASH_MODES.includes(value as ClashMode))

const formatTraffic = (bytes?: number) => {
  if (!bytes || bytes <= 0) return ''

  const units = ['Б', 'КБ', 'МБ', 'ГБ', 'ТБ']
  let value = bytes
  let unitIndex = 0

  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024
    unitIndex += 1
  }

  return `${value.toFixed(value >= 10 ? 0 : 1)} ${units[unitIndex]}`
}

const SimpleHomePage = () => {
  const navigate = useNavigate()
  const { current } = useProfiles()
  const { verge } = useVerge()
  const { connected } = useSimpleConnection()
  const { proxies, clashConfig, refreshClashConfig } = useAppData()

  const currentMode = clashConfig?.mode?.toLowerCase()
  const activeMode = isClashMode(currentMode) ? currentMode : 'rule'
  const profileName = current?.name || 'Профиль не выбран'
  const subscriptionInfo = useMemo(
    () => formatTraffic(current?.extra?.total),
    [current?.extra?.total],
  )

  const currentServerName = useMemo(() => {
    if (!proxies) return ''
    if (activeMode === 'direct') return 'DIRECT'
    if (activeMode === 'global') return proxies.global?.now ?? ''

    const firstSelector = (proxies.groups ?? []).find(
      (group: IProxyGroupItem) => group?.type === 'Selector',
    )
    return firstSelector?.now ?? ''
  }, [proxies, activeMode])

  const changeMode = useLockFn(async (mode: ClashMode) => {
    if (mode === activeMode) return

    if (verge?.auto_close_connection) {
      closeAllConnections()
    }

    await patchClashMode(mode)
    refreshClashConfig()
  })

  if (!current) {
    return (
      <Box className="simple-home-page">
        <Box className="simple-home-page__empty">
          <Typography className="simple-home-page__profile-name">
            Профиль не выбран
          </Typography>
          <Button variant="contained" onClick={() => navigate('/profile')}>
            Профили
          </Button>
        </Box>
      </Box>
    )
  }

  return (
    <Box className="simple-home-page">
      <Box className="simple-home-page__shell">
        <Box className="simple-home-page__header">
          <Typography className="simple-home-page__profile-name" noWrap>
            {profileName}
          </Typography>
          {subscriptionInfo && (
            <Typography className="simple-home-page__profile-sub" noWrap>
              {subscriptionInfo}
            </Typography>
          )}
        </Box>

        <Box className="simple-home-page__center">
          <SimpleConnectButton />

          <Box className="simple-home-page__mode">
            <ButtonGroup variant="outlined" size="small">
              {CLASH_MODES.map((mode) => (
                <Button
                  key={mode}
                  variant={mode === activeMode ? 'contained' : 'outlined'}
                  onClick={() => changeMode(mode)}
                >
                  {MODE_LABELS[mode]}
                </Button>
              ))}
            </ButtonGroup>
          </Box>

          <Typography className="simple-connect-button__state">
            {connected ? 'Защита включена' : 'Защита отключена'}
          </Typography>

          {currentServerName && (
            <Typography className="simple-home-page__server" noWrap>
              {currentServerName}
            </Typography>
          )}

          <Box className="simple-info-card">
            <Typography className="simple-info-card__label">Прокси</Typography>
            <SimpleProxySelector />
          </Box>
        </Box>
      </Box>
    </Box>
  )
}

export default SimpleHomePage
