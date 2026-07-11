<script lang="ts">
  import { X, ChevronDown, MoreHorizontal, Trash2, Search } from "@lucide/svelte";
  import * as DropdownMenu from "@/shared/ui/dropdown-menu";
  import * as ContextMenu from "@/shared/ui/context-menu";
  import * as InputGroup from "@/shared/ui/input-group";
  import { Badge } from "@/shared/ui/badge";
  import { Button } from "@/shared/ui/button";
  import { cn } from "@/shared/lib/utils";
  import type { TabMeta } from "@/shared/api";
  import type { TabShell } from "../../model/viewer-session";
  import { TEST_IDS } from "@/shared/testids";
  import { splitTabs, viewerTabKey } from "../../model/tab-strip-fit";

  type TabDisplay = TabShell & {
    readonly meta: TabMeta | null;
    readonly isRefreshing: boolean;
  };

  type Props = {
    tabs: TabDisplay[];
    activeLocalId: string | null;
    onActivate: (localId: string) => void | Promise<void>;
    onClose: (localId: string) => void | Promise<void>;
    onCloseOthers: (localId: string) => void | Promise<void>;
    onCloseAll: () => void | Promise<void>;
  };
  let { tabs, activeLocalId, onActivate, onClose, onCloseOthers, onCloseAll }: Props = $props();

  const TAB_MIN = 120;
  const RESERVED = 220;
  let stripWidth = $state(1200);
  let query = $state("");

  const activeIndex = $derived(tabs.findIndex((tab) => tab.localId === activeLocalId));
  const fit = $derived(splitTabs(tabs, activeIndex, stripWidth, TAB_MIN, RESERVED));
  const overflowFiltered = $derived(
    query.trim() === ""
      ? fit.overflow
      : fit.overflow.filter((t) => tabLabel(t).toLowerCase().includes(query.toLowerCase())),
  );
  function tabLabel(tab: TabDisplay): string {
    return tab.meta?.title || tab.meta?.repo_name || tab.live?.displayName || "Opening diff";
  }

  function tabBadge(tab: TabDisplay): { text: string; variant: "secondary" | "destructive" } | null {
    if (tab.failure !== null) return { text: "Error", variant: "destructive" };
    if (tab.tabId === null) return { text: "Opening", variant: "secondary" };
    return tab.isRefreshing ? { text: "Refreshing", variant: "secondary" } : null;
  }

  function nativeTitle(tab: TabDisplay): string | undefined {
    return tab.failure ?? undefined;
  }
</script>

<div
  data-testid={TEST_IDS.tabs.strip}
  class="flex items-end gap-1 border-b border-border bg-surface px-2.5 pt-2"
  bind:clientWidth={stripWidth}
>
  {#each fit.visible as tab (viewerTabKey(tab))}
    {@const isActive = tab.localId === activeLocalId}
    {@const label = tabLabel(tab)}
    {@const badge = tabBadge(tab)}
    <ContextMenu.Root>
      <ContextMenu.Trigger>
        <div
          data-testid={TEST_IDS.tabs.tab}
          role="tab"
          tabindex="0"
          aria-selected={isActive}
          title={nativeTitle(tab) ?? label}
          class={cn(
            "group flex max-w-[190px] min-w-0 cursor-pointer items-center gap-2 rounded-t-lg bg-muted py-2 pr-1 pl-3.5 text-[13px] text-foreground-muted transition-colors hover:text-foreground",
            isActive && "bg-background text-foreground shadow-[inset_0_2px_0_var(--color-accent)]",
          )}
          onclick={() => onActivate(tab.localId)}
          onkeydown={(e) => {
            if (e.key === "Enter" || e.key === " ") onActivate(tab.localId);
          }}
          onmousedown={(e) => {
            if (e.button === 1) {
              e.preventDefault();
              onClose(tab.localId);
            }
          }}
        >
          {#if tab.isNew === true}
            <span
              data-testid={TEST_IDS.tabs.freshDot}
              class="size-1.5 shrink-0 rounded-full bg-accent"
              aria-hidden="true"
            ></span>
          {/if}
          <span class="truncate">{label}</span>
          {#if badge !== null}
            <Badge
              data-testid={TEST_IDS.tabs.nativeBadge}
              variant={badge.variant}
              class="shrink-0"
              title={tab.failure ?? badge.text}
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
              onClose(tab.localId);
            }}
          >
            <X />
          </Button>
        </div>
      </ContextMenu.Trigger>
      <ContextMenu.Content>
        <ContextMenu.Item onclick={() => onClose(tab.localId)}>Close</ContextMenu.Item>
        <ContextMenu.Item onclick={() => onCloseOthers(tab.localId)}>Close others</ContextMenu.Item>
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
          <DropdownMenu.Item onclick={() => onActivate(tab.localId)}>
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
  </div>
</div>
