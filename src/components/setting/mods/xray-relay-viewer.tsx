import {
  Box,
  Chip,
  Divider,
  List,
  ListItem,
  ListItemText,
  Stack,
  TextField,
  Typography,
} from '@mui/material'
import { useLockFn } from 'ahooks'
import type { Ref } from 'react'
import { useImperativeHandle, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { BaseDialog, DialogRef } from '@/components/base'
import {
  exportRuntimeConfig,
  exportXrayConfig,
  getXrayRelayStatus,
} from '@/services/cmds'
import { showNotice } from '@/services/notice-service'

/**
 * The answer to "is my traffic going through xray right now, and if not, why not".
 *
 * Every node is listed, relayed or not, with the reason it is not — a node quietly missing
 * from the relay looks identical to one that was never there, and the whole point of the
 * mode is that the user can tell.
 */
export function XrayRelayViewer({ ref }: { ref?: Ref<DialogRef> }) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  const [status, setStatus] = useState<IXrayRelayStatus | null>(null)
  const [exported, setExported] = useState('')

  const refresh = async () => {
    try {
      setStatus(await getXrayRelayStatus())
    } catch (err) {
      showNotice.error(err)
    }
  }

  useImperativeHandle(ref, () => ({
    open: () => {
      setOpen(true)
      setExported('')
      void refresh()
    },
    close: () => setOpen(false),
  }))

  const onExport = useLockFn(async (which: 'xray' | 'runtime') => {
    try {
      const text =
        which === 'xray'
          ? await exportXrayConfig(false)
          : await exportRuntimeConfig(false)
      setExported(text)
    } catch (err) {
      showNotice.error(err)
    }
  })

  const relayed = status?.nodes.filter((node) => node.relayed).length ?? 0
  const total = status?.nodes.length ?? 0

  return (
    <BaseDialog
      open={open}
      title={t('settings.modals.xrayRelay.title')}
      contentSx={{ width: 640, maxHeight: 560 }}
      cancelBtn={t('shared.actions.close')}
      onClose={() => setOpen(false)}
      onCancel={() => setOpen(false)}
    >
      <Stack spacing={1} sx={{ mb: 1 }}>
        <Box sx={{ display: 'flex', gap: 1, flexWrap: 'wrap' }}>
          <Chip
            size="small"
            color={status?.active ? 'success' : 'default'}
            label={
              status?.active
                ? t('settings.modals.xrayRelay.status.active', {
                    relayed,
                    total,
                  })
                : t('settings.modals.xrayRelay.status.native')
            }
          />
          {status?.forced && (
            <Chip
              size="small"
              variant="outlined"
              label={t('settings.modals.xrayRelay.status.forced')}
            />
          )}
          {status?.suppressed && (
            <Chip
              size="small"
              color="warning"
              label={t('settings.modals.xrayRelay.status.suppressed')}
            />
          )}
          <Chip
            size="small"
            variant="outlined"
            label={
              status?.has_template
                ? t('settings.modals.xrayRelay.status.fromTemplate')
                : t('settings.modals.xrayRelay.status.fromConverter')
            }
          />
        </Box>
        <Typography variant="caption" color="text.secondary">
          {t('settings.modals.xrayRelay.description')}
        </Typography>
      </Stack>

      <Divider />

      <List dense sx={{ maxHeight: 300, overflowY: 'auto' }}>
        {status?.nodes.map((node) => (
          <ListItem key={node.name} sx={{ px: 0 }}>
            <ListItemText
              primary={node.name}
              secondary={
                node.relayed
                  ? t('settings.modals.xrayRelay.node.viaPort', {
                      port: node.port,
                    })
                  : (node.reason ?? t('settings.modals.xrayRelay.node.native'))
              }
              slotProps={{
                primary: { sx: { wordBreak: 'break-all' } },
                secondary: { sx: { fontSize: 12 } },
              }}
            />
          </ListItem>
        ))}
        {status?.nodes.length === 0 && (
          <ListItem sx={{ px: 0 }}>
            <ListItemText
              secondary={t('settings.modals.xrayRelay.node.none')}
            />
          </ListItem>
        )}
      </List>

      <Divider sx={{ mb: 1 }} />

      <Stack direction="row" spacing={2} sx={{ mb: 1 }}>
        <Typography
          variant="button"
          sx={{ cursor: 'pointer', color: 'primary.main' }}
          onClick={() => onExport('xray')}
        >
          {t('settings.modals.xrayRelay.export.xray')}
        </Typography>
        <Typography
          variant="button"
          sx={{ cursor: 'pointer', color: 'primary.main' }}
          onClick={() => onExport('runtime')}
        >
          {t('settings.modals.xrayRelay.export.runtime')}
        </Typography>
      </Stack>

      {exported && (
        <>
          <Typography variant="caption" color="text.secondary">
            {t('settings.modals.xrayRelay.export.masked')}
          </Typography>
          <TextField
            multiline
            fullWidth
            size="small"
            value={exported}
            slotProps={{ input: { readOnly: true, sx: { fontSize: 11 } } }}
            rows={10}
          />
        </>
      )}
    </BaseDialog>
  )
}
