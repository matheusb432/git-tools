<script lang="ts">
  import { createDiffReviewStore } from "../../model/diff-review";
  import FileDiffPanel from "./FileDiffPanel.svelte";

  type Props = {
    readonly full: boolean;
    readonly added?: number;
    readonly removed?: number;
    readonly expanded?: boolean;
    readonly panelTop?: number;
    readonly panelHeight?: number;
    readonly outerScrollTop?: number;
    readonly outerViewportHeight?: number;
  };

  let {
    full,
    added = 1,
    removed = 0,
    expanded = true,
    panelTop = 0,
    panelHeight = 96,
    outerScrollTop = 0,
    outerViewportHeight = 400,
  }: Props = $props();
  const reviewStore = createDiffReviewStore();
</script>

<FileDiffPanel
  tabId={7}
  {reviewStore}
  fileIdx={0}
  path="src/main.rs"
  status="M"
  {added}
  {removed}
  hasFull={true}
  commits={[]}
  layout="unified"
  {full}
  selected={true}
  {expanded}
  onToggleExpanded={() => undefined}
  {panelTop}
  {panelHeight}
  {outerScrollTop}
  {outerViewportHeight}
  onScrollToTop={() => undefined}
/>
