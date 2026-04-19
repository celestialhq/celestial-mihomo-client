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

const SettingPage = () => {
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
    borderRadius: 2,
    border: '1px solid rgba(148, 163, 184, 0.11)',
    background:
      'linear-gradient(180deg, rgba(23, 32, 51, 0.96), rgba(17, 26, 43, 0.96))',
    boxShadow: '0 18px 40px rgba(0, 0, 0, 0.22)',
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
            sx={{
              ...panelSx,
              marginBottom: 1.5,
            }}
          >
            <SettingSystem onError={onError} />
          </Box>
          <Box sx={panelSx}>
            <SettingClash onError={onError} />
          </Box>
        </Grid>
        <Grid size={6}>
          <Box
            sx={{
              ...panelSx,
              marginBottom: 1.5,
            }}
          >
            <SettingVergeBasic onError={onError} />
          </Box>
          <Box sx={panelSx}>
            <SettingVergeAdvanced onError={onError} />
          </Box>
        </Grid>
      </Grid>
    </BasePage>
  )
}

export default SettingPage
