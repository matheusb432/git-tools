import { describe, expect, test } from "vitest";
import { initTabs } from "./tabbed";

describe("initTabs", () => {
  test("selects one semantic panel at a time", () => {
    const tabs = document.createElement("nav");
    tabs.className = "tabs";
    const tab0 = document.createElement("button");
    tab0.className = "tab active";
    tab0.id = "tab-0";
    tab0.setAttribute("data-tab", "0");
    tab0.setAttribute("aria-selected", "true");
    const tab1 = document.createElement("button");
    tab1.className = "tab";
    tab1.id = "tab-1";
    tab1.setAttribute("data-tab", "1");
    tab1.setAttribute("aria-selected", "false");
    tabs.appendChild(tab0);
    tabs.appendChild(tab1);
    const panel0 = document.createElement("section");
    panel0.className = "panel";
    panel0.id = "panel-0";
    const panel1 = document.createElement("section");
    panel1.className = "panel";
    panel1.id = "panel-1";
    panel1.hidden = true;
    document.body.appendChild(tabs);
    document.body.appendChild(panel0);
    document.body.appendChild(panel1);

    initTabs();
    tab1.click();

    expect(tab0.getAttribute("aria-selected")).toBe("false");
    expect(tab1.getAttribute("aria-selected")).toBe("true");
    expect(panel0.hidden).toBe(true);
    expect(panel1.hidden).toBe(false);
  });
});
