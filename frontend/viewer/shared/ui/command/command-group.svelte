<script lang="ts">
  import { Command as CommandPrimitive, useId } from "bits-ui";
  import { cn } from "@/shared/lib/utils.js";

  let {
    ref = $bindable(null),
    class: className,
    children,
    heading,
    value,
    ...restProps
  }: CommandPrimitive.GroupProps & {
    heading?: string;
  } = $props();
</script>

<CommandPrimitive.Group
  bind:ref
  data-slot="command-group"
  class={cn(
    "**:[[cmdk-group-heading]]:text-muted-foreground overflow-hidden p-1 text-foreground **:[[cmdk-group-heading]]:px-2 **:[[cmdk-group-heading]]:py-1.5 **:[[cmdk-group-heading]]:text-xs **:[[cmdk-group-heading]]:font-medium",
    className,
  )}
  value={value ?? heading ?? `----${useId()}`}
  {...restProps}
>
  {#if heading}
    <CommandPrimitive.GroupHeading class="text-muted-foreground px-2 py-1.5 text-xs font-medium">
      {heading}
    </CommandPrimitive.GroupHeading>
  {/if}
  {#if children !== undefined}
    <CommandPrimitive.GroupItems {children} />
  {/if}
</CommandPrimitive.Group>
