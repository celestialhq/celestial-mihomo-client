import { Button, ButtonGroup } from '@mui/material'
import { useTranslation } from 'react-i18next'

type ThemeValue = IVergeConfig['theme_mode']

interface Props {
  value?: ThemeValue
  onChange?: (value: ThemeValue) => void
}

export const ThemeModeSwitch = (props: Props) => {
  const { onChange } = props
  const { t } = useTranslation()

  return (
    <ButtonGroup size="small" sx={{ my: '4px' }}>
      <Button
        variant="contained"
        onClick={() => onChange?.('dark')}
        sx={{ textTransform: 'capitalize' }}
      >
        {t('settings.sections.appearance.dark')}
      </Button>
    </ButtonGroup>
  )
}
