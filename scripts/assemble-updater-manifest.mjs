#!/usr/bin/env node
// Assemble latest.json from the build legs' updater fragments and assert the updater contract
// over the files that are about to be published -- the same bytes, before any client can see them.
//
// What it checks, and why each check is a build failure rather than a review item:
//   - every asset carries a name Redrob Launcher can install, and every built leg left an
//     installable one (otherwise the launcher silently skips the release);
//   - every bundle type a leg builds left a file (a leg that compiled and bundled nothing);
//   - latest.json names the release version, has an entry for every built platform, and every
//     entry points at this repository's own published download URL for a file in this set
//     (a missing or misdirected manifest does not break installation -- it silently freezes every
//     existing client on its current version);
//   - every signature VERIFIES against the bytes it names. "Has a signature" is not "has the right
//     signature": the AppImage is repacked after Tauri signed it, and a stale signature looks
//     signed until the first update check on a user's machine.
//
// Usage: node scripts/assemble-updater-manifest.mjs <assets-dir> <fragments-dir> <version>
// Env:   GITHUB_REPOSITORY, TAURI_UPDATER_PUBLIC_KEY,
//        REQUIRED_PLATFORMS (space-separated updater keys), REQUIRED_LAUNCHER_TOKENS,
//        REQUIRED_ASSET_SUFFIXES -- the three lists scripts/release-matrix.mjs emits.
// Writes <assets-dir>/latest.json.

import { execFileSync } from "node:child_process";
import { readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { checkReleaseAssetNames } from "./release-asset-names.mjs";
import {
  checkUpdaterManifest,
  mergeUpdaterFragments,
} from "./updater-manifest.mjs";

const [, , assetsDir, fragmentsDir, version] = process.argv;
if (!assetsDir || !fragmentsDir || !version) {
  console.error(
    "usage: assemble-updater-manifest.mjs <assets-dir> <fragments-dir> <version>",
  );
  process.exit(2);
}
const repository = process.env.GITHUB_REPOSITORY;
const publicKey = process.env.TAURI_UPDATER_PUBLIC_KEY?.trim();
if (!repository || !publicKey) {
  console.error("GITHUB_REPOSITORY and TAURI_UPDATER_PUBLIC_KEY are required");
  process.exit(2);
}
const words = (name) => (process.env[name] ?? "").split(/\s+/).filter(Boolean);
const requiredPlatforms = words("REQUIRED_PLATFORMS");
const requiredTokens = words("REQUIRED_LAUNCHER_TOKENS");
const requiredSuffixes = words("REQUIRED_ASSET_SUFFIXES");
const tag = `v${version}`;

const files = (dir) =>
  readdirSync(dir)
    .filter((name) => statSync(join(dir, name)).isFile())
    .sort();

const fail = (title, problems) => {
  console.error(`::error::${title}:\n- ${problems.join("\n- ")}`);
  process.exit(1);
};

const assetNames = files(assetsDir).filter((name) => name !== "latest.json");
console.log(`Assets for ${tag}:\n  ${assetNames.join("\n  ")}`);

const nameProblems = checkReleaseAssetNames(
  assetNames,
  version,
  requiredTokens,
);
if (nameProblems.length > 0)
  fail(`release ${tag} is not installable from Redrob Launcher`, nameProblems);

const missingSuffixes = requiredSuffixes.filter(
  (suffix) => !assetNames.some((name) => name.endsWith(suffix)),
);
if (missingSuffixes.length > 0) {
  fail(
    `a built platform produced no installer`,
    missingSuffixes.map((s) => `no asset ending in ${s}`),
  );
}

const fragments = files(fragmentsDir)
  .filter((name) => name.endsWith(".json"))
  .map((name) => JSON.parse(readFileSync(join(fragmentsDir, name), "utf8")));
if (fragments.length === 0)
  fail("no updater fragments", [`${fragmentsDir} holds no *.json`]);

const manifest = mergeUpdaterFragments(fragments, {
  version,
  notes: "See CHANGELOG.md for release notes.",
  pubDate: new Date().toISOString(),
});
const manifestProblems = checkUpdaterManifest(manifest, {
  version,
  repository,
  tag,
  requiredPlatforms,
  assetNames,
});
if (manifestProblems.length > 0)
  fail("latest.json does not describe this release", manifestProblems);

const verifier = fileURLToPath(
  new URL("./verify-updater-signature.mjs", import.meta.url),
);
const prefix = `https://github.com/${repository}/releases/download/${encodeURIComponent(tag)}/`;
let checked = 0;
for (const [platform, entry] of Object.entries(manifest.platforms)) {
  const asset = decodeURIComponent(entry.url.slice(prefix.length));
  let result;
  try {
    result = execFileSync(
      "node",
      [verifier, publicKey, entry.signature, join(assetsDir, asset)],
      {
        encoding: "utf8",
        stdio: ["ignore", "pipe", "inherit"],
      },
    ).trim();
  } catch (error) {
    result = `${error.stdout ?? ""}`.trim() || error.message;
    fail(`updater signature does not verify`, [
      `${platform}: ${result} (${asset})`,
    ]);
  }
  console.log(`${platform}: ${result} (${asset})`);
  checked += 1;
}

writeFileSync(
  join(assetsDir, "latest.json"),
  `${JSON.stringify(manifest, null, 2)}\n`,
);
console.log(
  `latest.json covers ${Object.keys(manifest.platforms).length} platform key(s) ` +
    `(required: ${requiredPlatforms.join(" ") || "none"}), every one of ${checked} signatures ` +
    `verifies against the bytes it names, and every asset is launcher-readable.`,
);
