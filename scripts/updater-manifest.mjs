// The Tauri updater manifest (`latest.json`) this repository assembles from its own build legs.
//
// tauri-action used to write this file while uploading to a draft release. The release is now
// created by the shared redrob-labs publish job from artifacts, so there is no release to upload
// to during the build and the manifest has to be written from the staged files instead. That is
// an improvement, not a workaround: every URL is the PUBLISHED download URL from the start
// (fully determined by the tag and the asset name), where the draft flow wrote REST asset ids
// that two later steps had to rewrite.
//
// Shape, as the updater plugin reads it and as tauri-action wrote it:
//   { version, notes, pub_date, platforms: { "<os>-<arch>": { signature, url }, ... } }
// Each built leg contributes its platform entries (`updaterEntries`); the updater-manifest job
// merges them (`mergeUpdaterFragments`) and asserts the result (`checkUpdaterManifest`).
//
// Pure module, unit-tested; stage-release-assets.mjs and assemble-updater-manifest.mjs do the I/O.

import {
  parseLauncherAssetName,
  TARGET_TOKENS,
} from "./release-asset-names.mjs";

// The payload the updater installs on each OS -- the entry under the bare `<os>-<arch>` key.
// Windows prefers the NSIS installer (tauri-action's `updaterJsonPreferNsis`), the per-user
// install this app ships; the MSI stays reachable under its own suffixed key.
const PREFERRED_PAYLOAD = {
  linux: (name) => name.endsWith(".AppImage"),
  macos: (name) => name.endsWith(".app.tar.gz"),
  windows: (name) => name.endsWith("-setup.exe"),
};

// Suffixed keys, one per signed bundle, the way tauri-action also wrote them
// (`linux-x86_64-appimage`, `windows-x86_64-msi`, ...). Harmless to a client that resolves the
// bare key; useful to one pinned to a bundle type.
const BUNDLE_KEYS = [
  [/\.AppImage$/, "appimage"],
  [/\.deb$/, "deb"],
  [/\.rpm$/, "rpm"],
  [/\.app\.tar\.gz$/, "app"],
  [/-setup\.exe$/, "nsis"],
  [/\.msi$/, "msi"],
];

export function publishedDownloadUrl(repository, tag, assetName) {
  return `https://github.com/${repository}/releases/download/${encodeURIComponent(tag)}/${encodeURIComponent(assetName)}`;
}

/**
 * The updater platform entries one build leg contributes, given the launcher-named files it
 * staged. Every payload that has a `.sig` beside it gets a suffixed key; the OS's preferred
 * payload also gets the bare `<os>-<arch>` key. Returns `[{ key, asset, sig }]`; the caller
 * fills `signature` from the `.sig` file's text.
 *
 * @param {string[]} stagedNames launcher-shaped names present in the leg's release directory
 * @param {string} target        Rust target triple of the leg
 */
export function updaterEntries(stagedNames, target) {
  const leg = TARGET_TOKENS[target];
  if (!leg)
    throw new Error(
      `unknown release target ${target}; add it to TARGET_TOKENS`,
    );
  const os = leg.token.split("-")[0];
  const preferred = PREFERRED_PAYLOAD[os];
  if (!preferred) throw new Error(`no updater payload rule for ${os}`);
  const names = new Set(stagedNames);
  const entries = [];
  for (const name of stagedNames) {
    if (name.endsWith(".sig")) continue;
    // Only this leg's own files: a Linux leg never describes the macOS payload.
    if (parseLauncherAssetName(name)?.token !== leg.token) continue;
    if (!names.has(`${name}.sig`)) continue;
    const bundle = BUNDLE_KEYS.find(([pattern]) => pattern.test(name))?.[1];
    if (!bundle) continue;
    if (preferred(name))
      entries.push({ key: leg.updater, asset: name, sig: `${name}.sig` });
    entries.push({
      key: `${leg.updater}-${bundle}`,
      asset: name,
      sig: `${name}.sig`,
    });
  }
  if (!entries.some((entry) => entry.key === leg.updater)) {
    throw new Error(
      `${target} staged no signed updater payload for ${leg.updater}; expected a signed ` +
        `${os === "linux" ? ".AppImage" : os === "macos" ? ".app.tar.gz" : "-setup.exe"}`,
    );
  }
  return entries;
}

/**
 * Merge the per-leg fragments (`{ platforms: {...} }`) into one manifest. A key written by two
 * legs is a configuration error, not something to resolve by order.
 */
export function mergeUpdaterFragments(fragments, { version, notes, pubDate }) {
  const platforms = {};
  for (const fragment of fragments) {
    for (const [key, entry] of Object.entries(fragment.platforms ?? {})) {
      if (key in platforms)
        throw new Error(`updater platform ${key} is written by two build legs`);
      platforms[key] = entry;
    }
  }
  return { version, notes, pub_date: pubDate, platforms };
}

/**
 * Problems with an assembled manifest against the release it describes; empty means a client
 * that reads it will download a file this release carries, from this repository, with a
 * signature entry to verify it against. Signature VALIDITY is checked separately against the
 * bytes (verify-updater-signature.mjs); this is the shape check.
 *
 * @param {object} manifest
 * @param {{ version: string, repository: string, tag: string, requiredPlatforms: string[],
 *   assetNames: string[] }} release
 */
export function checkUpdaterManifest(manifest, release) {
  const problems = [];
  if (manifest.version !== release.version) {
    problems.push(
      `version ${JSON.stringify(manifest.version)} is not the release version ${release.version}`,
    );
  }
  if (
    typeof manifest.pub_date !== "string" ||
    Number.isNaN(Date.parse(manifest.pub_date))
  ) {
    problems.push(
      `pub_date ${JSON.stringify(manifest.pub_date)} is not a date`,
    );
  }
  const platforms = manifest.platforms ?? {};
  const assets = new Set(release.assetNames);
  const prefix = `https://github.com/${release.repository}/releases/download/${encodeURIComponent(release.tag)}/`;
  if (Object.keys(platforms).length === 0) problems.push("platforms is empty");
  for (const [key, entry] of Object.entries(platforms)) {
    if (typeof entry?.url !== "string" || !entry.url.startsWith(prefix)) {
      problems.push(
        `${key}: url is not a release asset of ${release.repository} at ${release.tag}: ${entry?.url}`,
      );
      continue;
    }
    const asset = decodeURIComponent(entry.url.slice(prefix.length));
    if (!assets.has(asset))
      problems.push(
        `${key}: names ${asset}, which is not among the release assets`,
      );
    if (!parseLauncherAssetName(asset))
      problems.push(`${key}: ${asset} is not a launcher-shaped asset name`);
    if (typeof entry.signature !== "string" || entry.signature.length === 0) {
      problems.push(`${key}: has no signature`);
    }
  }
  for (const platform of release.requiredPlatforms) {
    if (!(platform in platforms))
      problems.push(`no entry for built platform ${platform}`);
  }
  return problems;
}
