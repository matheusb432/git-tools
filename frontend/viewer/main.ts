import { installTabWheel } from "./tab-scroll";
import { installThemeControl } from "./theme-control";
import { installToastDismiss } from "./toast";
import { installViewerLoading } from "./view-loading";
import { installViewerUpdates } from "./viewer-updates";

installThemeControl(document);
installToastDismiss(document);
installTabWheel(document);
installViewerLoading(document);
void installViewerUpdates(window);
