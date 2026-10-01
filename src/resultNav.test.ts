import { describe, expect, it } from "vitest";
import { stepIndex } from "./resultNav";

describe("stepIndex", () => {
  it("moves down and up one result at a time", () => {
    expect(stepIndex(5, 1, "ArrowDown")).toBe(2);
    expect(stepIndex(5, 1, "ArrowUp")).toBe(0);
  });

  it("stops at either end instead of wrapping", () => {
    expect(stepIndex(5, 4, "ArrowDown")).toBe(4);
    expect(stepIndex(5, 0, "ArrowUp")).toBe(0);
  });

  it("starts at the first result when nothing is selected", () => {
    expect(stepIndex(5, -1, "ArrowDown")).toBe(0);
  });

  it("jumps to the ends with Home and End", () => {
    expect(stepIndex(5, 2, "Home")).toBe(0);
    expect(stepIndex(5, 2, "End")).toBe(4);
  });

  it("ignores other keys and empty lists", () => {
    expect(stepIndex(5, 2, "a")).toBeNull();
    expect(stepIndex(0, -1, "ArrowDown")).toBeNull();
  });
});
