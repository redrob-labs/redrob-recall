#!/usr/bin/env node
// Stage one build leg's bundles for the release artifact: copy every installer and updater payload
// Tauri wrote under `bundle/` into <out-dir> under the name Redrob Launcher installs
// (release-asset-names.mjs), and write the leg's updater platform entries as a fragment the
// updater-manifest job merges into latest.json.
//
// Replaces the asset renaming tauri-action's draft upload needed (a PATCH per asset plus a
// latest.json rewrite): the files are named right before they leave the runner, and the manifest
// URLs are the published ones from the start.
//
// Usage: node scripts/stage-release-assets.mjs <bundle-dir> <version> <target> <out-dir> <fragment>
//   bundle-dir  src-tauri/target/<target>/release/bundle
//   version     release version without the leading "v"
//   target      Rust target triple of this leg
//   out-dir     directory to fill with the launcher-named files (uploaded as release-<target>)
//   fragment    path of the updater fragment to write (uploaded as updater-<target>)
// Env:   GITHUB_REPOSITORY (owner/repo the published URLs point at)

import {
  copyFileSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { dirname, join } from "node:path";
import { launcherAssetName } from "./release-asset-names.mjs";
import { publishedDownloadUrl, updaterEntries } from "./updater-manifest.mjs";

const [, , bundleDir, version, target, outDir, fragmentPath] = process.argv;
if (!bundleDir || !version || !target || !outDir || !fragmentPath) {
  console.error(
    "usage: stage-release-assets.mjs <bundle-dir> <version> <target> <out-dir> <fragment>",
  );
  process.exit(2);
}
const repository = process.env.GITHUB_REPOSITORY;
if (!repository) {
  console.error(
    "GITHUB_REPOSITORY is required to write the published download URLs",
  );
  process.exit(2);
}
const tag = `v${version}`;

// Tauri writes one directory per bundle type (appimage/, deb/, dmg/, macos/, nsis/, msi/); the
// files sit one level down. Deeper trees are the unpacked .app and the AppDir, not assets.
const candidates = [];
for (const kind of readdirSync(bundleDir)) {
  const dir = join(bundleDir, kind);
  if (!statSync(dir).isDirectory()) continue;
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    if (statSync(path).isFile()) candidates.push({ name, path });
  }
}

mkdirSync(outDir, { recursive: true });
const staged = new Map(); // launcher name -> source path
for (const { name, path } of candidates) {
  const newName = launcherAssetName(name, version, target);
  if (!newName) continue;
  if (staged.has(newName)) {
    console.error(
      `::error::${newName} would be written from both ${staged.get(newName)} and ${path}`,
    );
    process.exit(1);
  }
  copyFileSync(path, join(outDir, newName));
  staged.set(newName, path);
  console.log(`staged ${name} -> ${newName}`);
}
if (staged.size === 0) {
  console.error(
    `::error::no bundle under ${bundleDir} matched version ${version} for ${target}`,
  );
  process.exit(1);
}

const platforms = {};
for (const { key, asset, sig } of updaterEntries([...staged.keys()], target)) {
  platforms[key] = {
    signature: readFileSync(join(outDir, sig), "utf8").trim(),
    url: publishedDownloadUrl(repository, tag, asset),
  };
  console.log(`updater ${key}: ${asset}`);
}
mkdirSync(dirname(fragmentPath), { recursive: true });
writeFileSync(
  fragmentPath,
  `${JSON.stringify({ target, platforms }, null, 2)}\n`,
);
console.log(
  `staged ${staged.size} file(s) into ${outDir}; updater fragment at ${fragmentPath}`,
);
