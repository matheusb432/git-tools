<script lang="ts">
  import { X, ChevronDown, MoreHorizontal, History, Trash2, Search } from "@lucide/svelte";
  import * as DropdownMenu from "@/shared/ui/dropdown-menu";
  import * as ContextMenu from "@/shared/ui/context-menu";
  import { cn } from "@/shared/lib/utils";
  import type { Tab } from "@/entities/tab";
  import { splitTabs } from "../model/fit";

  type Props = {
    tabs: Tab[];
    active: number;
    showHistory: boolean;
    onActivate: (i: number) => void;
    onClose: (i: number) => void;
    onCloseOthers: (i: number) => void;
    onCloseAll: () => void;
    onOpenHistory: () => void;
  };
  let { tabs, active, showHistory, onActivate, onClose, onCloseOthers, onCloseAll, onOpenHistory }: Props = $props();

  const TAB_MIN = 120;
  const RESERVED = 220;
  let stripWidth = $state(1200);
  let query = $state("");

  const fit = $derived(splitTabs(tabs, active, stripWidth, TAB_MIN, RESERVED));
  const overflowFiltered = $derived(
    query.trim() === "" ? fit.overflow : fit.overflow.filter((t) => t.label.toLowerCase().includes(query.toLowerCase())),
  );
  const indexOf = (t: Tab) => tabs.indexOf(t);
</script>

<div
  class="flex items-end gap-1 bg-surface px-2.5 pt-2 border-b border-border"
  bind:clientWidth={stripWidth}
>
  {#each fit.visible as tab (tab.url)}
    {@const i = indexOf(tab)}
    {@const isActive = !showHistory && i === active}
    <ContextMenu.Root>
      <ContextMenu.Trigger>
        <div
          role="tab"
          tabindex="0"
          aria-selected={isActive}
          title={tab.label}
          class={cn(
            "group flex items-center gap-2 max-w-[190px] min-w-0 rounded-t-lg pl-3.5 pr-1 py-2 text-[13px] cursor-pointer text-foreground-muted bg-muted transition-colors hover:text-foreground",
            isActive && "bg-background text-foreground shadow-[inset_0_2px_0_var(--color-accent)]",
          )}
          onclick={() => onActivate(i)}
          onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") onActivate(i); }}
          onmousedown={(e) => { if (e.button === 1) { e.preventDefault(); onClose(i); } }}
        >
          <span class="truncate">{tab.label}</span>
          <button
            type="button"
            aria-label={`Close ${tab.label}`}
            title="Close tab"
            class={cn(
              "grid place-items-center size-6 rounded-md text-foreground-muted opacity-0 group-hover:opacity-100 focus-visible:opacity-100 hover:bg-destructive/20 hover:text-destructive motion-safe:transition-opacity",
              isActive && "opacity-100",
            )}
            onclick={(e) => { e.stopPropagation(); onClose(i); }}
          >
            <X class="size-3.5" />
          </button>
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
      <DropdownMenu.Trigger
        class="flex items-center gap-1.5 rounded-lg px-3 py-2 mb-px text-[12.5px] text-foreground-muted bg-muted hover:text-foreground"
      >
        <MoreHorizontal class="size-3.5" />{fit.overflow.length} more<ChevronDown class="size-3.5" />
      </DropdownMenu.Trigger>
      <DropdownMenu.Content class="w-72">
        <div class="flex items-center gap-2 px-2 py-1.5">
          <Search class="size-3.5 text-foreground-muted" />
          <input
            class="w-full bg-transparent text-[13px] outline-none placeholder:text-foreground-muted"
            placeholder="Search open tabs…"
            bind:value={query}
          />
        </div>
        <DropdownMenu.Separator />
        {#each overflowFiltered as tab (tab.url)}
          <DropdownMenu.Item onclick={() => onActivate(indexOf(tab))}>
            <span class="truncate">{tab.label}</span>
          </DropdownMenu.Item>
        {/each}
      </DropdownMenu.Content>
    </DropdownMenu.Root>
  {/if}

  <div class="ml-auto flex items-center gap-1.5 pb-2">
    {#if tabs.length > 0}
      <button
        type="button"
        class="flex items-center gap-1.5 rounded-md px-3 py-1.5 text-[12.5px] font-medium text-foreground-muted hover:bg-destructive/15 hover:text-destructive"
        onclick={onCloseAll}
      >
        <Trash2 class="size-3.5" />Close all
      </button>
    {/if}
    <button
      type="button"
      class={cn(
        "flex items-center gap-1.5 rounded-md px-3 py-1.5 text-[12.5px] font-medium text-foreground-muted hover:bg-muted hover:text-foreground",
        showHistory && "bg-accent/15 text-accent",
      )}
      onclick={onOpenHistory}
    >
      <History class="size-3.5" />History
    </button>
  </div>
</div>
