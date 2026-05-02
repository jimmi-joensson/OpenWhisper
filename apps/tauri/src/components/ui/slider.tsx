import { Slider as SliderPrimitive } from "@base-ui/react/slider"

import { cn } from "@/lib/utils"

// Convenience wrapper that renders the full slider anatomy
// (Root > Control > Track > Indicator + Thumb) — mirrors shadcn's
// Radix-based <Slider> shape on top of base-ui primitives so the
// rest of the app gets the same `value / onValueChange / min / max /
// step / disabled` props it would expect from a stock shadcn Slider.
//
// Single-thumb only for now. If we ever need a range slider, switch
// to passing `value` as `number[]` and rendering a Thumb per index;
// base-ui's Root accepts both shapes.
type SliderProps = Omit<
  SliderPrimitive.Root.Props,
  "value" | "defaultValue" | "onValueChange"
> & {
  value?: number
  defaultValue?: number
  onValueChange?: (value: number) => void
}

function Slider({
  className,
  value,
  defaultValue,
  onValueChange,
  // base-ui sets `role=slider` on the Thumb, not the Root, so an
  // aria-label spread onto Root never reaches the screen-reader-
  // visible element. Forward it explicitly to the Thumb so
  // `getByRole("slider", { name })` and assistive tech see the same
  // accessible name.
  "aria-label": ariaLabel,
  ...props
}: SliderProps) {
  return (
    <SliderPrimitive.Root
      data-slot="slider"
      thumbAlignment="edge"
      value={value !== undefined ? [value] : undefined}
      defaultValue={defaultValue !== undefined ? [defaultValue] : undefined}
      onValueChange={
        onValueChange ? (next) => onValueChange(Array.isArray(next) ? next[0] : next) : undefined
      }
      // className passes through to the Root container — that's what
      // the parent flex layout needs to size (e.g. `flex-1` to grow
      // the slider between flanking min/max labels). The default
      // `w-full` keeps the slider self-stretching when no override
      // is supplied. Without this, Root renders with intrinsic-zero
      // width and the slider visibly disappears, even though Track
      // and Thumb inside have `w-full`.
      className={cn("w-full", className)}
      {...props}
    >
      <SliderPrimitive.Control className="relative flex w-full touch-none items-center py-2 select-none data-[disabled]:opacity-50">
        <SliderPrimitive.Track className="relative h-1.5 w-full grow overflow-hidden rounded-full bg-muted">
          <SliderPrimitive.Indicator className="absolute h-full bg-primary" />
        </SliderPrimitive.Track>
        <SliderPrimitive.Thumb
          index={0}
          aria-label={ariaLabel}
          className={cn(
            "block size-4 shrink-0 rounded-full border border-primary/50 bg-background shadow-sm transition-[color,box-shadow]",
            "hover:bg-accent",
            "focus-visible:outline-2 focus-visible:outline-ring",
            "data-[dragging]:cursor-grabbing",
            "data-[disabled]:pointer-events-none",
          )}
        />
      </SliderPrimitive.Control>
    </SliderPrimitive.Root>
  )
}

export { Slider }
