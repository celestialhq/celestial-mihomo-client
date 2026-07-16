import { GitHub, HelpOutlineRounded, Telegram } from '@mui/icons-material'
import { Box, ButtonGroup, IconButton, Grid } from '@mui/material'
import { useLockFn } from 'ahooks'
import { useTranslation } from 'react-i18next'

import { BasePage } from '@/components/base'
import SettingClash from '@/components/setting/setting-clash'
import SettingSystem from '@/components/setting/setting-system'
import SettingVergeAdvanced from '@/components/setting/setting-verge-advanced'
import SettingVergeBasic from '@/components/setting/setting-verge-basic'
import { openWebUrl } from '@/services/cmds'
import { showNotice } from '@/services/notice-service'
import getSystem from '@/utils/get-system'

// Clash core config (ports, DNS, log level, core version switching) and
// Verge's power-user tooling (config/log dirs, dev tools, diagnostics
// export) are desktop power-user surfaces with little value on a mobile
// VPN client — hide both entirely on Android to keep Settings focused.
const IS_MOBILE_PLATFORM = getSystem() === 'android'

const AdvancedSettingPage = () => {
  const { t } = useTranslation()

  const onError = (err: any) => {
    showNotice.error(err)
  }

  const toGithubRepo = useLockFn(() => {
    return openWebUrl(
      'https://github.com/pius-pp/celestial-mihomo-client-public',
    )
  })

  const toGithubDoc = useLockFn(() => {
    return openWebUrl(
      'https://github.com/pius-pp/celestial-mihomo-client-public#readme',
    )
  })

  const toTelegramChannel = useLockFn(() => {
    return openWebUrl('https://t.me/celestial_releases')
  })

  const panelSx = {
    borderRadius: '12px',
    border: '1px solid var(--border)',
    background: 'var(--card)',
    boxShadow: 'none',
    overflow: 'hidden',
  }

  return (
    <BasePage
      title={t('settings.page.title')}
      header={
        <ButtonGroup variant="contained" aria-label="Basic button group">
          <IconButton
            size="medium"
            color="inherit"
            title={t('settings.page.actions.manual')}
            onClick={toGithubDoc}
          >
            <HelpOutlineRounded fontSize="inherit" />
          </IconButton>
          <IconButton
            size="medium"
            color="inherit"
            title={t('settings.page.actions.telegram')}
            onClick={toTelegramChannel}
          >
            <Telegram fontSize="inherit" />
          </IconButton>

          <IconButton
            size="medium"
            color="inherit"
            title={t('settings.page.actions.github')}
            onClick={toGithubRepo}
          >
            <GitHub fontSize="inherit" />
          </IconButton>
        </ButtonGroup>
      }
    >
      <Grid container spacing={1.5} columns={{ xs: 6, sm: 6, md: 12 }}>
        <Grid size={6}>
          <Box
            sx={
              IS_MOBILE_PLATFORM
                ? panelSx
                : {
                    ...panelSx,
                    marginBottom: 1.5,
                  }
            }
          >
            <SettingSystem onError={onError} />
          </Box>
          {!IS_MOBILE_PLATFORM && (
            <Box sx={panelSx}>
              <SettingClash onError={onError} />
            </Box>
          )}
        </Grid>
        <Grid size={6}>
          <Box sx={panelSx}>
            <SettingVergeBasic onError={onError} />
          </Box>
          {!IS_MOBILE_PLATFORM && (
            <Box sx={{ ...panelSx, marginTop: 1.5 }}>
              <SettingVergeAdvanced onError={onError} />
            </Box>
          )}
        </Grid>
      </Grid>
    </BasePage>
  )
}

const SettingPage = () => {
  return <AdvancedSettingPage />
}

export default SettingPage
