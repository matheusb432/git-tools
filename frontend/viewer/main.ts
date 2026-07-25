import { installPendingRecipes } from "./pending-recipes";
import { installTabWheel } from "./tab-scroll";
import { installThemeControl } from "./theme-control";
import { installToastDismiss } from "./toast";

installThemeControl(document);
installToastDismiss(document);
installTabWheel(document);
void installPendingRecipes(window);
