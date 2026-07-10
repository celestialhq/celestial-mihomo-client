import { ChevronRightRounded } from '@mui/icons-material'
import {
  Box,
  List,
  ListItem,
  ListItemButton,
  ListItemText,
  ListSubheader,
} from '@mui/material'
import CircularProgress from '@mui/material/CircularProgress'
import React, { ReactNode, useState } from 'react'

import isAsyncFunction from '@/utils/is-async-function'

interface ItemProps {
  label: ReactNode
  extra?: ReactNode
  children?: ReactNode
  secondary?: ReactNode
  onClick?: () => void | Promise<any>
}

export const SettingItem: React.FC<ItemProps> = ({
  label,
  extra,
  children,
  secondary,
  onClick,
}) => {
  const clickable = !!onClick

  const primary = (
    <Box
      sx={{
        display: 'flex',
        alignItems: 'center',
        fontSize: '13.5px',
        fontWeight: 600,
        fontFamily: "'Montserrat', sans-serif",
      }}
    >
      <span>{label}</span>
      {extra ? extra : null}
    </Box>
  )

  const [isLoading, setIsLoading] = useState(false)
  const handleClick = () => {
    if (onClick) {
      if (isAsyncFunction(onClick)) {
        setIsLoading(true)
        onClick()!.finally(() => setIsLoading(false))
      } else {
        onClick()
      }
    }
  }

  return clickable ? (
    <ListItem disablePadding sx={{ borderTop: '1px solid var(--border)' }}>
      <ListItemButton
        onClick={handleClick}
        disabled={isLoading}
        sx={{ py: '12px' }}
      >
        <ListItemText primary={primary} secondary={secondary} />
        {isLoading ? (
          <CircularProgress color="inherit" size={20} />
        ) : (
          <ChevronRightRounded sx={{ color: 'var(--text3)' }} />
        )}
      </ListItemButton>
    </ListItem>
  ) : (
    <ListItem
      sx={{ pt: '12px', pb: '12px', borderTop: '1px solid var(--border)' }}
    >
      <ListItemText primary={primary} secondary={secondary} />
      {children}
    </ListItem>
  )
}

export const SettingList: React.FC<{
  title: string
  children: ReactNode
}> = ({ title, children }) => (
  <List sx={{ py: 0 }}>
    <ListSubheader
      sx={{
        background: 'transparent',
        fontSize: '12px',
        fontWeight: 700,
        fontFamily: "'JetBrains Mono', monospace",
        letterSpacing: '0.05em',
        textTransform: 'uppercase',
        color: 'var(--text2)',
        lineHeight: '32px',
      }}
      disableSticky
    >
      {title}
    </ListSubheader>

    {children}
  </List>
)
