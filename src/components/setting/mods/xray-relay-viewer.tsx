import {
  Box,
  Button,
  Chip,
  Divider,
  List,
  ListItem,
  ListItemText,
  MenuItem,
  Stack,
  Typography,
} from '@mui/material'
import { useLockFn } from 'ahooks'
import type { Ref } from 'react'
import { useImperativeHandle, useState } from 'react'
import { useTranslation } from 'react-i18next'

import {
  BaseDialog,
  BaseStyledSelect,
  DialogRef,
  MonacoEditor,
} from '@/components/base'
import {
  checkXrayCoreUpdate,
  exportRuntimeConfig,
  exportXrayConfig,
  getXrayCoreStatus,
  getXrayRelayStatus,
  installXrayCore,
  patchVergeConfig,
  restartCore,
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
  // Which document is on screen, so the editor highlights it as what it is rather than
  // guessing from the text.
  const [exportedKind, setExportedKind] = useState<'xray' | 'runtime'>('xray')
  const [core, setCore] = useState<IXrayCoreStatus | null>(null)
  const [channel, setChannel] = useState<'stable' | 'prerelease'>('stable')
  // A version upstream has that this machine does not, held until the user acts on it.
  // Empty means there is nothing to offer — either nothing was checked, or the check found
  // nothing newer.
  const [pending, setPending] = useState('')
  // A check that came back with nothing to do. Worth saying out loud: silence after pressing
  // a button reads as a failure.
  const [latest, setLatest] = useState(false)
  const [busy, setBusy] = useState(false)

  const refreshCore = async () => {
    try {
      setCore(await getXrayCoreStatus())
    } catch (err) {
      showNotice.error(err)
    }
  }

  // Selecting applies. Recording the choice and leaving the old core running would mean the
  // control says one thing while the traffic does another, and the only way to reconcile
  // them would be a restart the user has to know to perform.
  const onSelect = useLockFn(async (value: string) => {
    if (value === (core?.selected ?? 'bundled')) return
    setBusy(true)
    try {
      await patchVergeConfig({ xray_core_version: value })
      await restartCore()
      await refreshCore()
      showNotice.success(t('settings.modals.xrayRelay.core.switched'))
    } catch (err) {
      showNotice.error(err)
    } finally {
      setBusy(false)
    }
  })

  const onCheck = useLockFn(async () => {
    setBusy(true)
    setLatest(false)
    try {
      const found = await checkXrayCoreUpdate(channel)
      // Already downloaded is the same answer as nothing newer: there is nothing to do
      // either way, and offering to install what is already installed is noise.
      const known =
        found === core?.selected || (core?.installed.includes(found) ?? false)
      setPending(known ? '' : found)
      setLatest(known)
    } catch (err) {
      showNotice.error(err)
    } finally {
      setBusy(false)
    }
  })

  // Installs and switches to it. Downloading a core and leaving it unused is not what
  // anybody pressed the button for.
  const onInstall = useLockFn(async () => {
    setBusy(true)
    try {
      const version = pending
      await installXrayCore(version)
      await patchVergeConfig({ xray_core_version: version })
      await restartCore()
      setPending('')
      await refreshCore()
      showNotice.success(t('settings.modals.xrayRelay.core.switched'))
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
      setExportedKind(which)
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

      {/* Which core carries the traffic. A different question from the node list below: that
          one is about this subscription, this one is about this machine. */}
      <Stack spacing={1}>
        <Typography variant="subtitle2">
          {t('settings.modals.xrayRelay.core.title')}
        </Typography>
        <Stack
          direction="row"
          spacing={1}
          sx={{ alignItems: 'center', flexWrap: 'wrap' }}
        >
          <BaseStyledSelect
            value={core?.selected ?? 'bundled'}
            onChange={(event) => onSelect(event.target.value)}
            disabled={busy}
            sx={{ width: 168 }}
          >
            <MenuItem value="bundled">
              {t('settings.modals.xrayRelay.core.bundled')}
            </MenuItem>
            {core?.installed.map((version) => (
              <MenuItem key={version} value={version}>
                {version}
              </MenuItem>
            ))}
          </BaseStyledSelect>
          {core?.downloadable && (
            <>
              <BaseStyledSelect
                value={channel}
                onChange={(event) =>
                  setChannel(event.target.value as 'stable' | 'prerelease')
                }
                disabled={busy}
                sx={{ width: 148 }}
              >
                <MenuItem value="stable">
                  {t('settings.modals.xrayRelay.core.stable')}
                </MenuItem>
                <MenuItem value="prerelease">
                  {t('settings.modals.xrayRelay.core.prerelease')}
                </MenuItem>
              </BaseStyledSelect>
              {/* One button with two faces. Checking tells you what exists; once it has told
                  you, the same button is what acts on the answer — a second control that
                  only ever means "yes, the thing you just asked about" is one control too
                  many. */}
              <Button
                size="small"
                variant={pending === '' ? 'text' : 'contained'}
                disabled={busy}
                onClick={pending === '' ? onCheck : onInstall}
              >
                {pending === ''
                  ? t('settings.modals.xrayRelay.core.check')
                  : t('settings.modals.xrayRelay.core.install', {
                      version: pending,
                    })}
              </Button>
            </>
          )}
        </Stack>
        <Typography variant="caption" color="text.secondary">
          {busy
            ? t('settings.modals.xrayRelay.core.working')
            : latest
              ? t('settings.modals.xrayRelay.core.latest')
              : t('settings.modals.xrayRelay.core.running', {
                  version:
                    core?.selected ??
                    t('settings.modals.xrayRelay.core.bundled'),
                })}
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
          <Box
            sx={{
              height: 320,
              border: 1,
              borderColor: 'divider',
              borderRadius: 1,
              overflow: 'hidden',
            }}
          >
            <MonacoEditor
              height="100%"
              language={exportedKind === 'xray' ? 'json' : 'yaml'}
              value={exported}
              options={{
                readOnly: true,
                minimap: { enabled: false },
                fontSize: 12,
                scrollBeyondLastLine: false,
                renderLineHighlight: 'none',
              }}
            />
          </Box>
        </>
      )}
    </BaseDialog>
  )
}
