import {
  ComputerRounded,
  MultipleStopRounded,
  TroubleshootRounded,
} from '@mui/icons-material'
import { Box, Stack, Typography } from '@mui/material'
import { useTranslation } from 'react-i18next'

import { ClashModeCard } from '@/components/home/clash-mode-card'
import ProxyControlSwitches from '@/components/shared/proxy-control-switches'
import { showNotice } from '@/services/notice-service'

const SimpleSettingsPage = () => {
  const { t } = useTranslation()

  const onError = (err: Error) => {
    showNotice.error(err)
  }

  return (
    <Box className="simple-settings-page">
      <Box className="simple-settings-page__header">
        <Typography variant="h5" sx={{ fontWeight: 900 }}>
          Настройки
        </Typography>
        <Typography variant="body2" color="text.secondary">
          Режим работы, системный прокси и TUN
        </Typography>
      </Box>

      <Stack spacing={2}>
        <Box className="simple-settings-section">
          <Stack direction="row" spacing={1.2} sx={{ alignItems: 'center' }}>
            <MultipleStopRounded color="primary" />
            <Typography variant="h6" sx={{ fontWeight: 800 }}>
              Режим маршрутизации
            </Typography>
          </Stack>
          <ClashModeCard />
        </Box>

        <Box className="simple-settings-section">
          <Stack direction="row" spacing={1.2} sx={{ alignItems: 'center' }}>
            <ComputerRounded color="primary" />
            <Typography variant="h6" sx={{ fontWeight: 800 }}>
              Системный прокси
            </Typography>
          </Stack>
          <ProxyControlSwitches
            label={t('settings.sections.system.toggles.systemProxy')}
            onError={onError}
            noRightPadding
          />
        </Box>

        <Box className="simple-settings-section">
          <Stack direction="row" spacing={1.2} sx={{ alignItems: 'center' }}>
            <TroubleshootRounded color="primary" />
            <Typography variant="h6" sx={{ fontWeight: 800 }}>
              TUN
            </Typography>
          </Stack>
          <ProxyControlSwitches
            label={t('settings.sections.system.toggles.tunMode')}
            onError={onError}
            noRightPadding
          />
        </Box>
      </Stack>
    </Box>
  )
}

export default SimpleSettingsPage
