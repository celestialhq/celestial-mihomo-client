import {
  AccountTreeRounded,
  DnsRounded,
  PublicRounded,
  ShieldRounded,
} from '@mui/icons-material'
import { Box, Chip, Stack, Typography } from '@mui/material'
import { useMemo } from 'react'

import { SimpleConnectButton } from '@/components/simple/simple-connect-button'
import { SimpleProxySelector } from '@/components/simple/simple-proxy-selector'
import { useProfiles } from '@/hooks/use-profiles'
import { useAppData } from '@/providers/app-data-context'

const MODE_LABELS: Record<string, string> = {
  rule: 'Режим правил',
  global: 'Глобальный режим',
  direct: 'Direct',
}

const SimpleHomePage = () => {
  const { current } = useProfiles()
  const { clashConfig, systemProxyAddress } = useAppData()

  const mode = clashConfig?.mode?.toLowerCase() ?? 'rule'
  const modeLabel = MODE_LABELS[mode] ?? mode

  const profileName = useMemo(
    () => current?.name || 'Профиль не выбран',
    [current],
  )

  return (
    <Box className="simple-home-page">
      <Box className="simple-home-page__status">
        <Stack spacing={1} sx={{ alignItems: 'center', textAlign: 'center' }}>
          <Box className="simple-home-page__avatar">
            <ShieldRounded />
          </Box>
          <Typography variant="h5" sx={{ fontWeight: 900 }}>
            Celestial VPN
          </Typography>
          <Typography variant="body2" color="text.secondary">
            {profileName}
          </Typography>
        </Stack>
      </Box>

      <Box className="simple-home-page__meta">
        <Stack
          direction={{ xs: 'column', sm: 'row' }}
          spacing={1.5}
          sx={{ alignItems: 'stretch' }}
        >
          <Box className="simple-stat">
            <PublicRounded />
            <span>{modeLabel}</span>
          </Box>
          <Box className="simple-stat">
            <DnsRounded />
            <span>{systemProxyAddress || '-'}</span>
          </Box>
          <Box className="simple-stat">
            <AccountTreeRounded />
            <span>{current?.type || 'Profile'}</span>
          </Box>
        </Stack>
      </Box>

      <Box className="simple-home-page__center">
        <SimpleConnectButton />
      </Box>

      <Box className="simple-home-page__selector">
        <SimpleProxySelector />
      </Box>

      <Stack
        direction="row"
        spacing={1}
        className="simple-home-page__footer"
        sx={{ justifyContent: 'center' }}
      >
        <Chip label={modeLabel} color="primary" variant="outlined" />
        <Chip label={systemProxyAddress || '-'} variant="outlined" />
      </Stack>
    </Box>
  )
}

export default SimpleHomePage
