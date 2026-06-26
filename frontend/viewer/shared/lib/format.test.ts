import { expect, test } from "bun:test";
import { formatBytes } from "./format";

test("zero", () => expect(formatBytes(0)).toBe("0 B"));
test("bytes", () => expect(formatBytes(512)).toBe("512 B"));
test("kilobytes rounded", () => expect(formatBytes(18432)).toBe("18 KB"));
test("kilobytes one decimal under ten", () => expect(formatBytes(1536)).toBe("1.5 KB"));
test("megabytes", () => expect(formatBytes(5 * 1024 * 1024)).toBe("5 MB"));
