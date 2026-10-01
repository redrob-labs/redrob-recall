import { describe, expect, it } from "vitest";
import { inMissingFolder } from "./folders";

const folder = (path: string, present: boolean) => ({
  path,
  present,
  files: 0,
  failed: 0,
});

describe("inMissingFolder", () => {
  it("is true for a file in a folder that is gone", () => {
    expect(
      inMissingFolder("/drive/photos/a.jpg", [folder("/drive/photos", false)]),
    ).toBe(true);
  });

  it("is false for a file in a folder that is there", () => {
    expect(
      inMissingFolder("/home/me/notes/a.md", [folder("/home/me/notes", true)]),
    ).toBe(false);
  });

  it("does not treat a look-alike sibling as inside", () => {
    expect(
      inMissingFolder("/drive/photos-old/a.jpg", [
        folder("/drive/photos", false),
      ]),
    ).toBe(false);
  });

  it("follows the deepest folder that holds the file", () => {
    const folders = [folder("/drive", false), folder("/drive/kept", true)];
    expect(inMissingFolder("/drive/kept/a.txt", folders)).toBe(false);
    expect(inMissingFolder("/drive/other/a.txt", folders)).toBe(true);
  });

  it("handles Windows separators", () => {
    expect(
      inMissingFolder("D:\\photos\\a.jpg", [folder("D:\\photos", false)]),
    ).toBe(true);
  });
});
