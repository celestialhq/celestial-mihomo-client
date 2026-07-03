// Line-icon set ported 1:1 from the Celestial Design mockup (Celestial App.dc.html)
// so the app's chrome (nav rail, bottom nav, theme toggle, brand mark) uses the
// exact same icon language as the design instead of MUI's filled Material icons.
import type { SVGProps } from 'react'

type IconProps = SVGProps<SVGSVGElement> & { size?: number }

const base = ({ size = 20, ...props }: IconProps) => ({
  width: size,
  height: size,
  viewBox: '0 0 24 24',
  fill: 'none' as const,
  stroke: 'currentColor',
  strokeWidth: 1.7,
  strokeLinecap: 'round' as const,
  strokeLinejoin: 'round' as const,
  ...props,
})

export const HomeNavIcon = (props: IconProps) => (
  <svg {...base(props)}>
    <path d="M3 10.5 12 3l9 7.5M5 9.5V20h14V9.5" />
  </svg>
)

export const ProxiesNavIcon = (props: IconProps) => (
  <svg {...base(props)}>
    <path d="M5 12.5a10 10 0 0 1 14 0M8.5 15.5a5 5 0 0 1 7 0M12 19h.01" />
  </svg>
)

export const ProfilesNavIcon = (props: IconProps) => (
  <svg {...base(props)}>
    <rect x="3" y="4" width="18" height="7" rx="1.5" />
    <rect x="3" y="13" width="18" height="7" rx="1.5" />
    <path d="M7 7.5h.01M7 16.5h.01" />
  </svg>
)

export const ConnectionsNavIcon = (props: IconProps) => (
  <svg {...base(props)}>
    <circle cx="12" cy="12" r="9" />
    <path d="M3 12h18M12 3a15 15 0 0 1 0 18 15 15 0 0 1 0-18" />
  </svg>
)

export const RulesNavIcon = (props: IconProps) => (
  <svg {...base(props)}>
    <circle cx="6" cy="6" r="2.5" />
    <circle cx="6" cy="18" r="2.5" />
    <path d="M6 8.5v7M8.5 6h6a3 3 0 0 1 3 3m0 0-2-2m2 2 2-2" />
  </svg>
)

export const LogsNavIcon = (props: IconProps) => (
  <svg {...base(props)}>
    <path d="M4 6h16M4 12h16M4 18h11" />
  </svg>
)

export const SettingsNavIcon = (props: IconProps) => (
  <svg {...base(props)}>
    <circle cx="12" cy="12" r="3" />
    <path d="M19.4 15a1.6 1.6 0 0 0 .3 1.8 2 2 0 1 1-2.8 2.8 1.6 1.6 0 0 0-2.7 1.1 2 2 0 1 1-4 0 1.6 1.6 0 0 0-2.7-1.1 2 2 0 1 1-2.8-2.8A1.6 1.6 0 0 0 3 12a2 2 0 1 1 0-.1" />
  </svg>
)

export const MoreNavIcon = (props: IconProps) => (
  <svg {...base(props)}>
    <circle cx="5" cy="12" r="1.6" fill="currentColor" stroke="none" />
    <circle cx="12" cy="12" r="1.6" fill="currentColor" stroke="none" />
    <circle cx="19" cy="12" r="1.6" fill="currentColor" stroke="none" />
  </svg>
)

export const CelestialMark = (props: IconProps) => (
  <svg {...base({ ...props, stroke: props.stroke ?? '#fff' })}>
    <path d="M17.5 19a4.5 4.5 0 0 0 .5-8.97A6 6 0 0 0 6.34 9.4 4 4 0 0 0 7 17.4" />
    <path d="M12 12v8m-3-5 3-3 3 3" />
  </svg>
)

export const SunIcon = (props: IconProps) => (
  <svg {...base(props)}>
    <circle cx="12" cy="12" r="4" />
    <path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4" />
  </svg>
)

export const MoonIcon = (props: IconProps) => (
  <svg {...base(props)}>
    <path d="M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z" />
  </svg>
)
