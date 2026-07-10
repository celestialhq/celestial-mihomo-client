import { styled, Box, Typography } from '@mui/material'
import { Rule } from 'tauri-plugin-mihomo-api'

const Item = styled(Box)(() => ({
  display: 'flex',
  alignItems: 'center',
  gap: '14px',
  padding: '12px 16px',
  color: 'var(--text)',
}))

interface Props {
  value: Rule & { lineNo: number }
}

const targetColor = (text: string) => {
  if (text === 'REJECT' || text === 'REJECT-DROP') return 'var(--bad)'
  if (text === 'DIRECT') return 'var(--text2)'
  return 'var(--accent)'
}

const RuleItem = (props: Props) => {
  const { value } = props

  return (
    <Item sx={{ borderTop: '1px solid var(--border)' }}>
      <Typography
        sx={{
          fontFamily: "'JetBrains Mono', monospace",
          fontSize: 11,
          fontWeight: 600,
          color: 'var(--text3)',
          width: 22,
          flex: 'none',
          textAlign: 'right',
        }}
      >
        {value.lineNo}
      </Typography>

      <Box sx={{ userSelect: 'text', minWidth: 0, flex: 1 }}>
        <Typography
          component="div"
          noWrap
          sx={{
            fontFamily: "'Montserrat', sans-serif",
            fontSize: 13,
            fontWeight: 500,
            color: 'var(--text)',
          }}
        >
          {value.payload || '-'}
        </Typography>

        <Typography
          component="div"
          sx={{
            fontFamily: "'JetBrains Mono', monospace",
            fontSize: 10,
            fontWeight: 600,
            letterSpacing: '0.04em',
            color: 'var(--text3)',
            mt: '2px',
          }}
        >
          {value.type}
        </Typography>
      </Box>

      <Typography
        component="span"
        sx={{
          fontFamily: "'Montserrat', sans-serif",
          fontSize: 11.5,
          fontWeight: 600,
          color: targetColor(value.proxy),
          flex: 'none',
        }}
      >
        {value.proxy}
      </Typography>
    </Item>
  )
}

export default RuleItem
