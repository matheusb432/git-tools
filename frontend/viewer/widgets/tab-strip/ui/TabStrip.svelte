<script lang="ts">
  import { X, ChevronDown, MoreHorizontal, History, Trash2, Search } from "@lucide/svelte";
  import * as DropdownMenu from "@/shared/ui/dropdown-menu";
  import * as ContextMenu from "@/shared/ui/context-menu";
  import * as InputGroup from "@/shared/ui/input-group";
  import { Badge } from "@/shared/ui/badge";
  import { Button } from "@/shared/ui/button";
  import { cn } from "@/shared/lib/utils";
  import type { NativeTab, ViewerTab } from "@/entities/diff-tab";
  import { TEST_IDS } from "@/shared/testids";
  import { splitTabs, viewerTabKey } from "../model/fit";

  type Props = {
    tabs: ViewerTab[];
    active: number;
    showHistory: boolean;
    onActivate: (i: number) => void | Promise<void>;
    onClose: (i: number) => void | Promise<void>;
    onCloseOthers: (i: number) => void | Promise<void>;
    onCloseAll: () => void | Promise<void>;
    onOpenHistory: () => void | Promise<void>;
  };
  let { tabs, active, showHistory, onActivate, onClose, onCloseOthers, onCloseAll, onOpenHistory }: Props = $props();

  const TAB_MIN = 120;
  const RESERVED = 220;
  let stripWidth = $state(1200);
  let query = $state("");

  const fit = $derived(splitTabs(tabs, active, stripWidth, TAB_MIN, RESERVED));
  const overflowFiltered = $derived(
    query.trim() === ""
      ? fit.overflow
      : fit.overflow.filter((t) => tabLabel(t).toLowerCase().includes(query.toLowerCase())),
  );
  const indexOf = (t: ViewerTab) => tabs.indexOf(t);

  function tabLabel(tab: ViewerTab): string {
    return tab.kind === "artifact" ? tab.label : tab.meta?.title || tab.meta?.repo_name || "Opening diff";
  }

  function tabBadge(tab: ViewerTab): { text: string; variant: "secondary" | "outline" | "destructive" } | null {
    if (tab.kind === "artifact") return null;
    switch (tab.lifecycle.state) {
      case "opening":
        return { text: "Opening", variant: "secondary" };
      case "refreshing":
        return { text: "Refreshing", variant: "secondary" };
      case "error":
        return { text: "Error", variant: "destructive" };
      case "ready":
        return tab.freshness.state === "stale" ? { text: "Stale", variant: "outline" } : null;
    }
  }

  function nativeTitle(tab: NativeTab): string | undefined {
    if (tab.lifecycle.state !== "error") return undefined;
    return tab.lifecycle.message;
  }
</script>

<div
  data-testid={TEST_IDS.tabs.strip}
  class="flex items-end gap-1 border-b border-border bg-surface px-2.5 pt-2"
  bind:clientWidth={stripWidth}
>
  {#each fit.visible as tab (viewerTabKey(tab))}
    {@const i = indexOf(tab)}
    {@const isActive = !showHistory && i === active}
    {@const label = tabLabel(tab)}
    {@const badge = tabBadge(tab)}
    <ContextMenu.Root>
      <ContextMenu.Trigger>
        <div
          data-testid={TEST_IDS.tabs.tab}
          role="tab"
          tabindex="0"
          aria-selected={isActive}
          title={tab.kind === "native" ? (nativeTitle(tab) ?? label) : label}
          class={cn(
            "group flex max-w-[190px] min-w-0 cursor-pointer items-center gap-2 rounded-t-lg bg-muted py-2 pr-1 pl-3.5 text-[13px] text-foreground-muted transition-colors hover:text-foreground",
            isActive && "bg-background text-foreground shadow-[inset_0_2px_0_var(--color-accent)]",
          )}
          onclick={() => onActivate(i)}
          onkeydown={(e) => {
            if (e.key === "Enter" || e.key === " ") onActivate(i);
          }}
          onmousedown={(e) => {
            if (e.button === 1) {
              e.preventDefault();
              onClose(i);
            }
          }}
        >
          <span class="truncate">{label}</span>
          {#if badge !== null}
            <Badge
              data-testid={TEST_IDS.tabs.nativeBadge}
              variant={badge.variant}
              class="shrink-0"
              title={tab.kind === "native" && tab.lifecycle.state === "error" ? tab.lifecycle.message : badge.text}
            >
              {badge.text}
            </Badge>
          {/if}
          <Button
            variant="ghost"
            size="icon-xs"
            aria-label={`Close ${label}`}
            title="Close tab"
            class={cn(
              "text-foreground-muted opacity-0 group-hover:opacity-100 hover:text-destructive focus-visible:opacity-100 motion-safe:transition-opacity",
              isActive && "opacity-100",
            )}
            onclick={(e) => {
              e.stopPropagation();
              onClose(i);
            }}
          >
            <X />
          </Button>
        </div>
      </ContextMenu.Trigger>
      <ContextMenu.Content>
        <ContextMenu.Item onclick={() => onClose(i)}>Close</ContextMenu.Item>
        <ContextMenu.Item onclick={() => onCloseOthers(i)}>Close others</ContextMenu.Item>
        <ContextMenu.Item onclick={onCloseAll}>Close all</ContextMenu.Item>
      </ContextMenu.Content>
    </ContextMenu.Root>
  {/each}

  {#if fit.overflow.length > 0}
    <DropdownMenu.Root>
      <DropdownMenu.Trigger>
        {#snippet child({ props })}
          <Button variant="outline" size="sm" class="mb-px text-foreground-muted hover:text-foreground" {...props}>
            <MoreHorizontal data-icon="inline-start" />
            {fit.overflow.length} more
            <ChevronDown data-icon="inline-end" />
          </Button>
        {/snippet}
      </DropdownMenu.Trigger>
      <DropdownMenu.Content class="w-72">
        <div class="px-2 py-1.5">
          <InputGroup.Root class="h-8">
            <InputGroup.Input placeholder="Search open tabs..." bind:value={query} />
            <InputGroup.Addon>
              <Search class="size-3.5 text-foreground-muted" />
            </InputGroup.Addon>
          </InputGroup.Root>
        </div>
        <DropdownMenu.Separator />
        {#each overflowFiltered as tab (viewerTabKey(tab))}
          {@const badge = tabBadge(tab)}
          <DropdownMenu.Item onclick={() => onActivate(indexOf(tab))}>
            <span class="truncate">{tabLabel(tab)}</span>
            {#if badge !== null}
              <Badge variant={badge.variant} class="ml-auto">{badge.text}</Badge>
            {/if}
          </DropdownMenu.Item>
        {/each}
      </DropdownMenu.Content>
    </DropdownMenu.Root>
  {/if}

  <div class="ml-auto flex items-center gap-1.5 pb-2">
    {#if tabs.length > 0}
      <Button variant="ghost" size="sm" class="text-foreground-muted hover:text-destructive" onclick={onCloseAll}>
        <Trash2 data-icon="inline-start" />
        Close all
      </Button>
    {/if}
    <Button
      data-testid={TEST_IDS.tabs.historyButton}
      variant={showHistory ? "secondary" : "ghost"}
      size="sm"
      class={cn("text-foreground-muted hover:text-foreground", showHistory && "text-accent")}
      onclick={onOpenHistory}
    >
      <History data-icon="inline-start" />
      History
    </Button>
  </div>
</div>
