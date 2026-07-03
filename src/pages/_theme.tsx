import getSystem from '@/utils/get-system'
const OS = getSystem()

// default theme setting
export const defaultTheme = {
  primary_color: '#4B57D6',
  secondary_color: '#7B86FF',
  primary_text: '#16161A',
  secondary_text: '#7C7C82',
  info_color: '#4B57D6',
  error_color: '#D64545',
  warning_color: '#C69D42',
  success_color: '#1F9D57',
  background_color: '#EEEDEB',
  font_family: `"Montserrat", -apple-system, BlinkMacSystemFont,"Microsoft YaHei UI", "Microsoft YaHei", Roboto, "Helvetica Neue", Arial, sans-serif, "Apple Color Emoji"${
    OS === 'windows' ? ', twemoji mozilla' : ''
  }`,
}

// dark mode
export const defaultDarkTheme = {
  ...defaultTheme,
  primary_color: '#7B86FF',
  secondary_color: '#9CA5FF',
  primary_text: '#F3F3F5',
  background_color: '#0D0D0F',
  secondary_text: '#8B8B94',
  info_color: '#7B86FF',
  error_color: '#FF6B6B',
  warning_color: '#F2DCA8',
  success_color: '#3ECF8E',
}
