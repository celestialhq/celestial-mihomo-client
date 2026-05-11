import getSystem from '@/utils/get-system'
const OS = getSystem()

// default theme setting
export const defaultTheme = {
  primary_color: '#7E67E8',
  secondary_color: '#A58FEF',
  primary_text: '#121820',
  secondary_text: '#627084',
  info_color: '#7E67E8',
  error_color: '#D96C7A',
  warning_color: '#C69D42',
  success_color: '#3F9E83',
  background_color: '#F4F8FC',
  font_family: `-apple-system, BlinkMacSystemFont,"Microsoft YaHei UI", "Microsoft YaHei", Roboto, "Helvetica Neue", Arial, sans-serif, "Apple Color Emoji"${
    OS === 'windows' ? ', twemoji mozilla' : ''
  }`,
}

// dark mode
export const defaultDarkTheme = {
  ...defaultTheme,
  primary_color: '#B9A7FF',
  secondary_color: '#E9E2FF',
  primary_text: '#F3F8FF',
  background_color: '#070B12',
  secondary_text: '#A8B6C8',
  info_color: '#B9A7FF',
  error_color: '#FFB4C2',
  warning_color: '#F2DCA8',
  success_color: '#B8F2E6',
}
