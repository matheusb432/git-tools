<script lang="ts">
  import { Copy, GitMerge, MessageSquareText, X } from "@lucide/svelte";
  import type { Commit } from "@/shared/api";
  import { cn } from "@/shared/lib/utils";
  import { Badge } from "@/shared/ui/badge";
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

<section data-testid={TEST_IDS.diffView.commitShelf} class="border-b border-border px-5 py-3">
  <div class="mb-3 flex items-center justify-between gap-2">
    <div class="flex items-center gap-2">
      <h2 class="text-sm font-semibold">Commits</h2>
      <Badge variant="outline">{commits.length}</Badge>
      {#if focusedCommits.size > 0}
        <Badge variant="secondary">{focusedCommits.size} focused</Badge>
      {/if}
    </div>
    {#if focusedCommits.size > 0}
      <Button variant="ghost" size="sm" onclick={onClearFocus}>
        <X data-icon="inline-start" />
        Clear focus
      </Button>
    {/if}
  </div>

  {#if commits.length > 0}
    <div class="flex gap-3 overflow-auto pb-1">
      {#each commits as commit (commit.sha)}
        <article
          class={cn(
            "max-w-[320px] min-w-[280px] rounded-lg border px-3 py-3 text-left transition-colors",
            focusedCommits.has(commit.sha) ? "border-accent/40 bg-muted" : "border-border bg-surface/60",
          )}
        >
          <div class="flex items-start gap-2">
            <div class="min-w-0 flex-1">
              <div class="flex items-center gap-2">
                <span class="font-mono text-xs text-accent">{shortSha(commit.sha)}</span>
                {#if commit.is_merge}
                  <Badge variant="outline">
                    <GitMerge data-icon="inline-start" />
                    Merge
                  </Badge>
                {/if}
              </div>
              <p class="mt-2 line-clamp-2 text-sm font-medium">{commit.subject}</p>
              <p class="mt-1 text-xs text-foreground-muted" title={commit.iso}>{commit.date}</p>
            </div>

            <div class="flex items-center gap-1">
              <Button
                variant={focusedCommits.has(commit.sha) ? "secondary" : "outline"}
                size="xs"
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
                        aria-label={`Show commit message for ${shortSha(commit.sha)}`}
                        title="Show commit body"
                        {...props}
                      >
                        <MessageSquareText />
                      </Button>
                    {/snippet}
                  </Popover.Trigger>
                  <Popover.Content class="w-80" align="end">
                    <div class="space-y-2">
                      <p class="font-mono text-xs text-foreground-muted">{commit.sha}</p>
                      <p class="text-sm whitespace-pre-wrap text-foreground">{commit.body}</p>
                    </div>
                  </Popover.Content>
                </Popover.Root>
              {/if}

              <Button
                variant="ghost"
                size="icon-xs"
                aria-label={`Copy commit hash ${shortSha(commit.sha)}`}
                title="Copy commit hash"
                onclick={async () => {
                  await copyText(commit.sha);
                }}
              >
                <Copy />
              </Button>
            </div>
          </div>

          {#if commit.members.length > 0}
            <div class="mt-3 flex flex-wrap gap-1.5">
              {#each commit.members as member}
                <Button
                  variant={focusedCommits.has(member) ? "secondary" : "outline"}
                  size="xs"
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
    <p class="text-sm text-foreground-muted">No commits are attached to this diff.</p>
  {/if}
</section>
