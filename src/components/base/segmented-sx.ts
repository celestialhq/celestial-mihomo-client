// Shared sx for the flat pill segmented controls used across Home/Proxies/Settings
// (network mode, work mode, theme mode, stack mode, connections tabs, etc.)
export const segmentedGroupSx = {
  border: '1px solid var(--border)',
  borderRadius: '9px',
  overflow: 'hidden',
  '& .MuiButtonGroup-grouped': { border: 'none' },
} as const

export const segmentedButtonSx = (active: boolean) => ({
  textTransform: 'none' as const,
  fontWeight: 600,
  fontFamily: "'Montserrat', sans-serif",
  bgcolor: active ? 'var(--accent)' : 'transparent',
  color: active ? '#fff' : 'var(--text2)',
  '&:hover': {
    bgcolor: active ? 'var(--accent)' : 'var(--track)',
  },
})
