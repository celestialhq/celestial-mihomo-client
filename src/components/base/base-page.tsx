import { Box, IconButton, Typography } from '@mui/material'
import { useTheme } from '@mui/material/styles'
import React, { ReactNode } from 'react'

import { MoonIcon, SunIcon } from '@/components/layout/nav-icons'
import { useVerge } from '@/hooks/use-verge'
import { useThemeMode } from '@/services/states'

import { BaseErrorBoundary } from './base-error-boundary'

interface Props {
  title?: React.ReactNode // the page title
  header?: React.ReactNode // something behind title
  contentStyle?: React.CSSProperties
  children?: ReactNode
  full?: boolean
}

const ThemeToggleButton = () => {
  const mode = useThemeMode()
  const { patchVerge } = useVerge()

  return (
    <IconButton
      size="small"
      onClick={() =>
        void patchVerge({ theme_mode: mode === 'dark' ? 'light' : 'dark' })
      }
      sx={{
        width: 36,
        height: 36,
        borderRadius: '10px',
        border: '1px solid var(--border)',
        background: 'var(--card)',
        color: 'var(--text2)',
        '&:hover': { color: 'var(--text)', background: 'var(--track)' },
      }}
    >
      {mode === 'dark' ? <SunIcon size={17} /> : <MoonIcon size={17} />}
    </IconButton>
  )
}

export const BasePage: React.FC<Props> = (props) => {
  const { title, header, contentStyle, full, children } = props
  const theme = useTheme()

  return (
    <BaseErrorBoundary>
      <div className="base-page">
        <header data-tauri-drag-region="true" style={{ userSelect: 'none' }}>
          <Typography
            sx={{ fontSize: '20px', fontWeight: '700 ' }}
            data-tauri-drag-region="true"
          >
            {title}
          </Typography>

          <Box sx={{ display: 'flex', alignItems: 'center', gap: '9px' }}>
            {header}
            <ThemeToggleButton />
          </Box>
        </header>

        <div
          className={full ? 'base-container no-padding' : 'base-container'}
          style={{
            backgroundColor: theme.palette.background.default,
          }}
        >
          <section
            style={{
              backgroundColor: theme.palette.background.default,
            }}
          >
            <div className="base-content" style={contentStyle}>
              {children}
            </div>
          </section>
        </div>
      </div>
    </BaseErrorBoundary>
  )
}
