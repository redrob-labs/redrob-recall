import { describe, expect, it } from "vitest";
import {
  checkReleaseAssetNames,
  launcherAssetName,
  parseLauncherAssetName,
} from "./release-asset-names.mjs";

// The asset list of the published v0.2.0 release, verbatim (`gh release view v0.2.0`). These are
// the names tauri-action produces for this app and the launcher could read none of them.
const V020 = [
  "latest.json",
  "Redrob.Recall_0.2.0_aarch64.app.tar.gz",
  "Redrob.Recall_0.2.0_aarch64.app.tar.gz.sig",
  "Redrob.Recall_0.2.0_aarch64.dmg",
  "Redrob.Recall_0.2.0_amd64.AppImage",
  "Redrob.Recall_0.2.0_amd64.AppImage.sig",
  "Redrob.Recall_0.2.0_amd64.deb",
  "Redrob.Recall_0.2.0_amd64.deb.sig",
  "Redrob.Recall_0.2.0_x64-setup.exe",
  "Redrob.Recall_0.2.0_x64-setup.exe.sig",
  "Redrob.Recall_0.2.0_x64_en-US.msi",
  "Redrob.Recall_0.2.0_x64_en-US.msi.sig",
];

const LINUX = "x86_64-unknown-linux-gnu";
const MAC = "aarch64-apple-darwin";
const WINDOWS = "x86_64-pc-windows-msvc";

function renameAll(names, version) {
  return names.map((name) => {
    // The legs the workflow builds, in matrix order (scripts/release-matrix.mjs).
    for (const target of [LINUX, MAC, WINDOWS]) {
      const renamed = launcherAssetName(name, version, target);
      if (renamed) return renamed;
    }
    return name;
  });
}

describe("launcherAssetName", () => {
  it("turns every Tauri name of the v0.2.0 release into the launcher shape", () => {
    expect(
      launcherAssetName("Redrob.Recall_0.2.0_amd64.AppImage", "0.2.0", LINUX),
    ).toBe("redrob-recall-0.2.0-linux-x86_64.AppImage");
    expect(
      launcherAssetName("Redrob.Recall_0.2.0_amd64.deb.sig", "0.2.0", LINUX),
    ).toBe("redrob-recall-0.2.0-linux-x86_64.deb.sig");
    expect(
      launcherAssetName("Redrob.Recall_0.2.0_aarch64.dmg", "0.2.0", MAC),
    ).toBe("redrob-recall-0.2.0-macos-aarch64.dmg");
    expect(
      launcherAssetName(
        "Redrob.Recall_0.2.0_aarch64.app.tar.gz.sig",
        "0.2.0",
        MAC,
      ),
    ).toBe("redrob-recall-0.2.0-macos-aarch64.app.tar.gz.sig");
    expect(
      launcherAssetName("Redrob.Recall_0.2.0_x64_en-US.msi", "0.2.0", WINDOWS),
    ).toBe("redrob-recall-0.2.0-windows-x64.msi");
    expect(
      launcherAssetName(
        "Redrob.Recall_0.2.0_x64-setup.exe.sig",
        "0.2.0",
        WINDOWS,
      ),
    ).toBe("redrob-recall-0.2.0-windows-x64-setup.exe.sig");
  });

  it("accepts the local file name with a space, as Tauri writes it before upload", () => {
    expect(
      launcherAssetName("Redrob Recall_0.2.1_amd64.AppImage", "0.2.1", LINUX),
    ).toBe("redrob-recall-0.2.1-linux-x86_64.AppImage");
  });

  it("leaves another leg's assets, renamed assets and the manifest alone", () => {
    // A Linux leg must not claim the macOS .dmg even when both bundlers write "aarch64".
    expect(
      launcherAssetName("Redrob.Recall_0.2.0_aarch64.dmg", "0.2.0", LINUX),
    ).toBeNull();
    expect(
      launcherAssetName(
        "Redrob.Recall_0.2.0_aarch64.dmg",
        "0.2.0",
        "aarch64-unknown-linux-gnu",
      ),
    ).toBeNull();
    expect(
      launcherAssetName(
        "Redrob.Recall_0.2.0_aarch64.AppImage",
        "0.2.0",
        "aarch64-unknown-linux-gnu",
      ),
    ).toBe("redrob-recall-0.2.0-linux-aarch64.AppImage");
    expect(
      launcherAssetName("Redrob.Recall_0.2.0_x64_en-US.msi", "0.2.0", MAC),
    ).toBeNull();
    expect(
      launcherAssetName(
        "redrob-recall-0.2.0-linux-x86_64.AppImage",
        "0.2.0",
        LINUX,
      ),
    ).toBeNull();
    expect(launcherAssetName("latest.json", "0.2.0", LINUX)).toBeNull();
    // A stale asset from another version is not this release's to rename.
    expect(
      launcherAssetName("Redrob.Recall_0.1.0_amd64.AppImage", "0.2.0", LINUX),
    ).toBeNull();
  });

  it("refuses an unknown target instead of guessing a token", () => {
    expect(() =>
      launcherAssetName(
        "Redrob.Recall_0.2.0_amd64.AppImage",
        "0.2.0",
        "mips-unknown-linux-gnu",
      ),
    ).toThrow(/unknown release target/);
  });

  it("keeps a pre-release version intact", () => {
    expect(
      launcherAssetName(
        "Redrob.Recall_0.3.0-rc.1_amd64.deb",
        "0.3.0-rc.1",
        LINUX,
      ),
    ).toBe("redrob-recall-0.3.0-rc.1-linux-x86_64.deb");
  });
});

