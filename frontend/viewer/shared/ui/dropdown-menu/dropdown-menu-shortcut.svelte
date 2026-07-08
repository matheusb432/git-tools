<script lang="ts">
  import type { HTMLAttributes } from "svelte/elements";
  import { cn, type WithElementRef } from "@/shared/lib/utils.js";

  let {
    ref = $bindable(null),
    class: className,
    children,
    ...restProps
  }: WithElementRef<HTMLAttributes<HTMLSpanElement>> = $props();
</script>

<span
  bind:this={ref}
  data-slot="dropdown-menu-shortcut"
  class={cn(
    "text-muted-foreground ml-auto text-xs tracking-widest group-focus/dropdown-menu-item:text-accent-foreground",
    className,
  )}
  {...restProps}
>
  {@render children?.()}
</span>
