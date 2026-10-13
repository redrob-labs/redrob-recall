#!/usr/bin/env node
// Write `redrob-launcher.json`, the asset Redrob Launcher reads from every release to learn that
// the release uses the launcher's shared sign-in. One source of truth with the check: the content
// comes from LAUNCHER_MANIFEST in release-asset-names.mjs and is re-read through
// checkLauncherManifest before this exits, so a hand edit that breaks the contract fails here,
// not on a user's launcher.
//
// Usage: node scripts/write-launcher-manifest.mjs <output-path>

import { writeFile, readFile } from "node:fs/promises";
import {
  checkLauncherManifest,
  launcherManifestText,
} from "./release-asset-names.mjs";

const [, , outputPath] = process.argv;
if (!outputPath) {
  console.error("usage: write-launcher-manifest.mjs <output-path>");
  process.exit(2);
}
await writeFile(outputPath, launcherManifestText(), "utf8");
const problems = checkLauncherManifest(await readFile(outputPath, "utf8"));
if (problems.length > 0) {
  console.error(`::error::${problems.join("\n")}`);
  process.exit(1);
}
console.log(`wrote ${outputPath}: ${launcherManifestText().trimEnd()}`);
