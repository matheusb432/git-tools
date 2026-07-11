import { handleDocumentCopy } from "./enhance-layout";
import { enhanceWithin, installSwapLifecycle } from "./swap";
import { initTabs } from "./tabbed";

enhanceWithin(document);
installSwapLifecycle(document);
document.addEventListener("copy", handleDocumentCopy);
initTabs();
