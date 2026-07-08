<script lang="ts">
  import { Copy, Filter, FoldVertical, Maximize2, Minimize2, RefreshCw, Rows3, SplitSquareVertical } from "@lucide/svelte";
  import type { TabMeta } from "@/shared/api";
  import { Badge } from "@/shared/ui/badge";
  import { Button } from "@/shared/ui/button";
  import * as InputGroup from "@/shared/ui/input-group";
  import * as ToggleGroup from "@/shared/ui/toggle-group";
  import { TEST_IDS } from "@/shared/testids";
  import type { DiffLayout } from "../model/diff-view";

  type Props = {
    readonly meta: TabMeta;
    readonly layout: DiffLayout;
    readonly full: boolean;
    readonly filterText: string;
    readonly refreshing: boolean;
    readonly canCopy: boolean;
    readonly onSetLayout: (layout: DiffLayout) => void;
    readonly onToggleFull: () => void;
    readonly onFilterTextChange: (value: string) => void;
    readonly onRefresh: () => void | Promise<void>;
    readonly onCopyVisible: () => void | Promise<void>;
  };

  let { meta, layout, full, filterText, refreshing, canCopy, onSetLayout, onToggleFull, onFilterTextChange, onRefresh, onCopyVisible }: Props = $props();

  function handleLayoutChange(value: string): void {
    if (value === "unified" || value === "split") onSetLayout(value);
  }

  function handleDensityChange(value: string): void {
    if ((value === "compact" && full) || (value === "full" && !full)) onToggleFull();
  }

  function handleFilterInput(event: Event & { currentTarget: EventTarget & HTMLInputElement }): void {
    onFilterTextChange(event.currentTarget.value);
  }
</script>

<div
  data-testid={TEST_IDS.diffView.toolbar}
  class="flex flex-wrap items-center gap-3 border-b border-border px-5 py-3"
>
  <div class="min-w-0 flex-1">
    <div class="flex flex-wrap items-center gap-2">
      <h1 class="truncate text-sm font-semibold">{meta.title || meta.repo_name || "Native diff"}</h1>
      <Badge variant="outline">{meta.commits_label}</Badge>
      {#if refreshing}
        <Badge variant="secondary">Refreshing</Badge>
      {/if}
    </div>
    <p class="mt-1 truncate font-mono text-xs text-foreground-muted" title={`${meta.cmd_lead}${meta.cmd_range}${meta.cmd_trail}`}>
      <span>{meta.cmd_lead}</span><span class="text-foreground">{meta.cmd_range}</span><span>{meta.cmd_trail}</span>
    </p>
  </div>

  <div class="flex flex-wrap items-center gap-2">
    <ToggleGroup.Root
      data-testid={TEST_IDS.diffView.layoutToggle}
      type="single"
      variant="outline"
      size="sm"
      value={layout}
      onValueChange={handleLayoutChange}
      aria-label="Choose diff layout"
    >
      <ToggleGroup.Item value="unified" aria-label="Unified layout" title="Unified layout">
        <Rows3 data-icon="inline-start" />
        Unified
      </ToggleGroup.Item>
      <ToggleGroup.Item value="split" aria-label="Split layout" title="Split layout">
        <SplitSquareVertical data-icon="inline-start" />
        Split
      </ToggleGroup.Item>
    </ToggleGroup.Root>

    <ToggleGroup.Root
      data-testid={TEST_IDS.diffView.fullToggle}
      type="single"
      variant="outline"
      size="sm"
      value={full ? "full" : "compact"}
      onValueChange={handleDensityChange}
      aria-label="Choose row density"
    >
      <ToggleGroup.Item value="compact" aria-label="Compact diff rows" title="Compact diff rows">
        <Minimize2 data-icon="inline-start" />
        Compact
      </ToggleGroup.Item>
      <ToggleGroup.Item value="full" aria-label="Full diff rows" title="Full diff rows">
        <Maximize2 data-icon="inline-start" />
        Full
      </ToggleGroup.Item>
    </ToggleGroup.Root>

    <Button variant="outline" size="sm" disabled title="Folded row groups land in Task 4">
      <FoldVertical data-icon="inline-start" />
      Fold all
    </Button>

    <InputGroup.Root class="h-8 w-[260px]" data-testid={TEST_IDS.diffView.filter}>
      <InputGroup.Input
        placeholder="Filter files"
        value={filterText}
        oninput={handleFilterInput}
        aria-label="Filter files"
      />
      <InputGroup.Addon>
        <Filter class="text-foreground-muted" />
      </InputGroup.Addon>
    </InputGroup.Root>

    <Button variant="outline" size="sm" onclick={onCopyVisible} disabled={!canCopy}>
      <Copy data-icon="inline-start" />
      Copy
    </Button>

    <Button
      data-testid={TEST_IDS.diffView.refresh}
      variant={refreshing ? "secondary" : "outline"}
      size="sm"
      onclick={onRefresh}
      disabled={refreshing}
    >
      <RefreshCw data-icon="inline-start" class={refreshing ? "animate-spin" : undefined} />
      Refresh
    </Button>
  </div>
</div>
