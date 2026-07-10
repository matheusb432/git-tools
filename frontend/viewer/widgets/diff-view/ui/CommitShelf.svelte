<script lang="ts">
  import { GitMerge, MessageSquareText, X } from "@lucide/svelte";
  import type { Commit } from "@/shared/api";
  import { cn } from "@/shared/lib/utils";
  import { Button } from "@/shared/ui/button";
  import * as Popover from "@/shared/ui/popover";
  import { TEST_IDS } from "@/shared/testids";

  type Props = {
    readonly commits: readonly Commit[];
    readonly focusedCommits: ReadonlySet<string>;
    readonly onToggleCommitFocus: (sha: string) => void;
    readonly onClearFocus: () => void;
  };

  let { commits, focusedCommits, onToggleCommitFocus, onClearFocus }: Props = $props();

  async function copyText(text: string): Promise<void> {
    if (navigator.clipboard?.writeText !== undefined) {
      await navigator.clipboard.writeText(text);
    }
  }

  function shortSha(sha: string): string {
    return sha.slice(0, 7);
  }
</script>

<aside
  data-testid={TEST_IDS.diffView.commitShelf}
  class="shelf min-h-0 overflow-auto border-l border-border bg-surface p-3 font-mono max-[1024px]:hidden"
>
  <div class="mb-3 flex items-center justify-between gap-2">
    <div class="min-w-0">
      <h2 class="text-[11px] tracking-[0.06em] text-foreground-dim uppercase">Commits</h2>
      <p class="mt-1 text-[11px] text-foreground-dim">
        {commits.length} total{focusedCommits.size > 0 ? `, ${focusedCommits.size} focused` : ""}
      </p>
    </div>
    {#if focusedCommits.size > 0}
      <Button
        variant="ghost"
        size="icon-xs"
        onclick={onClearFocus}
        aria-label="Clear commit focus"
        title="Clear commit focus"
      >
        <X />
      </Button>
    {/if}
  </div>

  {#if commits.length > 0}
    <div class="space-y-1">
      {#each commits as commit (commit.sha)}
        <article
          class={cn(
            "relative ml-2 border-l-2 border-border-strong py-2 pr-2 pl-5 text-left transition-colors hover:bg-muted",
            focusedCommits.has(commit.sha) && "bg-accent/10 shadow-[inset_2px_0_0_var(--color-accent)]",
          )}
        >
          <span class="absolute top-3 -left-[9px] size-3 rounded-full border-2 border-border-strong bg-background"
          ></span>
          <div class="flex min-w-0 items-center gap-1.5">
            <Button
              variant="ghost"
              size="xs"
              class="h-5 rounded border border-accent/40 bg-accent/15 px-1.5 font-mono text-[11px] text-accent hover:bg-accent hover:text-accent-foreground"
              aria-label={`Copy commit hash ${shortSha(commit.sha)}`}
              title="Copy commit hash"
              onclick={async () => {
                await copyText(commit.sha);
              }}
            >
              {shortSha(commit.sha)}
            </Button>
            {#if commit.is_merge}
              <span
                class="flex items-center gap-1 rounded border border-border-strong px-1.5 py-0.5 text-[10.5px] text-foreground-dim"
              >
                <GitMerge class="size-3" />
                Merge
              </span>
            {/if}
            <time class="ml-auto shrink-0 text-[11px] text-foreground-dim" title={commit.iso}>{commit.date}</time>
          </div>

          <div class="mt-1.5 flex min-w-0 items-start gap-1">
            <p class="min-w-0 flex-1 text-[12.5px] leading-snug break-words text-foreground-muted">{commit.subject}</p>
            <Button
              variant={focusedCommits.has(commit.sha) ? "secondary" : "ghost"}
              size="xs"
              class="h-5 shrink-0 rounded px-1.5 text-[11px]"
              aria-pressed={focusedCommits.has(commit.sha)}
              onclick={() => onToggleCommitFocus(commit.sha)}
            >
              {focusedCommits.has(commit.sha) ? "Focused" : "Focus"}
            </Button>

            {#if commit.body.trim() !== ""}
              <Popover.Root>
                <Popover.Trigger>
                  {#snippet child({ props })}
                    <Button
                      variant="ghost"
                      size="icon-xs"
                      class="size-5 shrink-0 rounded"
                      aria-label={`Show commit message for ${shortSha(commit.sha)}`}
                      title="Show commit body"
                      {...props}
                    >
                      <MessageSquareText />
                    </Button>
                  {/snippet}
                </Popover.Trigger>
                <Popover.Content class="w-80 max-w-[calc(100vw-2rem)]" align="end">
                  <div class="space-y-2">
                    <p class="font-mono text-xs text-foreground-muted">{commit.sha}</p>
                    <p class="text-sm whitespace-pre-wrap text-foreground">{commit.body}</p>
                  </div>
                </Popover.Content>
              </Popover.Root>
            {/if}
          </div>

          {#if commit.members.length > 0}
            <div class="mt-2 flex flex-wrap gap-1">
              {#each commit.members as member}
                <Button
                  variant={focusedCommits.has(member) ? "secondary" : "outline"}
                  size="xs"
                  class="h-5 rounded px-1.5 font-mono text-[10.5px]"
                  title={`Toggle merge member ${shortSha(member)}`}
                  onclick={() => onToggleCommitFocus(member)}
                >
                  {shortSha(member)}
                </Button>
              {/each}
            </div>
          {/if}
        </article>
      {/each}
    </div>
  {:else}
    <p class="text-[12.5px] text-foreground-muted">No commits are attached to this diff.</p>
  {/if}
</aside>
