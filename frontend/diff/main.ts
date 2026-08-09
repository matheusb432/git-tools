import { enhanceLayout } from "./enhance-layout";
import { createEnhancementLifecycle } from "./enhancement-lifecycle";
import { initTabs } from "./tabbed";

const layoutLifecycle = createEnhancementLifecycle(".layout", enhanceLayout);

layoutLifecycle.enhanceWithin(document);
initTabs();
