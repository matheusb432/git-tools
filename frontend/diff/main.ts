import { mount } from "svelte";
import App from "./App.svelte";
import { initTabs } from "./tabbed";

document.querySelectorAll<HTMLElement>(".layout").forEach((root) => {
  mount(App, { target: root, props: { root } });
});
initTabs();
