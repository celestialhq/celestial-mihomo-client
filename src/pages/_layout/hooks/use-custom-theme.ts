import { alpha, createTheme, Theme as MuiTheme, Shadows } from '@mui/material'
import {
  getCurrentWebviewWindow,
  WebviewWindow,
} from '@tauri-apps/api/webviewWindow'
import { Theme as TauriOsTheme } from '@tauri-apps/api/window'
import { useEffect, useMemo } from 'react'

import { useVerge } from '@/hooks/use-verge'
import { defaultDarkTheme, defaultTheme } from '@/pages/_theme'
import { useSetThemeMode, useThemeMode } from '@/services/states'

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
  const { theme_setting, theme_mode } = verge ?? {}
  const setMode = useSetThemeMode()
  const resolvedMode = useThemeMode()
  const mode = resolvedMode ?? 'dark'
  const userBackgroundImage = theme_setting?.background_image || ''
  const hasUserBackground = !!userBackgroundImage

  useEffect(() => {
    const resolveMode = (): 'light' | 'dark' => {
      if (theme_mode === 'light' || theme_mode === 'dark') return theme_mode
      return window.matchMedia?.('(prefers-color-scheme: dark)').matches
        ? 'dark'
        : 'light'
    }

    const applyMode = () => {
      const resolved = resolveMode()
      setMode(resolved)
      appWindow.setTheme(resolved as TauriOsTheme).catch((err) => {
        console.error('Failed to sync window theme:', err)
      })
    }

    applyMode()

    if (theme_mode === 'system' || !theme_mode) {
      const media = window.matchMedia?.('(prefers-color-scheme: dark)')
      media?.addEventListener('change', applyMode)
      return () => media?.removeEventListener('change', applyMode)
    }
  }, [appWindow, setMode, theme_mode])

  const theme = useMemo(() => {
    const setting = theme_setting || {}
    const dt = mode === 'light' ? defaultTheme : defaultDarkTheme
    const tokens =
      mode === 'light'
        ? {
            bg: '#EEEDEB',
            panel: '#FFFFFF',
            railbg: '#FFFFFF',
            card: '#FFFFFF',
            card2: '#F6F6F4',
            border: 'rgba(0, 0, 0, 0.1)',
            border2: 'rgba(0, 0, 0, 0.18)',
            text2: '#7C7C82',
            text3: '#A8A7A3',
            accentBg: '#EEF0FF',
            track: 'rgba(0, 0, 0, 0.08)',
            good: '#1F9D57',
            bad: '#D64545',
            selectColor: '#FFFFFF',
            scrollbarBg: '#EEEDEB',
            scrollbarThumb: 'rgba(0, 0, 0, 0.22)',
            scrollbarThumbHover: 'rgba(0, 0, 0, 0.34)',
          }
        : {
            bg: '#0D0D0F',
            panel: '#141417',
            railbg: '#141417',
            card: '#1A1A1F',
            card2: '#101013',
            border: 'rgba(255, 255, 255, 0.08)',
            border2: 'rgba(255, 255, 255, 0.16)',
            text2: '#8B8B94',
            text3: '#5C5C65',
            accentBg: 'rgba(123, 134, 255, 0.15)',
            track: 'rgba(255, 255, 255, 0.1)',
            good: '#3ECF8E',
            bad: '#FF6B6B',
            selectColor: '#0D0D0F',
            scrollbarBg: '#0D0D0F',
            scrollbarThumb: 'rgba(255, 255, 255, 0.14)',
            scrollbarThumbHover: 'rgba(255, 255, 255, 0.24)',
          }
    const surfaceColor = tokens.card
    const elevatedSurfaceColor = tokens.card2
    const dividerColor = tokens.border
    const accentColor = setting.primary_color || dt.primary_color
    let muiTheme: MuiTheme

    try {
      muiTheme = createTheme({
        breakpoints: {
          values: { xs: 0, sm: 650, md: 900, lg: 1200, xl: 1536 },
        },
        palette: {
          mode,
          primary: { main: accentColor },
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
        shape: { borderRadius: 10 },
        shadows: [
          'none',
          '0 4px 12px rgba(0, 0, 0, 0.14)',
          ...Array(23).fill('0 8px 20px rgba(0, 0, 0, 0.18)'),
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
                  background: accentColor,
                  color: '#FFFFFF',
                  boxShadow: 'none',
                },
              },
              {
                props: { variant: 'outlined', color: 'primary' },
                style: {
                  borderColor: dividerColor,
                  color: accentColor,
                },
              },
            ],
            styleOverrides: {
              root: {
                borderRadius: 9,
                textTransform: 'none',
                fontWeight: 700,
                boxShadow: 'none',
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
        shape: { borderRadius: 10 },
        typography: { fontFamily: dt.font_family },
      })
    }

    const rootEle = document.documentElement
    if (rootEle) {
      const backgroundColor = dt.background_color
      rootEle.style.setProperty('--bg', tokens.bg)
      rootEle.style.setProperty('--panel', tokens.panel)
      rootEle.style.setProperty('--railbg', tokens.railbg)
      rootEle.style.setProperty('--card', tokens.card)
      rootEle.style.setProperty('--card2', tokens.card2)
      rootEle.style.setProperty('--border', tokens.border)
      rootEle.style.setProperty('--border2', tokens.border2)
      rootEle.style.setProperty('--text', muiTheme.palette.text.primary)
      rootEle.style.setProperty('--text2', tokens.text2)
      rootEle.style.setProperty('--text3', tokens.text3)
      rootEle.style.setProperty('--accent', accentColor)
      rootEle.style.setProperty('--accent-bg', tokens.accentBg)
      rootEle.style.setProperty('--track', tokens.track)
      rootEle.style.setProperty('--good', tokens.good)
      rootEle.style.setProperty('--bad', tokens.bad)

      rootEle.style.setProperty('--divider-color', dividerColor)
      rootEle.style.setProperty('--background-color', backgroundColor)
      rootEle.style.setProperty('--surface-color', surfaceColor)
      rootEle.style.setProperty(
        '--surface-color-elevated',
        elevatedSurfaceColor,
      )
      rootEle.style.setProperty('--celestial-main', accentColor)
      rootEle.style.setProperty('--celestial-soft', tokens.accentBg)
      rootEle.style.setProperty('--proxy-accent', accentColor)
      rootEle.style.setProperty('--proxy-accent-soft', tokens.accentBg)
      rootEle.style.setProperty('--text-primary', muiTheme.palette.text.primary)
      rootEle.style.setProperty(
        '--text-secondary',
        muiTheme.palette.text.secondary,
      )
      rootEle.style.setProperty('--selection-color', tokens.selectColor)
      rootEle.style.setProperty('--scroller-color', tokens.border2)
      rootEle.style.setProperty('--primary-main', muiTheme.palette.primary.main)
      rootEle.style.setProperty(
        '--background-color-alpha',
        alpha(muiTheme.palette.primary.main, 0.14),
      )
      rootEle.style.setProperty('--window-border-color', tokens.border)
      rootEle.style.setProperty('--scrollbar-bg', tokens.scrollbarBg)
      rootEle.style.setProperty('--scrollbar-thumb', tokens.scrollbarThumb)
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
        /* scrollbar */
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
          background-color: ${tokens.scrollbarThumbHover};
        }

        /* flat background, optional user image */
        body {
          background: var(--background-color);
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

        .MuiPaper-root {
          border-color: var(--window-border-color) !important;
        }

        .MuiButton-containedPrimary {
          color: #FFFFFF !important;
        }

        .MuiIconButton-root {
          border-radius: 9px;
        }

        .MuiChip-root {
          font-weight: 700;
        }

        .MuiDialog-paper {
          background-color: ${surfaceColor} !important;
          border: 1px solid var(--divider-color);
          border-radius: 12px !important;
        }

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
