import { describe, expect, it } from "vitest";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { execFileSync } from "node:child_process";
import {
  NON_INSTALLER_ASSETS,
  checkReleaseAssetNames,
  launcherAssetName,
  parseLauncherAssetName,
} from "./release-asset-names.mjs";
import {
  checkUpdaterManifest,
  mergeUpdaterFragments,
  publishedDownloadUrl,
  updaterEntries,
} from "./updater-manifest.mjs";

// The asset list of the published v0.2.0 release, verbatim (`gh release view v0.2.0`). These are
// the names tauri-action produced for this app and the launcher could read none of them.
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
const REPO = "redrob-labs/redrob-recall";

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

  // bundle/macos/ holds `Redrob Recall.app.tar.gz` with no version or arch: the updater payload
  // as the bundler writes it, which tauri-action used to rename on upload and the staging step
  // names now.
  it("names the bare macOS updater payload after the leg's arch", () => {
    expect(launcherAssetName("Redrob Recall.app.tar.gz", "0.3.0", MAC)).toBe(
      "redrob-recall-0.3.0-macos-aarch64.app.tar.gz",
    );
    expect(
      launcherAssetName("Redrob Recall.app.tar.gz.sig", "0.3.0", MAC),
    ).toBe("redrob-recall-0.3.0-macos-aarch64.app.tar.gz.sig");
    expect(
      launcherAssetName("Redrob Recall.app.tar.gz", "0.3.0", LINUX),
    ).toBeNull();
    expect(
      launcherAssetName("Redrob Recall.app.tar.gz", "0.3.0", WINDOWS),
    ).toBeNull();
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
    // Every Tauri-named file is a problem (latest.json is not), plus one per leg with no
    // installable asset.
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

  // The shared redrob-labs publish job adds these two beside the installers; neither is one.
  it("ignores the files the shared publish job adds", () => {
    expect([...NON_INSTALLER_ASSETS]).toEqual(
      expect.arrayContaining(["redrob-launcher.json", "SHA256SUMS.txt"]),
    );
    const renamed = [...renameAll(V020, "0.2.0"), ...NON_INSTALLER_ASSETS];
    expect(checkReleaseAssetNames(renamed, "0.2.0", ["linux-x86_64"])).toEqual(
      [],
    );
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

describe("updaterEntries", () => {
  const staged = renameAll(V020, "0.2.0").filter((n) => n !== "latest.json");

  it("writes the bare key for the OS's payload and a suffixed key per signed bundle", () => {
    const keys = (target) =>
      updaterEntries(staged, target).map((e) => [e.key, e.asset]);
    expect(keys(LINUX)).toEqual([
      ["linux-x86_64", "redrob-recall-0.2.0-linux-x86_64.AppImage"],
      ["linux-x86_64-appimage", "redrob-recall-0.2.0-linux-x86_64.AppImage"],
      ["linux-x86_64-deb", "redrob-recall-0.2.0-linux-x86_64.deb"],
    ]);
    expect(keys(MAC)).toEqual([
      ["darwin-aarch64", "redrob-recall-0.2.0-macos-aarch64.app.tar.gz"],
      ["darwin-aarch64-app", "redrob-recall-0.2.0-macos-aarch64.app.tar.gz"],
    ]);
    // NSIS is the payload under the bare key (per-user install); the MSI keeps its own key.
    expect(keys(WINDOWS)).toEqual([
      ["windows-x86_64", "redrob-recall-0.2.0-windows-x64-setup.exe"],
      ["windows-x86_64-nsis", "redrob-recall-0.2.0-windows-x64-setup.exe"],
      ["windows-x86_64-msi", "redrob-recall-0.2.0-windows-x64.msi"],
    ]);
    for (const entry of updaterEntries(staged, LINUX)) {
      expect(entry.sig).toBe(`${entry.asset}.sig`);
    }
  });

  it("skips a payload without a signature beside it, and fails when the OS payload has none", () => {
    const noDebSig = staged.filter(
      (n) => n !== "redrob-recall-0.2.0-linux-x86_64.deb.sig",
    );
    expect(updaterEntries(noDebSig, LINUX).map((e) => e.key)).toEqual([
      "linux-x86_64",
      "linux-x86_64-appimage",
    ]);
    const noAppImageSig = staged.filter(
      (n) => n !== "redrob-recall-0.2.0-linux-x86_64.AppImage.sig",
    );
    expect(() => updaterEntries(noAppImageSig, LINUX)).toThrow(
      /no signed updater payload for linux-x86_64/,
    );
  });
});

describe("latest.json", () => {
  const tag = "v0.2.0";
  const url = (name) => publishedDownloadUrl(REPO, tag, name);
  const fragment = (target) => ({
    platforms: Object.fromEntries(
      updaterEntries(renameAll(V020, "0.2.0"), target).map((e) => [
        e.key,
        { signature: `sig-of-${e.asset}`, url: url(e.asset) },
      ]),
    ),
  });
  const release = {
    version: "0.2.0",
    repository: REPO,
    tag,
    requiredPlatforms: ["linux-x86_64", "darwin-aarch64", "windows-x86_64"],
    assetNames: renameAll(V020, "0.2.0").filter((n) => n !== "latest.json"),
  };

  it("merges the legs' fragments into one manifest and refuses a key written twice", () => {
    const manifest = mergeUpdaterFragments(
      [fragment(LINUX), fragment(MAC), fragment(WINDOWS)],
      { version: "0.2.0", notes: "n", pubDate: "2026-10-14T00:00:00.000Z" },
    );
    expect(Object.keys(manifest.platforms).sort()).toEqual([
      "darwin-aarch64",
      "darwin-aarch64-app",
      "linux-x86_64",
      "linux-x86_64-appimage",
      "linux-x86_64-deb",
      "windows-x86_64",
      "windows-x86_64-msi",
      "windows-x86_64-nsis",
    ]);
    expect(manifest.platforms["windows-x86_64"].url).toBe(
      "https://github.com/redrob-labs/redrob-recall/releases/download/v0.2.0/redrob-recall-0.2.0-windows-x64-setup.exe",
    );
    expect(checkUpdaterManifest(manifest, release)).toEqual([]);
    expect(() =>
      mergeUpdaterFragments([fragment(LINUX), fragment(LINUX)], {
        version: "0.2.0",
        notes: "n",
        pubDate: "2026-10-14T00:00:00.000Z",
      }),
    ).toThrow(/linux-x86_64 is written by two build legs/);
  });

  it("names every way a manifest can silently freeze clients", () => {
    const good = mergeUpdaterFragments([fragment(LINUX), fragment(MAC)], {
      version: "0.2.0",
      notes: "n",
      pubDate: "2026-10-14T00:00:00.000Z",
    });
    // A built platform with no entry.
    expect(checkUpdaterManifest(good, release)).toEqual([
      "no entry for built platform windows-x86_64",
    ]);
    // Wrong version, a URL off this repository, a URL to a file not in the set, no signature.
    const bad = structuredClone(good);
    bad.version = "0.1.9";
    bad.platforms["linux-x86_64"].url =
      "https://cdn.example.com/redrob-recall-0.2.0-linux-x86_64.AppImage";
    bad.platforms["linux-x86_64-deb"].url = url(
      "redrob-recall-0.2.0-linux-x86_64.rpm",
    );
    bad.platforms["darwin-aarch64"].signature = "";
    const problems = checkUpdaterManifest(bad, {
      ...release,
      requiredPlatforms: [],
    });
    expect(problems).toEqual([
      expect.stringMatching(
        /^version "0\.1\.9" is not the release version 0\.2\.0/,
      ),
      expect.stringMatching(
        /^linux-x86_64: url is not a release asset of redrob-labs\/redrob-recall at v0\.2\.0/,
      ),
      expect.stringMatching(
        /^linux-x86_64-deb: names redrob-recall-0\.2\.0-linux-x86_64\.rpm, which is not among/,
      ),
      "darwin-aarch64: has no signature",
    ]);
  });

  // Rehearsal of the release legs and the updater-manifest job on real bytes: Tauri's own signer
  // (a throwaway key) signs fake bundles laid out as the bundler writes them, the staging script
  // names them and writes the fragment, and the assembly script verifies every signature against
  // the staged bytes and writes latest.json. A tampered file after signing must fail it.
  it("stage-release-assets and assemble-updater-manifest agree end to end", async () => {
    const dir = await mkdtemp(join(tmpdir(), "recall-release-"));
    try {
      const tauri = join("node_modules", ".bin", "tauri");
      execFileSync(
        tauri,
        ["signer", "generate", "-w", join(dir, "key"), "--ci"],
        { stdio: "pipe" },
      );
      const signEnv = {
        ...process.env,
        TAURI_SIGNING_PRIVATE_KEY: await readFile(join(dir, "key"), "utf8"),
        TAURI_SIGNING_PRIVATE_KEY_PASSWORD: "",
      };
      const publicKey = (await readFile(join(dir, "key.pub"), "utf8")).trim();
      const bundle = join(dir, "bundle");
      const write = async (sub, name, signed) => {
        await mkdir(join(bundle, sub), { recursive: true });
        const path = join(bundle, sub, name);
        await writeFile(path, `bytes of ${name}\n`);
        if (signed)
          execFileSync(tauri, ["signer", "sign", path], {
            env: signEnv,
            stdio: "pipe",
          });
      };
      await write("appimage", "Redrob Recall_0.2.0_amd64.AppImage", true);
      await write("deb", "Redrob Recall_0.2.0_amd64.deb", true);
      // The unpacked AppDir sits beside the AppImage and is not an asset.
      await mkdir(join(bundle, "appimage", "Redrob Recall.AppDir"), {
        recursive: true,
      });

      const assets = join(dir, "assets");
      const fragments = join(dir, "fragments");
      const env = { ...process.env, GITHUB_REPOSITORY: REPO };
      const staged = execFileSync(
        "node",
        [
          "scripts/stage-release-assets.mjs",
          bundle,
          "0.2.0",
          LINUX,
          assets,
          join(fragments, `${LINUX}.json`),
        ],
        { env, encoding: "utf8" },
      );
      expect(staged).toMatch(/staged 4 file\(s\)/);
      const fragment = JSON.parse(
        await readFile(join(fragments, `${LINUX}.json`), "utf8"),
      );
      expect(Object.keys(fragment.platforms)).toEqual([
        "linux-x86_64",
        "linux-x86_64-appimage",
        "linux-x86_64-deb",
      ]);
      expect(fragment.platforms["linux-x86_64"].signature).toBe(
        (
          await readFile(
            join(assets, "redrob-recall-0.2.0-linux-x86_64.AppImage.sig"),
            "utf8",
          )
        ).trim(),
      );

      const assembleEnv = {
        ...env,
        TAURI_UPDATER_PUBLIC_KEY: publicKey,
        REQUIRED_PLATFORMS: "linux-x86_64",
        REQUIRED_LAUNCHER_TOKENS: "linux-x86_64",
        REQUIRED_ASSET_SUFFIXES: ".AppImage .deb",
      };
      const assembled = execFileSync(
        "node",
        ["scripts/assemble-updater-manifest.mjs", assets, fragments, "0.2.0"],
        { env: assembleEnv, encoding: "utf8" },
      );
      expect(assembled).toMatch(/linux-x86_64: VALID/);
      expect(assembled).toMatch(/every one of 3 signatures verifies/);
      const latest = JSON.parse(
        await readFile(join(assets, "latest.json"), "utf8"),
      );
      expect(latest.version).toBe("0.2.0");
      expect(latest.platforms["linux-x86_64"].url).toBe(
        url("redrob-recall-0.2.0-linux-x86_64.AppImage"),
      );

      // A platform the matrix built but no leg wrote.
      expect(() =>
        execFileSync(
          "node",
          ["scripts/assemble-updater-manifest.mjs", assets, fragments, "0.2.0"],
          {
            env: {
              ...assembleEnv,
              REQUIRED_PLATFORMS: "linux-x86_64 darwin-aarch64",
            },
            stdio: "pipe",
          },
        ),
      ).toThrow(/no entry for built platform darwin-aarch64/);

      // Bytes changed after signing (the AppImage repack, if its re-sign step were skipped).
      await writeFile(
        join(assets, "redrob-recall-0.2.0-linux-x86_64.AppImage"),
        "repacked\n",
      );
      expect(() =>
        execFileSync(
          "node",
          ["scripts/assemble-updater-manifest.mjs", assets, fragments, "0.2.0"],
          {
            env: assembleEnv,
            stdio: "pipe",
          },
        ),
      ).toThrow(/updater signature does not verify/);
    } finally {
      await rm(dir, { recursive: true, force: true });
    }
  });
});
