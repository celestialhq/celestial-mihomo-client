import getSystem from '@/utils/get-system'
const OS = getSystem()

// default theme setting
export const defaultTheme = {
  primary_color: '#3478C6',
  secondary_color: '#2AA7B8',
  primary_text: '#152235',
  secondary_text: '#66758A',
  info_color: '#2D8ACF',
  error_color: '#EF4444',
  warning_color: '#F59E0B',
  success_color: '#10B981',
  background_color: '#F3F7FB',
  font_family: `-apple-system, BlinkMacSystemFont,"Microsoft YaHei UI", "Microsoft YaHei", Roboto, "Helvetica Neue", Arial, sans-serif, "Apple Color Emoji"${
    OS === 'windows' ? ', twemoji mozilla' : ''
  }`,
}

// dark mode
export const defaultDarkTheme = {
  ...defaultTheme,
  primary_color: '#7DB7FF',
  secondary_color: '#22D3EE',
  primary_text: '#F8FAFC',
  background_color: '#0F172A',
  secondary_text: '#CBD5E1',
  info_color: '#38BDF8',
  error_color: '#F87171',
  warning_color: '#FBBF24',
  success_color: '#34D399',
}
