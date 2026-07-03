import { CheckCircleOutlineRounded } from '@mui/icons-material'
import {
  Box,
  ListItem,
  ListItemButton,
  ListItemIcon,
  ListItemText,
  styled,
  SxProps,
  Theme,
} from '@mui/material'
import { useLockFn } from 'ahooks'
import { useCallback, useEffect, useReducer } from 'react'

import { BaseLoading } from '@/components/base'
import { useVerge } from '@/hooks/use-verge'
import delayManager, { DelayUpdate } from '@/services/delay'

interface Props {
  group: IProxyGroupItem
  proxy: IProxyItem
  selected: boolean
  showType?: boolean
  sx?: SxProps<Theme>
  onClick?: (name: string) => void
}

const Widget = styled(Box)(() => ({
  padding: '3px 6px',
  fontSize: 12,
  fontFamily: "'JetBrains Mono', monospace",
  fontWeight: 600,
  borderRadius: '4px',
}))

const TypeBox = styled('span')(() => ({
  display: 'inline-block',
  border: '1px solid var(--border)',
  color: 'var(--text2)',
  borderRadius: 4,
  fontSize: 9,
  fontFamily: "'JetBrains Mono', monospace",
  fontWeight: 600,
  marginRight: '4px',
  padding: '1px 4px',
  lineHeight: 1.4,
}))

export const ProxyItem = (props: Props) => {
  const { group, proxy, selected, showType = true, sx, onClick } = props

  const presetList = ['DIRECT', 'REJECT', 'REJECT-DROP', 'PASS', 'COMPATIBLE']
  const isPreset = presetList.includes(proxy.name)
  // -1/<=0 为不显示，-2 为 loading
  const [delayState, setDelayState] = useReducer(
    (_: DelayUpdate, next: DelayUpdate) => next,
    {
      delay: -1,
      updatedAt: 0,
    },
  )
  const { verge } = useVerge()
  const timeout = verge?.default_latency_timeout || 10000

  useEffect(() => {
    if (isPreset) return
    delayManager.setListener(proxy.name, group.name, setDelayState)

    return () => {
      delayManager.removeListener(proxy.name, group.name)
    }
  }, [proxy.name, group.name, isPreset])

  const updateDelay = useCallback(() => {
    if (!proxy) return
    const cachedUpdate = delayManager.getDelayUpdate(proxy.name, group.name)
    if (cachedUpdate) {
      setDelayState({ ...cachedUpdate })
      return
    }

    const fallbackDelay = delayManager.getDelayFix(proxy, group.name)
    if (fallbackDelay === -1) {
      setDelayState({ delay: -1, updatedAt: 0 })
      return
    }

    let updatedAt = 0
    const history = proxy.history
    if (history && history.length > 0) {
      const lastRecord = history[history.length - 1]
      const parsed = Date.parse(lastRecord.time)
      if (!Number.isNaN(parsed)) {
        updatedAt = parsed
      }
    }

    setDelayState({
      delay: fallbackDelay,
      updatedAt,
    })
  }, [proxy, group.name])

  useEffect(() => {
    updateDelay()
  }, [updateDelay])

  const onDelay = useLockFn(async () => {
    setDelayState({ delay: -2, updatedAt: Date.now() })
    setDelayState(
      await delayManager.checkDelay(proxy.name, group.name, timeout),
    )
  })

  const delayValue = delayState.delay

  return (
    <ListItem sx={sx}>
      <ListItemButton
        dense
        selected={selected}
        onClick={() => onClick?.(proxy.name)}
        sx={[
          {
            borderRadius: '10px',
            px: 1.25,
            transition: 'border-color 0.15s ease, background-color 0.15s ease',
          },
          () => {
            const showDelay = delayValue > 0

            return {
              '&:hover .the-check': { display: !showDelay ? 'block' : 'none' },
              '&:hover .the-delay': { display: showDelay ? 'block' : 'none' },
              '&:hover .the-icon': { display: 'none' },
              '&:hover': {
                borderColor: 'var(--border2)',
              },
              '&.Mui-selected': {
                border: '1.5px solid var(--accent)',
                bgcolor: 'var(--accent-bg)',
              },
              '&.Mui-selected:hover': {
                borderColor: 'var(--accent)',
              },
              backgroundColor: 'var(--card)',
              border: '1px solid var(--border)',
              boxShadow: 'none',
              marginBottom: '8px',
              minHeight: '44px',
            }
          },
        ]}
      >
        <ListItemText
          title={proxy.name}
          secondary={
            <>
              <Box
                sx={{
                  display: 'inline-block',
                  marginRight: '8px',
                  fontSize: '14px',
                  color: 'text.primary',
                }}
              >
                {proxy.name}
                {showType && proxy.now && ` - ${proxy.now}`}
              </Box>
              {showType && !!proxy.provider && (
                <TypeBox>{proxy.provider}</TypeBox>
              )}
              {showType && <TypeBox>{proxy.type}</TypeBox>}
              {showType && proxy.udp && <TypeBox>UDP</TypeBox>}
              {showType && proxy.xudp && <TypeBox>XUDP</TypeBox>}
              {showType && proxy.tfo && <TypeBox>TFO</TypeBox>}
              {showType && proxy.mptcp && <TypeBox>MPTCP</TypeBox>}
              {showType && proxy.smux && <TypeBox>SMUX</TypeBox>}
            </>
          }
        />

        <ListItemIcon
          sx={{
            justifyContent: 'flex-end',
            color: 'var(--accent)',
            display: isPreset ? 'none' : '',
          }}
        >
          {delayValue === -2 && (
            <Widget>
              <BaseLoading />
            </Widget>
          )}

          {!proxy.provider && delayValue !== -2 && (
            // provider 的节点不支持检测
            <Widget
              className="the-check"
              onClick={(e) => {
                e.preventDefault()
                e.stopPropagation()
                onDelay()
              }}
              sx={{
                display: 'none', // hover 时显示
                ':hover': { bgcolor: 'var(--track)' },
              }}
            >
              Check
            </Widget>
          )}

          {delayValue > 0 && (
            // 显示延迟
            <Widget
              className="the-delay"
              onClick={(e) => {
                if (proxy.provider) return
                e.preventDefault()
                e.stopPropagation()
                onDelay()
              }}
              sx={{
                color: delayManager.formatDelayColor(delayValue, timeout),
                ...(!proxy.provider
                  ? { ':hover': { bgcolor: 'var(--track)' } }
                  : {}),
              }}
            >
              {delayManager.formatDelay(delayValue, timeout)}
            </Widget>
          )}

          {delayValue !== -2 && delayValue <= 0 && selected && (
            // 展示已选择的 icon
            <CheckCircleOutlineRounded
              className="the-icon"
              sx={{ fontSize: 16 }}
            />
          )}
        </ListItemIcon>
      </ListItemButton>
    </ListItem>
  )
}
