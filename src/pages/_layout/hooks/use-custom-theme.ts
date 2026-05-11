import { alpha, createTheme, Theme as MuiTheme, Shadows } from '@mui/material'
import {
  getCurrentWebviewWindow,
  WebviewWindow,
} from '@tauri-apps/api/webviewWindow'
import { Theme as TauriOsTheme } from '@tauri-apps/api/window'
import { useEffect, useMemo } from 'react'

import { useVerge } from '@/hooks/use-verge'
import { defaultDarkTheme } from '@/pages/_theme'
import { useSetThemeMode } from '@/services/states'

const CSS_INJECTION_SCOPE_ROOT = '[data-css-injection-root]'
const CSS_INJECTION_SCOPE_LIMIT =
  ':is(.monaco-editor .view-lines, .monaco-editor .view-line, .monaco-editor .margin, .monaco-editor .margin-view-overlays, .monaco-editor .view-overlays, .monaco-editor [class^="mtk"], .monaco-editor [class*=" mtk"])'
const TOP_LEVEL_AT_RULES = [
  '@charset',
  '@import',
  '@namespace',
  '@font-face',
  '@keyframes',
  '@counter-style',
  '@page',
  '@property',
  '@font-feature-values',
  '@color-profile',
]
let cssScopeSupport: boolean | null = null

const canUseCssScope = () => {
  if (cssScopeSupport !== null) {
    return cssScopeSupport
  }
  try {
    const testStyle = document.createElement('style')
    testStyle.textContent = '@scope (:root) { }'
    document.head.appendChild(testStyle)
    cssScopeSupport = !!testStyle.sheet?.cssRules?.length
    document.head.removeChild(testStyle)
  } catch {
    cssScopeSupport = false
  }
  return cssScopeSupport
}

const wrapCssInjectionWithScope = (css?: string) => {
  if (!css?.trim()) {
    return ''
  }
  const lowerCss = css.toLowerCase()
  const hasTopLevelOnlyRule = TOP_LEVEL_AT_RULES.some((rule) =>
    lowerCss.includes(rule),
  )
  if (hasTopLevelOnlyRule) {
    return null
  }
  const scopeRoot = CSS_INJECTION_SCOPE_ROOT
  const scopeLimit = CSS_INJECTION_SCOPE_LIMIT
  const scopedBlock = `@scope (${scopeRoot}) to (${scopeLimit}) {
${css}
}`
  return scopedBlock
}

/**
 * custom theme
 */
