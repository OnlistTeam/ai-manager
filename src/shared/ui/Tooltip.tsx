import * as React from "react";
import * as TooltipPrimitive from "@radix-ui/react-tooltip";
import { cn } from "./cn";

const TooltipProvider = TooltipPrimitive.Provider;
const Tooltip = TooltipPrimitive.Root;
const TooltipTrigger = TooltipPrimitive.Trigger;

const TooltipContent = React.forwardRef<
  React.ElementRef<typeof TooltipPrimitive.Content>,
  React.ComponentPropsWithoutRef<typeof TooltipPrimitive.Content>
>(({ children, className, sideOffset = 6, ...props }, ref) => (
  <TooltipPrimitive.Portal>
    <TooltipPrimitive.Content
      ref={ref}
      sideOffset={sideOffset}
      className={cn(
        "pointer-events-none z-[100] max-w-xs select-none overflow-hidden rounded-md bg-content px-2.5 py-1.5 text-caption text-canvas shadow-lg",
        "motion-safe:animate-in motion-safe:fade-in-0 motion-safe:zoom-in-95",
        "data-[state=closed]:motion-safe:animate-out data-[state=closed]:motion-safe:fade-out-0 data-[state=closed]:motion-safe:zoom-out-95",
        "data-[side=bottom]:motion-safe:slide-in-from-top-1 data-[side=left]:motion-safe:slide-in-from-right-1 data-[side=right]:motion-safe:slide-in-from-left-1 data-[side=top]:motion-safe:slide-in-from-bottom-1",
        className,
      )}
      {...props}
    >
      {children}
      <TooltipPrimitive.Arrow className="fill-content" width={8} height={4} />
    </TooltipPrimitive.Content>
  </TooltipPrimitive.Portal>
));
TooltipContent.displayName = TooltipPrimitive.Content.displayName;

export { Tooltip, TooltipTrigger, TooltipContent, TooltipProvider };
