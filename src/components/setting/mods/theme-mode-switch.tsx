import { Button, ButtonGroup } from '@mui/material'
import { useTranslation } from 'react-i18next'

import {
  segmentedButtonSx,
  segmentedGroupSx,
} from '@/components/base/segmented-sx'

type ThemeValue = IVergeConfig['theme_mode']

interface Props {
  value?: ThemeValue
  onChange?: (value: ThemeValue) => void
}

export const ThemeModeSwitch = (props: Props) => {
  const { value, onChange } = props
  const { t } = useTranslation()

  const options: { key: ThemeValue; label: string }[] = [
    { key: 'dark', label: t('settings.sections.appearance.dark') },
    { key: 'light', label: t('settings.sections.appearance.light') },
  ]

  return (
    <ButtonGroup
      size="small"
      disableElevation
      sx={{ my: '4px', ...segmentedGroupSx }}
    >
      {options.map((opt) => {
        const active = (value ?? 'dark') === opt.key
        return (
          <Button
            key={opt.key}
            onClick={() => onChange?.(opt.key)}
            sx={segmentedButtonSx(active)}
          >
            {opt.label}
          </Button>
        )
      })}
    </ButtonGroup>
  )
}
