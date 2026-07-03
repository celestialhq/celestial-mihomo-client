import { styled } from '@mui/material/styles'
import { default as MuiSwitch, SwitchProps } from '@mui/material/Switch'

export const Switch = styled((props: SwitchProps) => (
  <MuiSwitch
    focusVisibleClassName=".Mui-focusVisible"
    disableRipple
    {...props}
  />
))(({ theme }) => ({
  width: 40,
  height: 23,
  padding: 0,
  marginRight: 1,
  '& .MuiSwitch-switchBase': {
    padding: 0,
    margin: 3,
    transitionDuration: '150ms',
    '&.Mui-checked': {
      transform: 'translateX(17px)',
      color: '#fff',
      '& + .MuiSwitch-track': {
        backgroundColor: 'var(--accent)',
        opacity: 1,
        border: 0,
      },
      '&.Mui-disabled + .MuiSwitch-track': {
        opacity: 0.5,
      },
    },
    '&.Mui-focusVisible .MuiSwitch-thumb': {
      border: '6px solid #fff',
    },
    '&.Mui-disabled .MuiSwitch-thumb': {
      color:
        theme.palette.mode === 'light'
          ? theme.palette.grey[100]
          : theme.palette.grey[600],
    },
    '&.Mui-disabled + .MuiSwitch-track': {
      opacity: theme.palette.mode === 'light' ? 0.7 : 0.3,
    },
  },
  '& .MuiSwitch-thumb': {
    boxSizing: 'border-box',
    width: 17,
    height: 17,
    boxShadow: 'none',
  },
  '& .MuiSwitch-track': {
    borderRadius: 12,
    backgroundColor: 'var(--track)',
    opacity: 1,
    transition: theme.transitions.create(['background-color'], {
      duration: 150,
    }),
  },
}))
