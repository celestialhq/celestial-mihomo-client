/* eslint-disable @eslint-react/set-state-in-effect */
import {
  Box,
  FormControl,
  MenuItem,
  Select,
  SelectChangeEvent,
  Skeleton,
  Stack,
} from '@mui/material'
import { useCallback, useEffect, useMemo, useState } from 'react'

import { useProfiles } from '@/hooks/use-profiles'
import { useProxySelection } from '@/hooks/use-proxy-selection'
import { useAppData } from '@/providers/app-data-context'

const SELECTOR_TYPE = 'Selector'

type SimpleGroup = {
  name: string
  now?: string
  all: string[]
}

const normalizeName = (value?: string | null) =>
  typeof value === 'string' ? value.trim() : ''

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
      return (currentGroup ?? groups[0])?.name ?? ''
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

  const showGroupSelect = !isGlobalMode && !isDirectMode && groups.length > 1

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
    return <Skeleton className="simple-proxy-selector" variant="rounded" />
  }

  return (
    <Box className="simple-proxy-selector">
      <Stack direction={{ xs: 'column', sm: 'row' }} spacing={1.2}>
        {showGroupSelect && (
          <FormControl fullWidth size="small" variant="standard">
            <Select
              labelId="simple-proxy-group-label"
              value={groupName}
              onChange={handleGroupChange}
              disableUnderline
            >
              {groups.map((group) => (
                <MenuItem key={group.name} value={group.name}>
                  {group.name}
                </MenuItem>
              ))}
            </Select>
          </FormControl>
        )}

        <FormControl fullWidth size="small" variant="standard">
          <Select
            labelId="simple-proxy-node-label"
            value={proxyName}
            onChange={handleProxyChange}
            disabled={isDirectMode || !currentGroup?.all.length}
            disableUnderline
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
                {name}
              </MenuItem>
            ))}
          </Select>
        </FormControl>
      </Stack>
    </Box>
  )
}
