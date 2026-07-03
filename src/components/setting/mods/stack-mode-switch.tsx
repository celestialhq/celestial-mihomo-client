import { Button, ButtonGroup } from '@mui/material'

import {
  segmentedButtonSx,
  segmentedGroupSx,
} from '@/components/base/segmented-sx'

interface Props {
  value?: string
  onChange?: (value: string) => void
}

const STACK_OPTIONS = ['system', 'gvisor', 'mixed'] as const
const STACK_LABELS: Record<(typeof STACK_OPTIONS)[number], string> = {
  system: 'System',
  gvisor: 'gVisor',
  mixed: 'Mixed',
}

export const StackModeSwitch = (props: Props) => {
  const { value, onChange } = props

  return (
    <ButtonGroup
      size="small"
      disableElevation
      sx={{ my: '4px', ...segmentedGroupSx }}
    >
      {STACK_OPTIONS.map((opt) => {
        const active = value?.toLowerCase() === opt
        return (
          <Button
            key={opt}
            onClick={() => onChange?.(opt)}
            sx={segmentedButtonSx(active)}
          >
            {STACK_LABELS[opt]}
          </Button>
        )
      })}
    </ButtonGroup>
  )
}
