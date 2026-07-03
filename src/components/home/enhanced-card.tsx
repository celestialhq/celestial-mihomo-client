import { Box } from '@mui/material'
import { forwardRef, ReactNode } from 'react'

// 自定义卡片组件接口
interface EnhancedCardProps {
  title: ReactNode
  icon?: ReactNode
  action?: ReactNode
  children: ReactNode
  iconColor?: 'primary' | 'secondary' | 'error' | 'warning' | 'info' | 'success'
  minHeight?: number | string
  noContentPadding?: boolean
}

// 自定义卡片组件 — flat block matching the Celestial design: mono all-caps
// label header, no icon chip, plain body. `icon`/`iconColor` are accepted
// for backward compat but no longer rendered — the mockup's cards carry no
// icon in their header, only a small mono label.
export const EnhancedCard = forwardRef<HTMLElement, EnhancedCardProps>(
  ({ title, action, children, minHeight, noContentPadding = false }, ref) => {
    return (
      <Box
        sx={{
          height: '100%',
          width: '100%',
          display: 'flex',
          flexDirection: 'column',
          borderRadius: '11px',
          border: '1px solid var(--border)',
          background: 'var(--card)',
          boxShadow: 'none',
          overflow: 'hidden',
          padding: '15px',
          boxSizing: 'border-box',
        }}
        ref={ref}
      >
        <Box
          sx={{
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            mb: '12px',
            flexShrink: 0,
          }}
        >
          {typeof title === 'string' ? (
            <Box
              component="span"
              sx={{
                fontFamily: "'JetBrains Mono', monospace",
                fontSize: '10.5px',
                fontWeight: 600,
                letterSpacing: '0.05em',
                textTransform: 'uppercase',
                color: 'var(--text2)',
                minWidth: 0,
                overflow: 'hidden',
                textOverflow: 'ellipsis',
                whiteSpace: 'nowrap',
              }}
              title={title}
            >
              {title}
            </Box>
          ) : (
            <Box sx={{ minWidth: 0, overflow: 'hidden' }}>{title}</Box>
          )}
          {action && <Box sx={{ ml: 2, flexShrink: 0 }}>{action}</Box>}
        </Box>
        <Box
          sx={{
            flex: 1,
            display: 'flex',
            flexDirection: 'column',
            p: noContentPadding ? 0 : 0,
            ...(minHeight && { minHeight }),
          }}
        >
          {children}
        </Box>
      </Box>
    )
  },
)
