import { describe, expect, test } from "vitest";
import { createUpdateCoordinator, parseDiffFragmentState } from "./bridge-model";

describe("parseDiffFragmentState", () => {
  test("accepts the two backend states and rejects incomplete payloads", () => {
    expect(parseDiffFragmentState({ state: "pending" })).toEqual({ state: "pending" });
    expect(
      parseDiffFragmentState({
        state: "ready",
        fragment: { html: '<section id="viewer-view"></section>', css: ":host{}" },
      }),
    ).toEqual({
      state: "ready",
      fragment: { html: '<section id="viewer-view"></section>', css: ":host{}" },
    });
    expect(parseDiffFragmentState({ state: "ready", fragment: { html: "missing css" } })).toBeNull();
    expect(parseDiffFragmentState("ready")).toBeNull();
  });
});

describe("createUpdateCoordinator", () => {
  test("serializes work, prioritizes pending recipes, and bounds duplicates", async () => {
    const started: string[] = [];
    const releases: Array<() => void> = [];
    const requestUpdate = createUpdateCoordinator((update) => {
      started.push(update);
      return new Promise<void>((resolve) => releases.push(resolve));
    });

    const settled = requestUpdate("ready-fragment");
    void requestUpdate("ready-fragment");
    void requestUpdate("pending-recipes");
    void requestUpdate("pending-recipes");
    expect(started).toEqual(["ready-fragment"]);

    releases.shift()?.();
    await Promise.resolve();
    expect(started).toEqual(["ready-fragment", "pending-recipes"]);

    releases.shift()?.();
    await Promise.resolve();
    expect(started).toEqual(["ready-fragment", "pending-recipes", "ready-fragment"]);

    releases.shift()?.();
    await settled;
  });
});
