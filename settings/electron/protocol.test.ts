import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";
import { parseSettings, parseStatus } from "./contract.js";

describe("desktop protocol", () => {
  it("rejects malformed settings JSON", () => expect(() => parseSettings("{")) .toThrow());
  it("rejects future settings versions", () => expect(() => parseSettings(JSON.stringify({ schemaVersion: 2 }))).toThrow());
  it("accepts valid status", () => expect(parseStatus(JSON.stringify({ schemaVersion: 1, state: "idle", extensionActive: false, ubuntu: "24.04", gnomeShell: "46" })).state).toBe("idle"));
  it("rejects unknown status state", () => expect(() => parseStatus(JSON.stringify({ schemaVersion: 1, state: "unknown", extensionActive: false, ubuntu: "", gnomeShell: "" }))).toThrow());
  it("shares malformed runtime fixture", () => expect(readFileSync(path.resolve(process.cwd(), "../tests/fixtures/protocol/runtime-malformed.json"), "utf8")).toContain("state"));
});
