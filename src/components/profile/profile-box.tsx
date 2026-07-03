import { Box, styled } from '@mui/material'

export const ProfileBox = styled(Box)(({ 'aria-selected': selected }) => {
  return {
    position: 'relative',
    display: 'block',
    cursor: 'pointer',
    textAlign: 'left',
    padding: '12px 16px 10px',
    boxSizing: 'border-box',
    background: selected ? 'var(--accent-bg)' : 'var(--card)',
    width: '100%',
    borderRadius: '12px',
    minHeight: 98,
    color: 'var(--text2)',
    border: `1px solid ${selected ? 'var(--accent)' : 'var(--border)'}`,
    boxShadow: 'none',
    overflow: 'hidden',
    transition: 'border-color 0.15s ease, background-color 0.15s ease',
    '&:hover': {
      borderColor: selected ? 'var(--accent)' : 'var(--border2)',
    },
    '& h2': { color: selected ? 'var(--accent)' : 'var(--text)' },
    '& .MuiLinearProgress-root': {
      height: 5,
      borderRadius: 999,
      marginTop: 8,
      backgroundColor: 'var(--track)',
    },
    '& .MuiLinearProgress-bar': {
      backgroundColor: 'var(--accent)',
    },
  }
})
