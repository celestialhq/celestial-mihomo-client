import type {
  DraggableAttributes,
  DraggableSyntheticListeners,
} from '@dnd-kit/core'
import {
  alpha,
  ListItem,
  ListItemButton,
  ListItemIcon,
  ListItemText,
} from '@mui/material'
import type { CSSProperties, ReactNode } from 'react'
import { useMatch, useNavigate, useResolvedPath } from 'react-router'

import { useVerge } from '@/hooks/use-verge'

interface SortableProps {
  setNodeRef?: (element: HTMLElement | null) => void
  attributes?: DraggableAttributes
  listeners?: DraggableSyntheticListeners
  style?: CSSProperties
  isDragging?: boolean
  disabled?: boolean
}

interface Props {
  to: string
  children: string
  icon: ReactNode[]
  sortable?: SortableProps
}
export const LayoutItem = (props: Props) => {
  const { to, children, icon, sortable } = props
  const { verge } = useVerge()
  const { menu_icon } = verge ?? {}
  const navCollapsed = verge?.collapse_navbar ?? false
  const resolved = useResolvedPath(to)
  const match = useMatch({ path: resolved.pathname, end: true })
  const navigate = useNavigate()

  const effectiveMenuIcon =
    navCollapsed && menu_icon === 'disable' ? 'monochrome' : menu_icon

  const { setNodeRef, attributes, listeners, style, isDragging, disabled } =
    sortable ?? {}

  const draggable = Boolean(sortable) && !disabled
  const dragHandleProps = draggable
    ? { ...(attributes ?? {}), ...(listeners ?? {}) }
    : undefined

  return (
    <ListItem
      ref={setNodeRef}
      style={style}
        sx={[
        { py: 0.5, maxWidth: 250, mx: 'auto', padding: '4px 0px' },
        isDragging ? { opacity: 0.78 } : {},
      ]}
    >
      <ListItemButton
        selected={!!match}
        {...(dragHandleProps ?? {})}
        sx={[
          {
            borderRadius: 2,
            marginLeft: 1.25,
            paddingLeft: 1.5,
            paddingRight: 1.25,
            marginRight: 1.25,
            minHeight: 48,
            gap: 1.5,
            justifyContent: 'flex-start',
            cursor: draggable ? 'grab' : 'pointer',
            transition:
              'background-color 0.2s ease, color 0.2s ease, transform 0.2s ease',
            '&:active': draggable ? { cursor: 'grabbing' } : {},
            '&:hover': {
              transform: 'translateX(2px)',
            },
            '& .MuiListItemText-primary': {
              color: 'text.primary',
              fontWeight: '700',
              textAlign: 'left',
            },
          },
          ({ palette: { mode, primary, text } }) => {
            const bgcolor =
              mode === 'light'
                ? alpha(primary.main, 0.14)
                : alpha(primary.main, 0.24)
            const hoverBgcolor =
              mode === 'light'
                ? alpha(primary.main, 0.08)
                : alpha(primary.main, 0.16)
            const color = mode === 'light' ? '#075985' : '#E0F2FE'
            return {
              '&:hover': { bgcolor: hoverBgcolor },
              '&.Mui-selected': { bgcolor },
              '&.Mui-selected:hover': { bgcolor },
              '&.Mui-selected .MuiListItemText-primary': { color },
              '&.Mui-selected .MuiListItemIcon-root': { color: primary.main },
              '&:not(.Mui-selected) .MuiListItemIcon-root': {
                color: alpha(text.primary, mode === 'light' ? 0.72 : 0.82),
              },
            }
          },
        ]}
        title={navCollapsed ? children : undefined}
        aria-label={navCollapsed ? children : undefined}
        onClick={() => navigate(to)}
      >
        {(effectiveMenuIcon === 'monochrome' || !effectiveMenuIcon) && (
          <ListItemIcon
            sx={{
              color: 'text.primary',
              minWidth: 24,
              marginLeft: 0,
              cursor: draggable ? 'grab' : 'inherit',
            }}
          >
            {icon[0]}
          </ListItemIcon>
        )}
        {effectiveMenuIcon === 'colorful' && (
          <ListItemIcon
            sx={{
              minWidth: 24,
              marginLeft: 0,
              cursor: draggable ? 'grab' : 'inherit',
            }}
          >
            {icon[1]}
          </ListItemIcon>
        )}
        <ListItemText
          sx={{
            minWidth: 0,
            textAlign: 'left',
            marginLeft: 0,
          }}
          primary={children}
        />
      </ListItemButton>
    </ListItem>
  )
}
