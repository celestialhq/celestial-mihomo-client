import { CheckCircleOutlineRounded } from '@mui/icons-material'
import { Box, ListItemButton, styled, Typography } from '@mui/material'
import { useLockFn } from 'ahooks'
import { useCallback, useEffect, useReducer } from 'react'
import { useTranslation } from 'react-i18next'

import { BaseLoading } from '@/components/base'
import { useVerge } from '@/hooks/use-verge'
import delayManager, { DelayUpdate } from '@/services/delay'

interface Props {
  group: IProxyGroupItem
  proxy: IProxyItem
  selected: boolean
  showType?: boolean
  onClick?: (name: string) => void
}

// 多列布局
export const ProxyItemMini = (props: Props) => {
  const { group, proxy, selected, showType = true, onClick } = props

  const { t } = useTranslation()

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
  }, [isPreset, proxy.name, group.name])

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
    <ListItemButton
      dense
      selected={selected}
      onClick={() => onClick?.(proxy.name)}
      sx={[
        {
          height: '100%',
          minHeight: 56,
          borderRadius: '10px',
          pl: 1.5,
          pr: 1,
          justifyContent: 'space-between',
          alignItems: 'center',
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
            '& .the-pin, & .the-unpin': {
              position: 'absolute',
              fontSize: '12px',
              top: '-5px',
              right: '-5px',
            },
            '& .the-unpin': { filter: 'grayscale(1)' },
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
          }
        },
      ]}
    >
      <Box
        title={`${proxy.name}\n${proxy.now ?? ''}`}
        sx={{ overflow: 'hidden' }}
      >
        <Typography
          variant="body2"
          component="div"
          color="text.primary"
          sx={{
            display: 'block',
            textOverflow: 'ellipsis',
            wordBreak: 'break-all',
            overflow: 'hidden',
            whiteSpace: 'nowrap',
          }}
        >
          {proxy.name}
        </Typography>

        {showType && (
          <Box
            sx={{
              display: 'flex',
              flexWrap: 'nowrap',
              flex: 'none',
              marginTop: '4px',
            }}
          >
            {proxy.now && (
              <Typography
                variant="body2"
                component="div"
                color="text.secondary"
                sx={{
                  display: 'block',
                  textOverflow: 'ellipsis',
                  wordBreak: 'break-all',
                  overflow: 'hidden',
                  whiteSpace: 'nowrap',
                  marginRight: '8px',
                }}
              >
                {proxy.now}
              </Typography>
            )}
            {!!proxy.provider && (
              <TypeBox color="text.secondary" component="span">
                {proxy.provider}
              </TypeBox>
            )}
            <TypeBox color="text.secondary" component="span">
              {proxy.type}
            </TypeBox>
            {proxy.udp && (
              <TypeBox color="text.secondary" component="span">
                UDP
              </TypeBox>
            )}
            {proxy.xudp && (
              <TypeBox color="text.secondary" component="span">
                XUDP
              </TypeBox>
            )}
            {proxy.tfo && (
              <TypeBox color="text.secondary" component="span">
                TFO
              </TypeBox>
            )}
            {proxy.mptcp && (
              <TypeBox color="text.secondary" component="span">
                MPTCP
              </TypeBox>
            )}
            {proxy.smux && (
              <TypeBox color="text.secondary" component="span">
                SMUX
              </TypeBox>
            )}
          </Box>
        )}
      </Box>
      <Box
        sx={{
          ml: 0.5,
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

        {delayValue >= 0 && (
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
        {proxy.type !== 'Direct' &&
          delayValue !== -2 &&
          delayValue < 0 &&
          selected && (
            // 展示已选择的 icon
            <CheckCircleOutlineRounded
              className="the-icon"
              sx={{ fontSize: 16, mr: 0.5, display: 'block' }}
            />
          )}
      </Box>
      {group.fixed && group.fixed === proxy.name && (
        // 展示 fixed 状态
        <span
          className={proxy.name === group.now ? 'the-pin' : 'the-unpin'}
          title={
            group.type === 'URLTest'
              ? t('proxies.page.labels.delayCheckReset')
              : ''
          }
        >
          📌
        </span>
      )}
    </ListItemButton>
  )
}

const Widget = styled(Box)(() => ({
  padding: '3px 6px',
  fontSize: 12,
  fontFamily: "'JetBrains Mono', monospace",
  fontWeight: 600,
  borderRadius: '4px',
}))

const TypeBox = styled(Box, {
  shouldForwardProp: (prop) => prop !== 'component',
})<{ component?: React.ElementType }>(() => ({
  display: 'inline-block',
  border: '1px solid var(--border)',
  color: 'var(--text2)',
  borderRadius: 4,
  fontSize: 9,
  fontFamily: "'JetBrains Mono', monospace",
  fontWeight: 600,
  marginRight: '4px',
  marginTop: 'auto',
  padding: '1px 4px',
  lineHeight: 1.4,
}))
