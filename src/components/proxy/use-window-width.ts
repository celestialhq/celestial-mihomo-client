import { RefObject, useEffect, useState } from 'react'

// Measures the actual proxy-list container width (not the full app window)
// so the responsive column count reflects space really available next to
// the nav rail, matching the Celestial design's `minmax(220px,1fr)` grid.
export const useWindowWidth = (
  containerRef?: RefObject<HTMLElement | null>,
) => {
  const [width, setWidth] = useState(
    () => containerRef?.current?.clientWidth ?? document.body.clientWidth,
  )

  useEffect(() => {
    const el = containerRef?.current
    if (!el) {
      const handleResize = () => {
        // eslint-disable-next-line @eslint-react/set-state-in-effect
        setWidth(document.body.clientWidth)
      }
      handleResize()
      window.addEventListener('resize', handleResize)
      return () => window.removeEventListener('resize', handleResize)
    }

    const measure = () => {
      // eslint-disable-next-line @eslint-react/set-state-in-effect
      setWidth(el.clientWidth)
    }
    measure()

    if (typeof ResizeObserver === 'undefined') {
      window.addEventListener('resize', measure)
      return () => window.removeEventListener('resize', measure)
    }
    const ro = new ResizeObserver(measure)
    ro.observe(el)
    return () => ro.disconnect()
  }, [containerRef])

  return { width }
}
