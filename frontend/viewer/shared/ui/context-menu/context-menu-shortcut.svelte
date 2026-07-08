<script lang="ts">
  import { cn, type WithElementRef } from "@/shared/lib/utils.js";
  import type { HTMLAttributes } from "svelte/elements";

  let {
    ref = $bindable(null),
    class: className,
    children,
    ...restProps
  }: WithElementRef<HTMLAttributes<HTMLSpanElement>> = $props();
</script>

<span
  bind:this={ref}
  data-slot="context-menu-shortcut"
  class={cn(
    "text-muted-foreground ml-auto text-xs tracking-widest group-focus/context-menu-item:text-accent-foreground",
    className,
  )}
  {...restProps}
>
  {@render children?.()}
</span>
