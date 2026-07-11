<script lang="ts">
  import {
    Copy,
    Filter,
    FoldVertical,
    Maximize2,
    Minimize2,
    RefreshCw,
    Rows3,
    SplitSquareVertical,
  } from "@lucide/svelte";
  import type { TabMeta } from "@/shared/api";
  import { Badge } from "@/shared/ui/badge";
  import { Button } from "@/shared/ui/button";
  import * as InputGroup from "@/shared/ui/input-group";
  import * as ToggleGroup from "@/shared/ui/toggle-group";
  import { TEST_IDS } from "@/shared/testids";
  import type { DiffLayout } from "../../model/file-panels";

  type Props = {
    readonly meta: TabMeta;
    readonly layout: DiffLayout;
    readonly full: boolean;
    readonly filterText: string;
    readonly refreshing: boolean;
    readonly canCopy: boolean;
    readonly allFilesFolded: boolean;
    readonly onSetLayout: (layout: DiffLayout) => void;
    readonly onToggleFull: () => void;
    readonly onToggleAllFilesFolded: () => void;
    readonly onFilterTextChange: (value: string) => void;
    readonly onRefresh: () => void | Promise<void>;
    readonly onCopyVisible: () => void | Promise<void>;
  };

  let {
    meta,
    layout,
    full,
    filterText,
    refreshing,
    canCopy,
    allFilesFolded,
    onSetLayout,
    onToggleFull,
    onToggleAllFilesFolded,
    onFilterTextChange,
    onRefresh,
    onCopyVisible,
  }: Props = $props();

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
  data-testid={TEST_IDS.diffView.titlebar}
  class="titlebar flex flex-col gap-3 border-b border-border bg-surface px-5 py-3 font-mono 2xl:flex-row 2xl:items-center"
>
  <div class="min-w-0 flex-1">
    <div class="flex flex-wrap items-center gap-2">
      <h1 class="truncate text-[18px] font-semibold">{meta.title || "Native diff"}</h1>
      <Badge variant="outline">{meta.commits_label}</Badge>
      {#if refreshing}
        <Badge variant="secondary">Refreshing</Badge>
      {/if}
    </div>
    <div class="mt-1 flex min-w-0 flex-wrap items-center gap-x-2 gap-y-0.5 text-[11.5px] text-foreground-muted">
      <span class="max-w-[20rem] truncate text-foreground" title={meta.repo_root}>{meta.repo_name}</span>
      <span aria-hidden="true" class="text-foreground-dim">/</span>
      <span class="max-w-[15rem] truncate" title={meta.branch}>{meta.branch}</span>
      <span aria-hidden="true" class="text-foreground-dim">/</span>
      <span class="max-w-[15rem] truncate" title={meta.upstream}>{meta.upstream}</span>
    </div>
    <p
      class="mt-0.5 truncate text-[12.5px] text-foreground-muted"
      title={`${meta.cmd_lead}${meta.cmd_range}${meta.cmd_trail}`}
    >
      <span>{meta.cmd_lead}</span><span class="text-foreground">{meta.cmd_range}</span><span>{meta.cmd_trail}</span>
    </p>
  </div>

  <div class="flex max-w-full flex-wrap items-center gap-2 2xl:shrink-0">
    <ToggleGroup.Root
      data-testid={TEST_IDS.diffView.layoutToggle}
      type="single"
      variant="outline"
      size="sm"
      value={layout}
      onValueChange={handleLayoutChange}
      aria-label="Choose diff layout"
    >
      <ToggleGroup.Item
        data-testid={TEST_IDS.diffView.layoutUnified}
        value="unified"
        aria-label="Unified layout"
        title="Unified layout"
      >
        <Rows3 data-icon="inline-start" />
        Unified
      </ToggleGroup.Item>
      <ToggleGroup.Item
        data-testid={TEST_IDS.diffView.layoutSplit}
        value="split"
        aria-label="Split layout"
        title="Split layout"
      >
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
      <ToggleGroup.Item
        data-testid={TEST_IDS.diffView.densityCompact}
        value="compact"
        aria-label="Compact diff rows"
        title="Compact diff rows"
      >
        <Minimize2 data-icon="inline-start" />
        Compact
      </ToggleGroup.Item>
      <ToggleGroup.Item
        data-testid={TEST_IDS.diffView.densityFull}
        value="full"
        aria-label="Full diff rows"
        title="Full diff rows"
      >
        <Maximize2 data-icon="inline-start" />
        Full
      </ToggleGroup.Item>
    </ToggleGroup.Root>

    <Button
      variant="outline"
      size="sm"
      onclick={onToggleAllFilesFolded}
      aria-pressed={allFilesFolded}
      title={allFilesFolded ? "Expand file sections" : "Fold file sections"}
    >
      <FoldVertical data-icon="inline-start" />
      {allFilesFolded ? "Expand all" : "Fold all"}
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
