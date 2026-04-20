/* eslint-disable @eslint-react/set-state-in-effect */
import {
  DnsRounded,
  LanRounded,
  RouterRounded,
  SignalWifi4BarRounded,
} from '@mui/icons-material'
import {
  Box,
  Chip,
  FormControl,
  InputLabel,
  MenuItem,
  Select,
  SelectChangeEvent,
  Skeleton,
  Stack,
  Typography,
} from '@mui/material'
import { useCallback, useEffect, useMemo, useState } from 'react'

import { useProfiles } from '@/hooks/use-profiles'
import { useProxySelection } from '@/hooks/use-proxy-selection'
import { useAppData } from '@/providers/app-data-context'
import delayManager from '@/services/delay'

const SELECTOR_TYPE = 'Selector'

type SimpleGroup = {
  name: string
  now?: string
  all: string[]
}

const normalizeName = (value?: string | null) =>
  typeof value === 'string' ? value.trim() : ''

const formatDelayColor = (
  delay: number,
): 'success' | 'warning' | 'error' | 'default' => {
  if (delay > 1e5 || delay === 0 || delay >= 10000) return 'error'
  if (delay >= 300) return 'warning'
  if (delay > 0) return 'success'
  return 'default'
}

export const SimpleProxySelector = () => {
  const { proxies, clashConfig, isCoreDataPending, refreshProxy } = useAppData()
  const { current } = useProfiles()
  const { changeProxy } = useProxySelection({
    onSuccess: () => {
      void refreshProxy()
    },
    onError: () => {
      void refreshProxy()
    },
  })

  const mode = clashConfig?.mode?.toLowerCase() ?? 'rule'
  const isGlobalMode = mode === 'global'
  const isDirectMode = mode === 'direct'

  const profileSelected = useMemo(
    () =>
      new Map(
        (current?.selected ?? []).map((item) => [
          normalizeName(item.name),
          normalizeName(item.now),
        ]),
      ),
    [current?.selected],
  )

  const groups = useMemo<SimpleGroup[]>(() => {
    if (!proxies) return []

    if (isDirectMode) {
      return [{ name: 'DIRECT', now: 'DIRECT', all: ['DIRECT'] }]
    }

    if (isGlobalMode && proxies.global?.all?.length) {
      return [
        {
          name: 'GLOBAL',
          now: normalizeName(proxies.global.now),
          all: proxies.global.all
            .map((item: string | { name?: string }) =>
              typeof item === 'string' ? item : item.name,
            )
            .map(normalizeName)
            .filter((name: string) =>
              Boolean(name && name !== 'DIRECT' && name !== 'REJECT'),
            ),
        },
      ]
    }

    return (proxies.groups ?? [])
      .filter((group: IProxyGroupItem) => group?.type === SELECTOR_TYPE)
      .map((group: IProxyGroupItem) => ({
        name: normalizeName(group.name),
        now: normalizeName(group.now),
        all: (group.all ?? [])
          .map((item: string | IProxyItem) =>
            typeof item === 'string' ? item : item.name,
          )
          .map(normalizeName)
          .filter(Boolean),
      }))
      .filter((group: SimpleGroup) =>
        Boolean(group.name && group.all.length > 0),
      )
  }, [isDirectMode, isGlobalMode, proxies])

  const [groupName, setGroupName] = useState('')
  const [proxyName, setProxyName] = useState('')

  useEffect(() => {
    if (!groups.length) {
      setGroupName('')
      setProxyName('')
      return
    }

    setGroupName((currentGroupName) => {
      const currentGroup = groups.find(
        (group) => group.name === currentGroupName,
      )
      const nextGroup = currentGroup ?? groups[0]
      return nextGroup?.name ?? ''
    })
  }, [groups])

  useEffect(() => {
    const group = groups.find((item) => item.name === groupName)
    if (!group) {
      setProxyName('')
      return
    }

    const savedProxy = profileSelected.get(group.name)
    const nextProxy =
      (savedProxy && group.all.includes(savedProxy) ? savedProxy : '') ||
      (group.now && group.all.includes(group.now) ? group.now : '') ||
      group.all[0] ||
      ''

    setProxyName(nextProxy)
  }, [groupName, groups, profileSelected])

  const currentGroup = useMemo(
    () => groups.find((group) => group.name === groupName),
    [groupName, groups],
  )

  const currentProxy = useMemo(
    () => (proxyName ? proxies?.records?.[proxyName] : null),
    [proxies?.records, proxyName],
  )

  const currentDelay = useMemo(() => {
    if (!currentProxy || !groupName || isDirectMode) return -1
    return delayManager.getDelayFix(currentProxy, groupName)
  }, [currentProxy, groupName, isDirectMode])

  const handleGroupChange = useCallback(
    (event: SelectChangeEvent<string>) => {
      const nextGroupName = event.target.value
      const nextGroup = groups.find((group) => group.name === nextGroupName)
      const nextProxy = nextGroup?.now || nextGroup?.all[0] || ''
      setGroupName(nextGroupName)
      setProxyName(nextProxy)
    },
    [groups],
  )

  const handleProxyChange = useCallback(
    (event: SelectChangeEvent<string>) => {
      const nextProxyName = event.target.value
      const previousProxy = proxyName

      setProxyName(nextProxyName)
      changeProxy(groupName, nextProxyName, previousProxy, isGlobalMode)
    },
    [changeProxy, groupName, isGlobalMode, proxyName],
  )

  if (isCoreDataPending) {
    return <Skeleton variant="rounded" height={164} />
  }

  return (
    <Box className="simple-proxy-selector">
      <Stack
        direction="row"
        spacing={1.5}
        sx={{ alignItems: 'center', mb: 2, minWidth: 0 }}
      >
        <Box className="simple-proxy-selector__icon">
          <RouterRounded fontSize="small" />
        </Box>
        <Box sx={{ minWidth: 0, flex: 1 }}>
          <Typography variant="overline" color="text.secondary">
            Выбор прокси
          </Typography>
          <Typography variant="h6" noWrap sx={{ fontWeight: 800 }}>
            {proxyName || 'Нет активного прокси'}
          </Typography>
        </Box>
        {currentProxy && !isDirectMode && (
          <Chip
            icon={<SignalWifi4BarRounded />}
            label={delayManager.formatDelay(currentDelay)}
            color={formatDelayColor(currentDelay)}
            variant="outlined"
          />
        )}
      </Stack>

      <Stack direction={{ xs: 'column', md: 'row' }} spacing={1.5}>
        <FormControl fullWidth size="small">
          <InputLabel id="simple-proxy-group-label">Группа</InputLabel>
          <Select
            labelId="simple-proxy-group-label"
            value={groupName}
            label="Группа"
            onChange={handleGroupChange}
            disabled={isGlobalMode || isDirectMode || groups.length < 2}
          >
            {groups.map((group) => (
              <MenuItem key={group.name} value={group.name}>
                <Stack
                  direction="row"
                  spacing={1}
                  sx={{ alignItems: 'center' }}
                >
                  <DnsRounded fontSize="small" />
                  <span>{group.name}</span>
                </Stack>
              </MenuItem>
            ))}
          </Select>
        </FormControl>

        <FormControl fullWidth size="small">
          <InputLabel id="simple-proxy-node-label">Прокси</InputLabel>
          <Select
            labelId="simple-proxy-node-label"
            value={proxyName}
            label="Прокси"
            onChange={handleProxyChange}
            disabled={isDirectMode || !currentGroup?.all.length}
            MenuProps={{
              slotProps: {
                paper: {
                  sx: { maxHeight: 420 },
                },
              },
            }}
          >
            {(currentGroup?.all ?? []).map((name) => (
              <MenuItem key={name} value={name}>
                <Stack
                  direction="row"
                  spacing={1}
                  sx={{ alignItems: 'center' }}
                >
                  <LanRounded fontSize="small" />
                  <span>{name}</span>
                </Stack>
              </MenuItem>
            ))}
          </Select>
        </FormControl>
      </Stack>
    </Box>
  )
}
