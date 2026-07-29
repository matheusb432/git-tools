import { installMobilePreviewActions } from "./mobile-preview-actions";
import { installPendingRecipes } from "./pending-recipes";
import { installTabWheel } from "./tab-scroll";
import { installThemeControl } from "./theme-control";
import { installToastDismiss } from "./toast";
import { installViewerLoading } from "./view-loading";

installThemeControl(document);
installToastDismiss(document);
installTabWheel(document);
installMobilePreviewActions(document);
void installViewerLoading(window);
void installPendingRecipes(window);
