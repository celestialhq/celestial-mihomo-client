import {
  BuildRounded,
  DeleteForeverRounded,
  PauseCircleOutlineRounded,
  PlayCircleOutlineRounded,
  SettingsRounded,
  WarningRounded,
} from '@mui/icons-material'
import { Box, Typography, alpha, useTheme } from '@mui/material'
import { useLockFn } from 'ahooks'
import React, { useCallback, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { DialogRef, Switch, TooltipIcon } from '@/components/base'
import { SysproxyViewer } from '@/components/setting/mods/sysproxy-viewer'
import { TunViewer } from '@/components/setting/mods/tun-viewer'
import { useServiceInstaller } from '@/hooks/use-service-installer'
import { useServiceUninstaller } from '@/hooks/use-service-uninstaller'
import { useSystemProxyState } from '@/hooks/use-system-proxy-state'
import { useSystemState } from '@/hooks/use-system-state'
import { useVerge } from '@/hooks/use-verge'
import { showNotice } from '@/services/notice-service'
import getSystem from '@/utils/get-system'

// The privileged-helper "service" install flow is a desktop-only concept
// (elevated TUN permissions via runas/pkexec/osascript) — Android grants
// VPN access via a one-time permission dialog instead, so there's nothing
// to install here.
const IS_SINGLE_MODE_PLATFORM = getSystem() === 'android'

interface ProxySwitchProps {
  label?: string
  onError?: (err: Error) => void
  noRightPadding?: boolean
}

interface SwitchRowProps {
  label: string
  active: boolean
  checked?: boolean
  disabled?: boolean
  infoTitle: string
  onInfoClick?: () => void
  extraIcons?: React.ReactNode
  onToggle: (value: boolean) => Promise<void>
  onError?: (err: Error) => void
  highlight?: boolean
}

/**
 * 抽取的子组件：统一的开关 UI
 * active = 真实状态OS/配置 乐观更新
 */
const SwitchRow = ({
  label,
  active,
  checked: checkedProp,
  disabled,
  infoTitle,
  onInfoClick,
  extraIcons,
  onToggle,
  onError,
  highlight,
}: SwitchRowProps) => {
  const theme = useTheme()
  const controlledChecked = checkedProp ?? active
  const [checked, setChecked] = useState(controlledChecked)
  const pendingRef = useRef(false)

  if (pendingRef.current) {
    if (controlledChecked === checked) pendingRef.current = false
  } else if (checked !== controlledChecked) {
    setChecked(controlledChecked)
  }

  const handleChange = (_: React.ChangeEvent, value: boolean) => {
    pendingRef.current = true
    setChecked(value)
    onToggle(value)
      .catch((err: any) => {
        setChecked(controlledChecked)
        onError?.(err)
      })
      .finally(() => {
        pendingRef.current = false
      })
  }

  return (
    <Box
      sx={{
        display: 'flex',
        alignItems: 'center',
        flexWrap: 'wrap',
        rowGap: 0.5,
        justifyContent: 'space-between',
        p: 1,
        pr: 2,
        borderRadius: 1.5,
        bgcolor: highlight
          ? alpha(theme.palette.success.main, 0.07)
          : 'transparent',
        opacity: disabled ? 0.6 : 1,
        transition: 'background-color 0.3s',
      }}
    >
      <Box
        sx={{
          display: 'flex',
          alignItems: 'center',
          flexWrap: 'wrap',
          minWidth: 0,
          flex: '1 1 auto',
        }}
      >
        {active ? (
          <PlayCircleOutlineRounded
            sx={{ color: 'success.main', mr: 1, flex: 'none' }}
          />
        ) : (
          <PauseCircleOutlineRounded
            sx={{ color: 'text.disabled', mr: 1, flex: 'none' }}
          />
        )}
        <Typography
          variant="subtitle1"
          sx={{ fontWeight: 500, fontSize: '15px', minWidth: 0 }}
          noWrap
        >
          {label}
        </Typography>
        <TooltipIcon
          title={infoTitle}
          icon={SettingsRounded}
          onClick={onInfoClick}
          sx={{ ml: 1, flex: 'none' }}
        />
        {extraIcons}
      </Box>

      <Switch
        edge="end"
        disabled={disabled}
        checked={checked}
        onChange={handleChange}
        sx={{ flex: 'none' }}
      />
    </Box>
  )
}

const ProxyControlSwitches = ({
  label,
  onError,
  noRightPadding = false,
}: ProxySwitchProps) => {
  const { t } = useTranslation()
  const { verge, mutateVerge, patchVerge } = useVerge()
  const { installServiceAndRestartCore } = useServiceInstaller()
  const { uninstallServiceAndRestartCore } = useServiceUninstaller()
  const {
    configState: systemProxyConfigState,
    indicator: systemProxyIndicator,
    toggleSystemProxy,
  } = useSystemProxyState()
  const { isServiceOk, isTunModeAvailable, mutateSystemState } =
    useSystemState()

  const sysproxyRef = useRef<DialogRef>(null)
  const tunRef = useRef<DialogRef>(null)

  const { enable_tun_mode } = verge ?? {}

  const showErrorNotice = useCallback(
    (msg: string) => showNotice.error(msg),
    [],
  )

  const handleTunToggle = async (value: boolean) => {
    if (!isTunModeAvailable) {
      const msgKey = 'settings.sections.proxyControl.tooltips.tunUnavailable'
      showErrorNotice(msgKey)
      throw new Error(t(msgKey))
    }
    mutateVerge({ ...verge, enable_tun_mode: value }, false)
    await patchVerge({ enable_tun_mode: value })
  }

  const onInstallService = useLockFn(async () => {
    try {
      await installServiceAndRestartCore()
      await mutateSystemState()
    } catch (err) {
      showNotice.error(err)
    }
  })

  const onUninstallService = useLockFn(async () => {
    try {
      if (verge?.enable_tun_mode) {
        await handleTunToggle(false)
      }
      await uninstallServiceAndRestartCore()
      await mutateSystemState()
    } catch (err) {
      showNotice.error(err)
    }
  })

  const isSystemProxyMode =
    label === t('settings.sections.system.toggles.systemProxy') || !label
  const isTunMode = label === t('settings.sections.system.toggles.tunMode')

  return (
    <Box sx={{ width: '100%', pr: noRightPadding ? 1 : 2 }}>
      {isSystemProxyMode && (
        <SwitchRow
          label={t('settings.sections.proxyControl.fields.systemProxy')}
          active={systemProxyIndicator}
          checked={systemProxyConfigState}
          infoTitle={t('settings.sections.proxyControl.tooltips.systemProxy')}
          onInfoClick={() => sysproxyRef.current?.open()}
          onToggle={(value) => toggleSystemProxy(value)}
          onError={onError}
          highlight={systemProxyIndicator}
        />
      )}

      {isTunMode && (
        <SwitchRow
          label={t('settings.sections.proxyControl.fields.tunMode')}
          active={enable_tun_mode || false}
          infoTitle={t('settings.sections.proxyControl.tooltips.tunMode')}
          onInfoClick={() => tunRef.current?.open()}
          onToggle={handleTunToggle}
          onError={onError}
          disabled={!isTunModeAvailable}
          highlight={enable_tun_mode || false}
          extraIcons={
            <>
              {!isTunModeAvailable && (
                <>
                  <TooltipIcon
                    title={t(
                      IS_SINGLE_MODE_PLATFORM
                        ? 'settings.sections.proxyControl.tooltips.tunUnavailableMobile'
                        : 'settings.sections.proxyControl.tooltips.tunUnavailable',
                    )}
                    icon={WarningRounded}
                    sx={{ color: 'warning.main', ml: 1 }}
                  />
                  {!IS_SINGLE_MODE_PLATFORM && (
                    <TooltipIcon
                      title={t(
                        'settings.sections.proxyControl.actions.installService',
                      )}
                      icon={BuildRounded}
                      color="primary"
                      onClick={onInstallService}
                      sx={{ ml: 1 }}
                    />
                  )}
                </>
              )}
              {isServiceOk && !IS_SINGLE_MODE_PLATFORM && (
                <TooltipIcon
                  title={t(
                    'settings.sections.proxyControl.actions.uninstallService',
                  )}
                  icon={DeleteForeverRounded}
                  color="secondary"
                  onClick={onUninstallService}
                  sx={{ ml: 1 }}
                />
              )}
            </>
          }
        />
      )}

      <SysproxyViewer ref={sysproxyRef} />
      <TunViewer ref={tunRef} />
    </Box>
  )
}

export default ProxyControlSwitches
