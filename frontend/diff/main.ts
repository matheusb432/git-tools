import { installHistoryActions } from "./history";
import { enhanceWithin, installSwapLifecycle } from "./swap";
import { initTabs } from "./tabbed";

enhanceWithin(document);
installSwapLifecycle(document);
installHistoryActions(document);
initTabs();
