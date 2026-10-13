#!/usr/bin/env node
// Fail the release unless every asset on it carries a name Redrob Launcher can install, and the
// release carries `redrob-launcher.json` (the launcher's release manifest).
//
// Usage: node scripts/check-release-asset-names.mjs <release-json> <tag> [required-tokens...]
//   release-json    the release object as `gh api repos/<repo>/releases/...` returns it
//   required-tokens os-arch tokens (linux-x86_64 macos-aarch64 ...) that must each have an
//                   installable asset; pass the tokens of the legs this run built
//   LAUNCHER_MANIFEST_PATH (env, optional) a copy of the release's redrob-launcher.json as
//                   downloaded back from the release; its content is checked too (schema 1,
//                   sharedAccount true) so a stale or hand-edited upload fails here.

import { readFile } from "node:fs/promises";
import {
  LAUNCHER_MANIFEST_NAME,
  checkLauncherManifest,
  checkReleaseAssetNames,
  parseLauncherAssetName,
} from "./release-asset-names.mjs";

const [, , releaseJsonPath, tag, ...requiredTokens] = process.argv;
if (!releaseJsonPath || !tag) {
  console.error(
    "usage: check-release-asset-names.mjs <release-json> <tag> [required-tokens...]",
  );
  process.exit(2);
}
const release = JSON.parse(await readFile(releaseJsonPath, "utf8"));
const names = (release.assets ?? []).map((a) => a.name);
const problems = checkReleaseAssetNames(
  names,
  tag.replace(/^v/, ""),
  requiredTokens,
);
const manifestPath = process.env.LAUNCHER_MANIFEST_PATH;
if (manifestPath) {
  problems.push(...checkLauncherManifest(await readFile(manifestPath, "utf8")));
}
if (problems.length > 0) {
  console.error(
    `::error::release ${tag} is not installable from Redrob Launcher:\n- ${problems.join("\n- ")}`,
  );
  process.exit(1);
}
const installable = names.filter((n) => parseLauncherAssetName(n)?.installable);
console.log(
  `Every asset on ${tag} is launcher-readable, ${LAUNCHER_MANIFEST_NAME} is attached${manifestPath ? " and valid" : ""}; installable: ${installable.join(" ")}`,
);
