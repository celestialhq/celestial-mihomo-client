import { alpha, Box, styled } from '@mui/material'

export const ProfileBox = styled(Box)(
  ({ theme, 'aria-selected': selected }) => {
    const { mode, primary, text } = theme.palette
    const key = `${mode}-${!!selected}`

    const backgroundColor =
      mode === 'light'
        ? 'linear-gradient(135deg, #ffffff 0%, #f8fbff 100%)'
        : 'linear-gradient(135deg, rgba(36, 39, 53, 0.98) 0%, rgba(25, 34, 50, 0.98) 100%)'

    const color = {
      'light-true': text.secondary,
      'light-false': text.secondary,
      'dark-true': alpha(text.secondary, 0.65),
      'dark-false': alpha(text.secondary, 0.65),
    }[key]!

    const h2color = {
      'light-true': primary.main,
      'light-false': text.primary,
      'dark-true': primary.main,
      'dark-false': text.primary,
    }[key]!

    const borderSelect = {
      'light-true': {
        borderLeft: `3px solid ${primary.main}`,
        width: `calc(100% + 3px)`,
        marginLeft: `-3px`,
      },
      'light-false': {
        width: '100%',
      },
      'dark-true': {
        borderLeft: `3px solid ${primary.main}`,
        width: `calc(100% + 3px)`,
        marginLeft: `-3px`,
      },
      'dark-false': {
        width: '100%',
      },
    }[key]

    return {
      position: 'relative',
      display: 'block',
      cursor: 'pointer',
      textAlign: 'left',
      padding: '12px 16px 10px',
      boxSizing: 'border-box',
      background: backgroundColor,
      ...borderSelect,
      borderRadius: '8px',
      minHeight: 98,
      color,
      border: `1px solid ${alpha(mode === 'light' ? text.primary : '#fff', mode === 'light' ? 0.08 : 0.06)}`,
      boxShadow:
        mode === 'light'
          ? '0 10px 24px rgba(15, 23, 42, 0.06)'
          : '0 14px 30px rgba(0, 0, 0, 0.22)',
      overflow: 'hidden',
      transition:
        'transform 0.18s ease, box-shadow 0.18s ease, border-color 0.18s ease',
      '&::before': {
        content: '""',
        position: 'absolute',
        top: 0,
        left: 0,
        right: 0,
        height: 2,
        background: selected
          ? `linear-gradient(90deg, ${primary.main}, ${theme.palette.info.main})`
          : 'transparent',
      },
      '&:hover': {
        transform: 'translateY(-1px)',
        borderColor: alpha(primary.main, mode === 'light' ? 0.28 : 0.36),
        boxShadow:
          mode === 'light'
            ? '0 14px 30px rgba(15, 23, 42, 0.09)'
            : '0 18px 36px rgba(0, 0, 0, 0.28)',
      },
      '& h2': { color: h2color },
      '& .MuiLinearProgress-root': {
        height: 3,
        borderRadius: 999,
        marginTop: 8,
        backgroundColor: alpha(primary.main, 0.12),
      },
    }
  },
)
