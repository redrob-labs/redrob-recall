#!/usr/bin/env node
// Rename the assets tauri-action just uploaded to the draft release so Redrob Launcher can read
// them, and repoint latest.json at the new names.
//
// Runs on every build leg after tauri-action (and, on Linux, after the AppImage re-sign). The
// rename is a PATCH on the asset -- same bytes, same id, new name -- so nothing is re-uploaded and
// the updater signatures stay valid. latest.json is then rewritten because its platform entries
// name the files: tauri-action wrote them as REST asset URLs (the draft has no public download URL
// yet), and those ids resolve to the renamed assets, so the final published URL is fully known
// here -- `releases/download/<tag>/<new name>`. The verify job's rewrite step then finds them
// already correct.
//
// Usage: node scripts/rename-release-assets.mjs <owner/repo> <tag> <target>
// Env:   GH_TOKEN (the run's GITHUB_TOKEN, contents: write)

import { execFileSync } from "node:child_process";
import { mkdtempSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { launcherAssetName } from "./release-asset-names.mjs";

const [, , repository, tag, target] = process.argv;
if (!repository || !tag || !target) {
  console.error("usage: rename-release-assets.mjs <owner/repo> <tag> <target>");
  process.exit(2);
}
const version = tag.replace(/^v/, "");

function gh(args, input) {
  return execFileSync("gh", args, {
    encoding: "utf8",
    input,
    stdio: ["pipe", "pipe", "inherit"],
  });
}

// A DRAFT release is not on the tags endpoint; find it in the listing, which includes drafts for
// a caller with push access.
const releases = JSON.parse(
  gh(["api", `repos/${repository}/releases?per_page=100`]),
);
const release = releases.find((r) => r.tag_name === tag);
if (!release) {
  console.error(`::error::no release found for ${tag}`);
  process.exit(1);
}

const renames = new Map(); // old name -> new name
for (const asset of release.assets) {
  const newName = launcherAssetName(asset.name, version, target);
  if (!newName || newName === asset.name) continue;
  if (release.assets.some((a) => a.name === newName)) {
    console.error(
      `::error::${newName} already exists on ${tag}; refusing to rename ${asset.name} over it`,
    );
    process.exit(1);
  }
  gh([
    "api",
    "-X",
    "PATCH",
    `repos/${repository}/releases/assets/${asset.id}`,
    "-f",
    `name=${newName}`,
  ]);
  console.log(`renamed ${asset.name} -> ${newName}`);
  renames.set(asset.name, newName);
}
if (renames.size === 0) {
  console.log(`no asset of ${tag} needed renaming for ${target}`);
  process.exit(0);
}

const manifest = release.assets.find((a) => a.name === "latest.json");
if (!manifest) {
  // tauri-action only writes latest.json when it found a signature; the verify job fails the
  // release in that case, with the better message.
  console.log("latest.json is not attached yet; nothing to repoint");
  process.exit(0);
}
const latest = JSON.parse(
  gh([
    "api",
    "-H",
    "Accept: application/octet-stream",
    `repos/${repository}/releases/assets/${manifest.id}`,
  ]),
);
const namesById = new Map(release.assets.map((a) => [String(a.id), a.name]));
const publishedPrefix = `https://github.com/${repository}/releases/download/${encodeURIComponent(tag)}/`;
let repointed = 0;
for (const [platform, entry] of Object.entries(latest.platforms ?? {})) {
  const url = entry?.url;
  if (typeof url !== "string") continue;
  const restId = url.match(/\/releases\/assets\/(\d+)$/)?.[1];
  const oldName = restId
    ? namesById.get(restId)
    : decodeURIComponent(url.split("/").pop());
  const newName = oldName && renames.get(oldName);
  if (!newName) continue;
  entry.url = `${publishedPrefix}${encodeURIComponent(newName)}`;
  console.log(`  ${platform}: ${newName}`);
  repointed += 1;
}
if (repointed === 0) {
  console.log("latest.json names none of the renamed assets; left as is");
  process.exit(0);
}

const work = mkdtempSync(join(tmpdir(), "rename-assets-"));
try {
  const path = join(work, "latest.json");
  writeFileSync(path, `${JSON.stringify(latest, null, 2)}\n`);
  // An asset name is unique per release: the old manifest goes before the new one takes its name.
  gh([
    "api",
    "-X",
    "DELETE",
    `repos/${repository}/releases/assets/${manifest.id}`,
  ]);
  gh(["release", "upload", tag, path, "--repo", repository]);
} finally {
  rmSync(work, { recursive: true, force: true });
}
console.log(
  `renamed ${renames.size} asset(s) and repointed ${repointed} latest.json platform entr${repointed === 1 ? "y" : "ies"}`,
);
