import "./components"; // registers custom elements (side-effect import)
import { initView } from "./preview";
import { initTabs } from "./tabbed";

[].forEach.call(document.querySelectorAll(".layout"), (root: Element) => initView(root as HTMLElement));
initTabs();
