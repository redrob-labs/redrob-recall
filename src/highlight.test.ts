import { describe, expect, it } from "vitest";
import { matchRanges } from "./highlight";

const marked = (text: string, query: string) =>
  matchRanges(text, query).map(([s, e]) => text.slice(s, e));

describe("matchRanges", () => {
  it("marks every whole-word occurrence of each query word, ignoring case", () => {
    expect(marked("Saffron rice. Add saffron last.", "saffron rice")).toEqual([
      "Saffron",
      "rice",
      "saffron",
    ]);
  });

  it("does not mark part of a word, which the index would not match", () => {
    expect(marked("saffron and saffrons", "saff saffron")).toEqual(["saffron"]);
  });

  it("ignores accents the same way the index does", () => {
    expect(marked("Meet at the Café", "cafe")).toEqual(["Café"]);
  });

  it("marks nothing for a query with no words", () => {
    expect(matchRanges("anything", " ?! ")).toEqual([]);
  });
});
