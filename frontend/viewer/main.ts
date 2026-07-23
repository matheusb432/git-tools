import { installPendingRecipes } from "./pending-recipes";
import { installThemeControl } from "./theme-control";
import { installToastDismiss } from "./toast";

installThemeControl(document);
installToastDismiss(document);
void installPendingRecipes(window);
