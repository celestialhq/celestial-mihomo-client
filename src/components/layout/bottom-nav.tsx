import { Box } from '@mui/material'
import { useState } from 'react'
import {
  useLocation,
  useMatch,
  useNavigate,
  useResolvedPath,
} from 'react-router'

import { MoreNavIcon } from '@/components/layout/nav-icons'
import type { navItems as NavItemsType } from '@/pages/_routers'

type NavItem = (typeof NavItemsType)[number]

const PRIMARY_COUNT = 4

interface NavButtonProps {
  to: string
  icon: React.ReactNode
  label: string
  active: boolean
  onClick: () => void
}

const NavButton = ({ icon, label, active, onClick }: NavButtonProps) => (
  <Box
    component="button"
    onClick={onClick}
    sx={{
      flex: 1,
      display: 'flex',
      flexDirection: 'column',
      alignItems: 'center',
      gap: '3px',
      padding: '4px 0',
      border: 'none',
      background: 'transparent',
      cursor: 'pointer',
      color: active ? 'var(--accent)' : 'var(--text3)',
      fontFamily: "'Montserrat', sans-serif",
      '& svg': { fontSize: 21 },
    }}
  >
    {icon}
    <Box component="span" sx={{ fontSize: '8.5px', fontWeight: 600 }}>
      {label}
    </Box>
  </Box>
)

interface Props {
  items: NavItem[]
  labels: (item: NavItem) => string
  moreLabel: string
}

export const BottomNav = ({ items, labels, moreLabel }: Props) => {
  const navigate = useNavigate()
  const { pathname } = useLocation()
  const [moreOpen, setMoreOpen] = useState(false)
  const primary = items.slice(0, PRIMARY_COUNT)
  const rest = items.slice(PRIMARY_COUNT)
  const isRestActive = rest.some((item) => pathname === item.path)

  return (
    <>
      <Box
        component="nav"
        sx={{
          flex: 'none',
          display: 'flex',
          justifyContent: 'space-around',
          alignItems: 'center',
          padding: '8px 4px calc(8px + env(safe-area-inset-bottom))',
          background: 'var(--panel)',
          borderTop: '1px solid var(--border)',
        }}
      >
        {primary.map((item) => (
          <BottomNavButton
            key={item.path}
            item={item}
            label={labels(item)}
            onNavigate={() => {
              setMoreOpen(false)
              navigate(item.path)
            }}
          />
        ))}
        <NavButton
          to="#more"
          icon={<MoreNavIcon />}
          label={moreLabel}
          active={moreOpen || isRestActive}
          onClick={() => setMoreOpen(true)}
        />
      </Box>

      {moreOpen && (
        <Box
          onClick={() => setMoreOpen(false)}
          sx={{
            position: 'fixed',
            inset: 0,
            background: 'rgba(0,0,0,.5)',
            zIndex: 40,
            display: 'flex',
            alignItems: 'flex-end',
          }}
        >
          <Box
            onClick={(e) => e.stopPropagation()}
            sx={{
              width: '100%',
              background: 'var(--panel)',
              borderTopLeftRadius: '18px',
              borderTopRightRadius: '18px',
              padding: '10px 14px calc(20px + env(safe-area-inset-bottom))',
            }}
          >
            <Box
              sx={{
                width: 38,
                height: 4,
                borderRadius: '2px',
                background: 'var(--border2)',
                margin: '6px auto 14px',
              }}
            />
            {rest.map((item) => (
              <MoreSheetItem
                key={item.path}
                item={item}
                label={labels(item)}
                onNavigate={() => {
                  setMoreOpen(false)
                  navigate(item.path)
                }}
              />
            ))}
          </Box>
        </Box>
      )}
    </>
  )
}

const BottomNavButton = ({
  item,
  label,
  onNavigate,
}: {
  item: NavItem
  label: string
  onNavigate: () => void
}) => {
  const resolved = useResolvedPath(item.path)
  const match = useMatch({ path: resolved.pathname, end: true })
  return (
    <NavButton
      to={item.path}
      icon={item.icon[0]}
      label={label}
      active={!!match}
      onClick={onNavigate}
    />
  )
}

const MoreSheetItem = ({
  item,
  label,
  onNavigate,
}: {
  item: NavItem
  label: string
  onNavigate: () => void
}) => {
  const resolved = useResolvedPath(item.path)
  const match = useMatch({ path: resolved.pathname, end: true })
  const active = !!match
  return (
    <Box
      component="button"
      onClick={onNavigate}
      sx={{
        display: 'flex',
        alignItems: 'center',
        gap: '13px',
        width: '100%',
        padding: '13px 12px',
        border: 'none',
        background: active ? 'var(--accent-bg)' : 'transparent',
        color: active ? 'var(--accent)' : 'var(--text)',
        borderRadius: '10px',
        font: "600 14px 'Montserrat'",
        cursor: 'pointer',
        textAlign: 'left',
        '& svg': { fontSize: 19 },
      }}
    >
      {item.icon[0]}
      {label}
    </Box>
  )
}