export const useCustomTheme = () => {
  const appWindow: WebviewWindow = useMemo(() => getCurrentWebviewWindow(), [])
  const { verge } = useVerge()
  const { theme_setting } = verge ?? {}
  const mode = 'dark' as const
  const setMode = useSetThemeMode()
  const userBackgroundImage = theme_setting?.background_image || ''
  const hasUserBackground = !!userBackgroundImage

  useEffect(() => {
    setMode('dark')
    appWindow.setTheme('dark' as TauriOsTheme).catch((err) => {
      console.error('Failed to force dark window theme:', err)
    })
  }, [appWindow, setMode])

  const theme = useMemo(() => {
    const setting = theme_setting || {}
    const dt = defaultDarkTheme
    const surfaceColor = '#101318'
    const elevatedSurfaceColor = '#171B22'
    const dividerColor = 'rgba(185, 167, 255, 0.16)'
    let muiTheme: MuiTheme

    try {
      muiTheme = createTheme({
        breakpoints: {
          values: { xs: 0, sm: 650, md: 900, lg: 1200, xl: 1536 },
        },
        palette: {
          mode,
          primary: { main: setting.primary_color || dt.primary_color },
          secondary: { main: setting.secondary_color || dt.secondary_color },
          info: { main: setting.info_color || dt.info_color },
          error: { main: setting.error_color || dt.error_color },
          warning: { main: setting.warning_color || dt.warning_color },
          success: { main: setting.success_color || dt.success_color },
          divider: dividerColor,
          text: {
            primary: setting.primary_text || dt.primary_text,
            secondary: setting.secondary_text || dt.secondary_text,
          },
          background: {
            paper: surfaceColor,
            default: dt.background_color,
          },
        },
        shape: { borderRadius: 8 },
        shadows: [
          'none',
          '0 18px 38px rgba(0, 0, 0, 0.28)',
          ...Array(23).fill('0 20px 44px rgba(0, 0, 0, 0.34)'),
        ] as Shadows,
        typography: {
          fontFamily: setting.font_family
            ? `${setting.font_family}, ${dt.font_family}`
            : dt.font_family,
        },
        components: {
          MuiButton: {
            variants: [
              {
                props: { variant: 'contained', color: 'primary' },
                style: {
                  background:
                    'linear-gradient(135deg, #E9E2FF 0%, #B9A7FF 100%)',
                  color: '#071018',
                  boxShadow: '0 10px 24px rgba(185, 167, 255, 0.18)',
                },
              },
              {
                props: { variant: 'outlined', color: 'primary' },
                style: {
                  borderColor: 'rgba(185, 167, 255, 0.38)',
                  color: '#E9E2FF',
                },
              },
            ],
            styleOverrides: {
              root: {
                borderRadius: 8,
                textTransform: 'none',
                fontWeight: 800,
              },
            },
          },
          MuiPaper: {
            styleOverrides: {
              root: {
                backgroundImage: 'none',
              },
            },
          },
          MuiMenu: {
            styleOverrides: {
              paper: {
                border: `1px solid ${dividerColor}`,
                backgroundColor: surfaceColor,
                backgroundImage: 'none',
              },
            },
          },
          MuiTooltip: {
            styleOverrides: {
              tooltip: {
                borderRadius: 8,
                fontWeight: 600,
              },
            },
          },
        },
      })
    } catch (e) {
      console.error('Error creating MUI theme, falling back to defaults:', e)
      muiTheme = createTheme({
        breakpoints: {
          values: { xs: 0, sm: 650, md: 900, lg: 1200, xl: 1536 },
        },
        palette: {
          mode,
          primary: { main: dt.primary_color },
          secondary: { main: dt.secondary_color },
          info: { main: dt.info_color },
          error: { main: dt.error_color },
          warning: { main: dt.warning_color },
          success: { main: dt.success_color },
          divider: dividerColor,
          text: { primary: dt.primary_text, secondary: dt.secondary_text },
          background: {
            paper: surfaceColor,
            default: dt.background_color,
          },
        },
        shape: { borderRadius: 8 },
        typography: { fontFamily: dt.font_family },
      })
    }

    const rootEle = document.documentElement
    if (rootEle) {
      const backgroundColor = dt.background_color
      const selectColor = '#B9A7FF'
      const scrollColor = 'rgba(185, 167, 255, 0.34)'
      rootEle.style.setProperty('--divider-color', dividerColor)
      rootEle.style.setProperty('--background-color', backgroundColor)
      rootEle.style.setProperty('--surface-color', surfaceColor)
      rootEle.style.setProperty(
        '--surface-color-elevated',
        elevatedSurfaceColor,
      )
      rootEle.style.setProperty('--celestial-main', '#B9A7FF')
      rootEle.style.setProperty('--celestial-soft', 'rgba(185, 167, 255, 0.16)')
      rootEle.style.setProperty('--proxy-accent', '#B9A7FF')
      rootEle.style.setProperty(
        '--proxy-accent-soft',
        'rgba(185, 167, 255, 0.16)',
      )
      rootEle.style.setProperty('--text-primary', muiTheme.palette.text.primary)
      rootEle.style.setProperty(
        '--text-secondary',
        muiTheme.palette.text.secondary,
      )
      rootEle.style.setProperty('--selection-color', selectColor)
      rootEle.style.setProperty('--scroller-color', scrollColor)
      rootEle.style.setProperty('--primary-main', muiTheme.palette.primary.main)
      rootEle.style.setProperty(
        '--background-color-alpha',
        alpha(muiTheme.palette.primary.main, 0.14),
      )
      rootEle.style.setProperty(
        '--window-border-color',
        'rgba(185, 167, 255, 0.16)',
      )
      rootEle.style.setProperty('--scrollbar-bg', '#070B12')
      rootEle.style.setProperty(
        '--scrollbar-thumb',
        'rgba(185, 167, 255, 0.28)',
      )
      rootEle.style.setProperty(
        '--user-background-image',
        hasUserBackground ? `url('${userBackgroundImage}')` : 'none',
      )
      rootEle.style.setProperty(
        '--background-blend-mode',
        setting.background_blend_mode || 'normal',
      )
      rootEle.style.setProperty(
        '--background-opacity',
        setting.background_opacity !== undefined
          ? String(setting.background_opacity)
          : '1',
      )
      rootEle.setAttribute('data-css-injection-root', 'true')
    }

    let styleElement = document.querySelector('style#verge-theme')
    if (!styleElement) {
      styleElement = document.createElement('style')
      styleElement.id = 'verge-theme'
      document.head.appendChild(styleElement!)
    }

    if (styleElement) {
      let scopedCss: string | null = null
      if (canUseCssScope() && setting.css_injection) {
        scopedCss = wrapCssInjectionWithScope(setting.css_injection)
      }
      const effectiveInjectedCss = scopedCss ?? setting.css_injection ?? ''
      const globalStyles = `
        /* 修复滚动条样式 */
        ::-webkit-scrollbar {
          width: 8px;
          height: 8px;
          background-color: var(--scrollbar-bg);
        }
        ::-webkit-scrollbar-thumb {
          background-color: var(--scrollbar-thumb);
          border-radius: 4px;
        }
        ::-webkit-scrollbar-thumb:hover {
          background-color: rgba(185, 167, 255, 0.52);
        }

        /* 背景图处理 */
        body {
          background:
            linear-gradient(rgba(255,255,255,0.025) 1px, transparent 1px),
            linear-gradient(90deg, rgba(255,255,255,0.025) 1px, transparent 1px),
            radial-gradient(circle at 82% 8%, rgba(185,167,255,0.18), transparent 24%),
            radial-gradient(circle at 12% 86%, rgba(255,255,255,0.07), transparent 28%),
            var(--background-color);
          background-size: 34px 34px, 34px 34px, auto, auto, auto;
          ${
            hasUserBackground
              ? `
            background-image: var(--user-background-image);
            background-size: cover;
            background-position: center;
            background-attachment: fixed;
            background-blend-mode: var(--background-blend-mode);
            opacity: var(--background-opacity);
          `
              : ''
          }
        }

        /* 修复可能的白色边框 */
        .MuiPaper-root {
          border-color: var(--window-border-color) !important;
        }

        .MuiButton-containedPrimary {
          color: #071018 !important;
        }

        .MuiIconButton-root {
          border-radius: 8px;
        }

        .MuiChip-root {
          font-weight: 800;
        }

        /* 确保模态框和对话框也使用暗色主题 */
        .MuiDialog-paper {
          background-color: ${surfaceColor} !important;
          border: 1px solid var(--divider-color);
          border-radius: 8px !important;
        }

        /* 移除可能的白色点或线条 */
        * {
          outline: none !important;
        }
      `

      styleElement.innerHTML = effectiveInjectedCss + globalStyles
    }

    return muiTheme
  }, [mode, theme_setting, userBackgroundImage, hasUserBackground])

  useEffect(() => {
    const id = setTimeout(() => {
      const dom = document.querySelector('#Gradient2')
      if (dom) {
        dom.innerHTML = `
        <stop offset="0%" stop-color="${theme.palette.primary.main}" />
        <stop offset="80%" stop-color="${theme.palette.primary.dark}" />
        <stop offset="100%" stop-color="${theme.palette.primary.dark}" />
        `
      }
    }, 0)
    return () => clearTimeout(id)
  }, [theme.palette.primary.main, theme.palette.primary.dark])

  return { theme }
}
