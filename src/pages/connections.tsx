import {
  AppsRounded,
  DeleteForeverRounded,
  TableChartRounded,
  TableRowsRounded,
  ViewColumnRounded,
} from '@mui/icons-material'
import {
  Avatar,
  Box,
  Button,
  ButtonGroup,
  Fab,
  IconButton,
  MenuItem,
  Tooltip,
  Zoom,
} from '@mui/material'
import { useQuery } from '@tanstack/react-query'
import { convertFileSrc } from '@tauri-apps/api/core'
import { useLockFn } from 'ahooks'
import { useCallback, useMemo, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { closeAllConnections } from 'tauri-plugin-mihomo-api'

import {
  BaseEmpty,
  BasePage,
  BaseSearchBox,
  BaseStyledSelect,
  VirtualList,
} from '@/components/base'
import {
  ConnectionDetail,
  ConnectionDetailRef,
} from '@/components/connection/connection-detail'
import { ConnectionItem } from '@/components/connection/connection-item'
import { ConnectionTable } from '@/components/connection/connection-table'
import { useConnectionData } from '@/hooks/use-connection-data'
import { useConnectionSetting } from '@/hooks/use-connection-setting'
import { getProcessIcon } from '@/services/cmds'
import parseTraffic from '@/utils/parse-traffic'

type OrderFunc = (list: IConnectionsItem[]) => IConnectionsItem[]
type ConnectionListItem =
  | {
      type: 'group'
      key: string
      name: string
      processPath?: string
      count: number
      upload: number
      download: number
    }
  | {
      type: 'connection'
      key: string
      value: IConnectionsItem
    }

const ORDER_OPTIONS = [
  {
    id: 'default',
    labelKey: 'connections.components.order.default',
    fn: (list: IConnectionsItem[]) =>
      list.sort(
        (a, b) =>
          new Date(b.start || '0').getTime()! -
          new Date(a.start || '0').getTime()!,
      ),
  },
  {
    id: 'uploadSpeed',
    labelKey: 'connections.components.order.uploadSpeed',
    fn: (list: IConnectionsItem[]) =>
      list.sort((a, b) => b.curUpload! - a.curUpload!),
  },
  {
    id: 'downloadSpeed',
    labelKey: 'connections.components.order.downloadSpeed',
    fn: (list: IConnectionsItem[]) =>
      list.sort((a, b) => b.curDownload! - a.curDownload!),
  },
] as const

type OrderKey = (typeof ORDER_OPTIONS)[number]['id']

const orderFunctionMap = ORDER_OPTIONS.reduce<Record<OrderKey, OrderFunc>>(
  (acc, option) => {
    acc[option.id] = option.fn
    return acc
  },
  {} as Record<OrderKey, OrderFunc>,
)

const getConnectionProcessName = (conn: IConnectionsItem) => {
  const { process, processPath } = conn.metadata
  if (process?.trim()) return process.trim()
  if (!processPath?.trim()) return 'Unknown app'
  return processPath.split(/[\\/]/).pop() || processPath
}

const buildConnectionGroups = (connections: IConnectionsItem[]) => {
  const groups = new Map<
    string,
    {
      name: string
      processPath?: string
      upload: number
      download: number
      items: IConnectionsItem[]
    }
  >()

  connections.forEach((conn) => {
    const name = getConnectionProcessName(conn)
    const processPath = conn.metadata.processPath
    const key = `${name}\n${processPath ?? ''}`
    const group = groups.get(key) ?? {
      name,
      processPath,
      upload: 0,
      download: 0,
      items: [],
    }

    group.upload += conn.curUpload ?? 0
    group.download += conn.curDownload ?? 0
    group.items.push(conn)
    groups.set(key, group)
  })

  return Array.from(groups.entries()).flatMap<ConnectionListItem>(
    ([key, group]) => [
      {
        type: 'group',
        key: `group:${key}`,
        name: group.name,
        processPath: group.processPath,
        count: group.items.length,
        upload: group.upload,
        download: group.download,
      },
      ...group.items.map((value) => ({
        type: 'connection' as const,
        key: value.id,
        value,
      })),
    ],
  )
}

const ProcessAvatar = ({
  name,
  processPath,
}: {
  name: string
  processPath?: string
}) => {
  const { data } = useQuery({
    queryKey: ['process-icon', processPath],
    queryFn: () => getProcessIcon(processPath ?? ''),
    enabled: Boolean(processPath),
    staleTime: Infinity,
    gcTime: 60 * 60 * 1000,
    retry: 1,
  })

  const src = data ? convertFileSrc(data) : ''

  return (
    <Avatar
      src={src || undefined}
      variant="rounded"
      sx={(theme) => ({
        width: 34,
        height: 34,
        borderRadius: 1.5,
        bgcolor:
          theme.palette.mode === 'light'
            ? theme.palette.primary.light
            : theme.palette.primary.dark,
        color: theme.palette.primary.contrastText,
        fontWeight: 800,
        fontSize: 14,
      })}
    >
      {name === 'Unknown app' ? (
        <AppsRounded fontSize="small" />
      ) : (
        name.trim().slice(0, 1).toUpperCase()
      )}
    </Avatar>
  )
}

const ConnectionGroupHeader = ({
  item,
}: {
  item: Extract<ConnectionListItem, { type: 'group' }>
}) => (
  <Box
    sx={(theme) => ({
      display: 'flex',
      alignItems: 'center',
      gap: 1.25,
      mx: 1.25,
      my: 0.75,
      px: 1.25,
      py: 1,
      borderRadius: 1.5,
      border: `1px solid ${theme.palette.divider}`,
      background:
        theme.palette.mode === 'light'
          ? 'linear-gradient(135deg, #ffffff, #f8fbff)'
          : 'linear-gradient(135deg, rgba(36, 39, 53, 0.98), rgba(25, 34, 50, 0.98))',
      boxShadow:
        theme.palette.mode === 'light'
          ? '0 10px 24px rgba(15, 23, 42, 0.05)'
          : '0 12px 26px rgba(0, 0, 0, 0.18)',
    })}
  >
    <ProcessAvatar name={item.name} processPath={item.processPath} />
    <Box sx={{ minWidth: 0, flex: 1 }}>
      <Box
        sx={{
          fontWeight: 800,
          fontSize: 14,
          overflow: 'hidden',
          textOverflow: 'ellipsis',
          whiteSpace: 'nowrap',
        }}
        title={item.processPath || item.name}
      >
        {item.name}
      </Box>
      <Box sx={{ color: 'text.secondary', fontSize: 12 }}>
        {item.count} connections
      </Box>
    </Box>
    <Box
      sx={{
        color: 'text.secondary',
        fontSize: 12,
        whiteSpace: 'nowrap',
      }}
    >
      {parseTraffic(item.upload).join(' ')}/s up ·{' '}
      {parseTraffic(item.download).join(' ')}/s down
    </Box>
  </Box>
)

const ConnectionsPage = () => {
  const { t } = useTranslation()
  const [match, setMatch] = useState<(input: string) => boolean>(
    () => () => true,
  )
  const [curOrderOpt, setCurOrderOpt] = useState<OrderKey>('default')
  const [connectionsType, setConnectionsType] = useState<'active' | 'closed'>(
    'active',
  )

  const {
    response: { data: connections },
    clearClosedConnections,
  } = useConnectionData()

  const [setting, setSetting] = useConnectionSetting()

  const isTableLayout = setting.layout === 'table'

  const [isColumnManagerOpen, setIsColumnManagerOpen] = useState(false)

  const [filterConn] = useMemo(() => {
    const orderFunc = orderFunctionMap[curOrderOpt]
    const conns =
      (connectionsType === 'active'
        ? connections?.activeConnections
        : connections?.closedConnections) ?? []
    let matchConns = conns.filter((conn) => {
      const { host, destinationIP, process } = conn.metadata
      return (
        match(host || '') || match(destinationIP || '') || match(process || '')
      )
    })

    if (orderFunc) matchConns = orderFunc(matchConns ?? [])

    return [matchConns]
  }, [connections, connectionsType, match, curOrderOpt])

  const onCloseAll = useLockFn(closeAllConnections)

  const detailRef = useRef<ConnectionDetailRef>(null!)

  const handleSearch = useCallback((match: (content: string) => boolean) => {
    setMatch(() => match)
  }, [])

  const hasTableData = filterConn.length > 0
  const groupedListItems = useMemo(
    () => buildConnectionGroups(filterConn),
    [filterConn],
  )

  return (
    <BasePage
      full
      title={
        <span style={{ whiteSpace: 'nowrap' }}>
          {t('connections.page.title')}
        </span>
      }
      contentStyle={{
        height: '100%',
        display: 'flex',
        flexDirection: 'column',
        overflow: 'hidden',
        borderRadius: '8px',
        minHeight: 0,
      }}
      header={
        <Box sx={{ display: 'flex', alignItems: 'center', gap: 2 }}>
          <Box sx={{ mx: 1 }}>
            {t('shared.labels.downloaded')}:{' '}
            {parseTraffic(connections?.downloadTotal)}
          </Box>
          <Box sx={{ mx: 1 }}>
            {t('shared.labels.uploaded')}:{' '}
            {parseTraffic(connections?.uploadTotal)}
          </Box>
          <IconButton
            color="inherit"
            size="small"
            onClick={() =>
              setSetting((o) =>
                o?.layout !== 'table'
                  ? { ...o, layout: 'table' }
                  : { ...o, layout: 'list' },
              )
            }
          >
            {isTableLayout ? (
              <TableRowsRounded titleAccess={t('shared.actions.listView')} />
            ) : (
              <TableChartRounded titleAccess={t('shared.actions.tableView')} />
            )}
          </IconButton>
          <Button size="small" variant="contained" onClick={onCloseAll}>
            <span style={{ whiteSpace: 'nowrap' }}>
              {t('shared.actions.closeAll')}
            </span>
          </Button>
        </Box>
      }
    >
      <Box
        sx={{
          pt: 1,
          mb: 0.5,
          mx: '10px',
          minHeight: '36px',
          display: 'flex',
          alignItems: 'center',
          gap: 1,
          userSelect: 'text',
          position: 'sticky',
          top: 0,
          zIndex: 2,
        }}
      >
        <ButtonGroup sx={{ mr: 1, flexBasis: 'content' }}>
          <Button
            size="small"
            variant={connectionsType === 'active' ? 'contained' : 'outlined'}
            onClick={() => setConnectionsType('active')}
          >
            {t('connections.components.actions.active')}{' '}
            {connections?.activeConnections.length}
          </Button>
          <Button
            size="small"
            variant={connectionsType === 'closed' ? 'contained' : 'outlined'}
            onClick={() => setConnectionsType('closed')}
          >
            {t('connections.components.actions.closed')}{' '}
            {connections?.closedConnections.length}
          </Button>
        </ButtonGroup>
        {!isTableLayout && (
          <BaseStyledSelect
            value={curOrderOpt}
            onChange={(e) => setCurOrderOpt(e.target.value as OrderKey)}
          >
            {ORDER_OPTIONS.map((option) => (
              <MenuItem key={option.id} value={option.id}>
                <span style={{ fontSize: 14 }}>{t(option.labelKey)}</span>
              </MenuItem>
            ))}
          </BaseStyledSelect>
        )}
        <Box
          sx={{
            flex: 1,
            display: 'flex',
            alignItems: 'center',
            '& > *': {
              flex: 1,
            },
          }}
        >
          <BaseSearchBox onSearch={handleSearch} />
        </Box>
        {isTableLayout && hasTableData && (
          <Tooltip title={t('connections.components.columnManager.title')}>
            <IconButton
              size="small"
              aria-label={t('connections.components.columnManager.title')}
              onClick={() => setIsColumnManagerOpen(true)}
              sx={{ flex: '0 0 auto' }}
            >
              <ViewColumnRounded fontSize="small" />
            </IconButton>
          </Tooltip>
        )}
      </Box>

      {!hasTableData ? (
        <BaseEmpty />
      ) : isTableLayout ? (
        <ConnectionTable
          connections={filterConn}
          onShowDetail={(detail) =>
            detailRef.current?.open(detail, connectionsType === 'closed')
          }
          columnManagerOpen={isTableLayout && isColumnManagerOpen}
          onCloseColumnManager={() => setIsColumnManagerOpen(false)}
        />
      ) : (
        <VirtualList
          count={groupedListItems.length}
          estimateSize={56}
          getItemKey={(i) => groupedListItems[i]?.key ?? i}
          renderItem={(i) => {
            const item = groupedListItems[i]
            if (!item) return null
            if (item.type === 'group') {
              return <ConnectionGroupHeader item={item} />
            }
            return (
              <ConnectionItem
                value={item.value}
                closed={connectionsType === 'closed'}
                onShowDetail={() =>
                  detailRef.current?.open(
                    item.value,
                    connectionsType === 'closed',
                  )
                }
              />
            )
          }}
          style={{
            flex: 1,
            borderRadius: '8px',
            WebkitOverflowScrolling: 'touch',
            overscrollBehavior: 'contain',
          }}
        />
      )}
      <ConnectionDetail ref={detailRef} />
      <Zoom
        in={connectionsType === 'closed' && filterConn.length > 0}
        unmountOnExit
      >
        <Fab
          size="medium"
          variant="extended"
          sx={{
            position: 'absolute',
            right: 16,
            bottom: isTableLayout ? 70 : 16,
          }}
          color="primary"
          onClick={() => clearClosedConnections()}
        >
          <DeleteForeverRounded sx={{ mr: 1 }} fontSize="small" />
          {t('shared.actions.clear')}
        </Fab>
      </Zoom>
    </BasePage>
  )
}

export default ConnectionsPage