describe("parseLauncherAssetName", () => {
  it("reads what the launcher reads and nothing else", () => {
    expect(
      parseLauncherAssetName("redrob-recall-0.2.1-linux-x86_64.AppImage"),
    ).toEqual({
      version: "0.2.1",
      token: "linux-x86_64",
      variant: "",
      ext: "AppImage",
      installable: true,
    });
    expect(
      parseLauncherAssetName("redrob-recall-0.2.1-windows-x64-portable.zip")
        ?.installable,
    ).toBe(true);
    expect(
      parseLauncherAssetName("redrob-recall-0.2.1-macos-aarch64.app.tar.gz")
        ?.installable,
    ).toBe(false);
    expect(
      parseLauncherAssetName("redrob-recall-0.2.1-windows-x64-setup.exe")
        ?.installable,
    ).toBe(false);
    expect(
      parseLauncherAssetName("redrob-recall-0.2.1-linux-x86_64.deb.sig")
        ?.installable,
    ).toBe(false);
  });

  it("rejects the tokens the launcher does not know", () => {
    for (const name of [
      "redrob-recall-0.2.1-linux-amd64.AppImage",
      "redrob-recall-0.2.1-linux-x64.deb",
      "redrob-recall-0.2.1-mac-aarch64.dmg",
      "redrob-recall-0.2.1-darwin-aarch64.dmg",
      "redrob-recall-0.2.1-win-x64.msi",
      "redrob-recall-v0.2.1-linux-x86_64.AppImage",
      "Redrob.Recall_0.2.1_amd64.AppImage",
    ]) {
      expect(parseLauncherAssetName(name), name).toBeNull();
    }
  });
});

describe("checkReleaseAssetNames", () => {
  it("fails the v0.2.0 release as published", () => {
    const problems = checkReleaseAssetNames(V020, "0.2.0", [
      "linux-x86_64",
      "macos-aarch64",
      "windows-x64",
    ]);
    expect(problems.length).toBe(V020.length - 1 + 3);
  });

  it("passes the same release once every asset is renamed", () => {
    const renamed = renameAll(V020, "0.2.0");
    expect(renamed).toContain("redrob-recall-0.2.0-linux-x86_64.AppImage");
    expect(renamed).toContain("redrob-recall-0.2.0-linux-x86_64.deb");
    expect(renamed).toContain("redrob-recall-0.2.0-macos-aarch64.dmg");
    expect(renamed).toContain("redrob-recall-0.2.0-windows-x64.msi");
    expect(renamed.filter((n) => n.startsWith("Redrob"))).toEqual([]);
    expect(
      checkReleaseAssetNames(renamed, "0.2.0", [
        "linux-x86_64",
        "macos-aarch64",
        "windows-x64",
      ]),
    ).toEqual([]);
  });

  it("reports a built leg that left no installable asset", () => {
    const renamed = renameAll(V020, "0.2.0").filter((n) => !n.endsWith(".dmg"));
    expect(checkReleaseAssetNames(renamed, "0.2.0", ["macos-aarch64"])).toEqual(
      ["no installable asset for macos-aarch64"],
    );
  });

  it("reports an asset from another version", () => {
    expect(
      checkReleaseAssetNames(["redrob-recall-0.2.0-linux-x86_64.deb"], "0.2.1"),
    ).toEqual([
      "redrob-recall-0.2.0-linux-x86_64.deb: version 0.2.0 is not the release version 0.2.1",
    ]);
  });
});
