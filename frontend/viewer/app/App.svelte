<script lang="ts">
  import DataProvider from "./providers/DataProvider.svelte";
  import { ViewerPage } from "@/pages/viewer";
</script>

<DataProvider>
  <ViewerPage />
</DataProvider>
