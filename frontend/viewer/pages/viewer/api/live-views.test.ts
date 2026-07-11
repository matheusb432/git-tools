import { expect, test } from "bun:test";
import { liveViewsKey, sourceProbeKey } from "./live-views.svelte";

test("live resources use complete transport-neutral keys", () => {
  expect(liveViewsKey).toEqual(["viewer", "live-views"]);
  expect(sourceProbeKey({ kind: "LocalRepo", value: "/repo" })).toEqual([
    "viewer",
    "source-probe",
    "LocalRepo",
    "/repo",
  ]);
});
