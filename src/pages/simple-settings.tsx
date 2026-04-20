import {
  ComputerRounded,
  SystemUpdateAltRounded,
  TroubleshootRounded,
} from '@mui/icons-material'
import { Box, Button, Stack, Typography } from '@mui/material'
import { useLockFn } from 'ahooks'
import { useRef } from 'react'
import { useTranslation } from 'react-i18next'

import { DialogRef } from '@/components/base'
import { UpdateViewer } from '@/components/setting/mods/update-viewer'
import ProxyControlSwitches from '@/components/shared/proxy-control-switches'
import { useUpdate } from '@/hooks/use-update'
import { showNotice } from '@/services/notice-service'

const SimpleSettingsPage = () => {
  const { t } = useTranslation()
  const updateViewerRef = useRef<DialogRef>(null)
  const { updateInfo, checkUpdate, loading } = useUpdate(false)

  const onError = (err: Error) => {
    showNotice.error(err)
  }

  const handleCheckUpdate = useLockFn(async () => {
    const result = await checkUpdate()
    const info = result.data ?? updateInfo

    if (info?.available) {
      updateViewerRef.current?.open()
      return
    }

    showNotice.info('Обновлений нет')
  })

  return (
    <Box className="simple-settings-page">
      <Box className="simple-settings-page__header">
        <Typography variant="h5" sx={{ fontWeight: 900 }}>
          Настройки
        </Typography>
        <Typography variant="body2" color="text.secondary">
          Системный прокси, TUN и обновления
        </Typography>
      </Box>

      <Stack spacing={2}>
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

        <Box className="simple-settings-section">
          <Stack
            direction={{ xs: 'column', sm: 'row' }}
            spacing={1.2}
            sx={{ alignItems: { xs: 'stretch', sm: 'center' } }}
          >
            <Stack
              direction="row"
              spacing={1.2}
              sx={{ alignItems: 'center', flex: 1 }}
            >
              <SystemUpdateAltRounded color="primary" />
              <Box>
                <Typography variant="h6" sx={{ fontWeight: 800 }}>
                  Обновления
                </Typography>
                <Typography variant="body2" color="text.secondary">
                  Проверить новую версию клиента
                </Typography>
              </Box>
            </Stack>
            <Button
              variant="contained"
              onClick={handleCheckUpdate}
              disabled={loading}
            >
              Проверить обновления
            </Button>
          </Stack>
        </Box>
      </Stack>

      <UpdateViewer ref={updateViewerRef} />
    </Box>
  )
}

export default SimpleSettingsPage
