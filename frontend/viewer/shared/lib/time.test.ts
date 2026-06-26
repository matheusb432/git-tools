import { expect, test } from "bun:test";
import { relativeTime } from "./time";

const now = new Date("2026-06-25T12:00:00Z");

test("just now under a minute", () => {
  expect(relativeTime("2026-06-25T11:59:30Z", now)).toBe("just now");
});
test("minutes ago", () => {
  expect(relativeTime("2026-06-25T11:40:00Z", now)).toBe("20m ago");
});
test("hours ago", () => {
  expect(relativeTime("2026-06-25T10:00:00Z", now)).toBe("2h ago");
});
test("yesterday", () => {
  expect(relativeTime("2026-06-24T09:00:00Z", now)).toBe("yesterday");
});
test("days ago within a week", () => {
  expect(relativeTime("2026-06-22T12:00:00Z", now)).toBe("3d ago");
});
test("same-year date falls back to month + day", () => {
  expect(relativeTime("2026-05-10T12:00:00Z", now)).toBe("May 10");
});
test("prior-year date includes the year", () => {
  expect(relativeTime("2025-12-01T12:00:00Z", now)).toBe("Dec 1, 2025");
});
test("invalid input returns empty string", () => {
  expect(relativeTime("not-a-date", now)).toBe("");
});
