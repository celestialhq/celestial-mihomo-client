import {
  Box,
  Button,
  Chip,
  Divider,
  List,
  ListItem,
  ListItemText,
  MenuItem,
  Select,
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
  checkXrayCoreUpdate,
  exportRuntimeConfig,
  exportXrayConfig,
  getXrayCoreStatus,
  getXrayRelayStatus,
  installXrayCore,
  patchVergeConfig,
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
  const [core, setCore] = useState<IXrayCoreStatus | null>(null)
  const [channel, setChannel] = useState<'stable' | 'prerelease'>('stable')
  // What a check found upstream, held until the user decides. Nothing installs on its own.
  const [offered, setOffered] = useState('')
  const [busy, setBusy] = useState(false)

  const refreshCore = async () => {
    try {
      setCore(await getXrayCoreStatus())
    } catch (err) {
      showNotice.error(err)
    }
  }

  // Selecting a core only records the choice; it takes effect the next time xray starts.
  const onSelect = useLockFn(async (value: string) => {
    try {
      await patchVergeConfig({
        xray_core_version: value === 'bundled' ? undefined : value,
      })
      await refreshCore()
    } catch (err) {
      showNotice.error(err)
    }
  })

  const onCheck = useLockFn(async () => {
    setBusy(true)
    try {
      setOffered(await checkXrayCoreUpdate(channel))
    } catch (err) {
      showNotice.error(err)
    } finally {
      setBusy(false)
    }
  })

  const onInstall = useLockFn(async () => {
    setBusy(true)
    try {
      await installXrayCore(offered)
      setOffered('')
      await refreshCore()
    } catch (err) {
      showNotice.error(err)
    } finally {
      setBusy(false)
    }
  })

  const refresh = async () => {
    void refreshCore()
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

      {/* Which core carries the traffic. Separate from the node list because it answers a
          different question: that one is about this subscription, this one about this
          machine. Nothing here downloads or replaces anything without being told to. */}
      <Stack spacing={1}>
        <Typography variant="subtitle2">
          {t('settings.modals.xrayRelay.core.title')}
        </Typography>
        <Stack
          direction="row"
          spacing={1}
          sx={{ alignItems: 'center', flexWrap: 'wrap' }}
        >
          <Select
            size="small"
            value={core?.selected ?? 'bundled'}
            onChange={(event) => onSelect(event.target.value)}
            sx={{ minWidth: 150 }}
          >
            <MenuItem value="bundled">
              {t('settings.modals.xrayRelay.core.bundled')}
            </MenuItem>
            {core?.installed.map((version) => (
              <MenuItem key={version} value={version}>
                {version}
              </MenuItem>
            ))}
          </Select>
          {core?.downloadable && (
            <>
              <Select
                size="small"
                value={channel}
                onChange={(event) =>
                  setChannel(event.target.value as 'stable' | 'prerelease')
                }
                sx={{ minWidth: 130 }}
              >
                <MenuItem value="stable">
                  {t('settings.modals.xrayRelay.core.stable')}
                </MenuItem>
                <MenuItem value="prerelease">
                  {t('settings.modals.xrayRelay.core.prerelease')}
                </MenuItem>
              </Select>
              <Button size="small" disabled={busy} onClick={onCheck}>
                {t('settings.modals.xrayRelay.core.check')}
              </Button>
            </>
          )}
        </Stack>
        {offered !== '' && (
          <Stack direction="row" spacing={1} sx={{ alignItems: 'center' }}>
            <Typography variant="caption" color="text.secondary">
              {core?.installed.includes(offered)
                ? t('settings.modals.xrayRelay.core.alreadyHave', {
                    version: offered,
                  })
                : t('settings.modals.xrayRelay.core.offered', {
                    version: offered,
                  })}
            </Typography>
            {!core?.installed.includes(offered) && (
              <Button size="small" disabled={busy} onClick={onInstall}>
                {t('settings.modals.xrayRelay.core.install')}
              </Button>
            )}
          </Stack>
        )}
        <Typography variant="caption" color="text.secondary">
          {t('settings.modals.xrayRelay.core.hint')}
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
