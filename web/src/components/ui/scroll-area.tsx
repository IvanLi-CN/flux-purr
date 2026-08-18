import type * as React from 'react'
import SimpleBar from 'simplebar-react'

import { cn } from '@/lib/utils'

function ScrollArea({
  className,
  autoHide = true,
  scrollbarMinSize = 64,
  ...props
}: React.ComponentProps<typeof SimpleBar>) {
  return (
    <SimpleBar
      data-slot="scroll-area"
      autoHide={autoHide}
      scrollbarMinSize={scrollbarMinSize}
      className={cn('scroll-area', className)}
      {...props}
    />
  )
}

export { ScrollArea }
