import { useCallback, useLayoutEffect, useState } from 'react'

export function useResponsiveSvgHeight(viewBoxWidth: number, initialHeight: number) {
  const [container, setContainer] = useState<HTMLDivElement | null>(null)
  const [viewBoxHeight, setViewBoxHeight] = useState(initialHeight)
  const containerRef = useCallback((node: HTMLDivElement | null) => setContainer(node), [])

  useLayoutEffect(() => {
    if (!container) return

    const updateViewBox = () => {
      const { width, height } = container.getBoundingClientRect()
      if (width <= 0 || height <= 0) return
      const nextHeight = (viewBoxWidth * height) / width
      setViewBoxHeight((currentHeight) =>
        Math.abs(currentHeight - nextHeight) < 0.5 ? currentHeight : nextHeight
      )
    }

    let frameId = 0
    const scheduleUpdate = () => {
      updateViewBox()
      if (typeof requestAnimationFrame !== 'function') return
      if (frameId !== 0) cancelAnimationFrame(frameId)
      frameId = requestAnimationFrame(() => {
        frameId = 0
        updateViewBox()
      })
    }

    const observer =
      typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(scheduleUpdate)

    updateViewBox()
    scheduleUpdate()
    observer?.observe(container)
    window.addEventListener('resize', scheduleUpdate)
    window.visualViewport?.addEventListener('resize', scheduleUpdate)
    return () => {
      observer?.disconnect()
      if (frameId !== 0) cancelAnimationFrame(frameId)
      window.removeEventListener('resize', scheduleUpdate)
      window.visualViewport?.removeEventListener('resize', scheduleUpdate)
    }
  }, [container, viewBoxWidth])

  return { containerRef, viewBoxHeight }
}
