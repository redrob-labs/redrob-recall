import { createHash } from "node:crypto";
import { createReadStream } from "node:fs";
import {
  lstat,
  mkdir,
  readFile,
  readdir,
  rename,
  rm,
  writeFile,
} from "node:fs/promises";
import { dirname, resolve } from "node:path";

const CDN_BASE = "https://cdn.redrob.ai/recall";
const PLATFORM_ASSET_SUFFIXES = {
  "linux-x86_64": ".AppImage.tar.gz",
  "darwin-aarch64": ".app.tar.gz",
  "darwin-x86_64": ".app.tar.gz",
  "windows-x86_64": ".nsis.zip",
};
const REQUIRED_PLATFORMS = Object.keys(PLATFORM_ASSET_SUFFIXES);
const RELEASE_TAG_PATTERN = /^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;
const VERSION_PATTERN = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;

function requiredEnvironmentPath(name) {
  const value = process.env[name]?.trim();
  if (!value) throw new Error(`${name} is required.`);
  return resolve(value);
}

function parseJson(contents, description) {
  try {
    return JSON.parse(contents);
  } catch (error) {
    throw new Error(`${description} is not valid JSON: ${error.message}`);
  }
}

function isObject(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function validateAssetName(name) {
  if (
    typeof name !== "string" ||
    name.length === 0 ||
    name.includes("/") ||
    name.includes("\\") ||
    name.includes("..") ||
    /[\u0000-\u001f\u007f]/u.test(name)
  ) {
    throw new Error(
      `Release asset has an unsafe name: ${JSON.stringify(name)}.`,
    );
  }
}

function parseVersion(version, description) {
  const match =
    typeof version === "string" ? VERSION_PATTERN.exec(version) : null;
  if (!match) {
    throw new Error(
      `${description} must be a stable numeric MAJOR.MINOR.PATCH.`,
    );
  }
  return match.slice(1).map((part) => BigInt(part));
}

function compareVersions(left, right) {
  for (let index = 0; index < left.length; index += 1) {
    if (left[index] < right[index]) return -1;
    if (left[index] > right[index]) return 1;
  }
  return 0;
}

function encodePathSegment(name) {
  return encodeURIComponent(name).replace(
    /[!'()*]/gu,
    (character) => `%${character.charCodeAt(0).toString(16).toUpperCase()}`,
  );
}

function contentTypeFor(name) {
  const lowerName = name.toLowerCase();
  if (lowerName.endsWith(".tar.gz") || lowerName.endsWith(".tgz")) {
    return "application/gzip";
  }
  if (lowerName.endsWith(".zip")) return "application/zip";
  if (lowerName.endsWith(".exe")) {
    return "application/vnd.microsoft.portable-executable";
  }
  if (lowerName.endsWith(".msi")) return "application/x-msi";
  if (lowerName.endsWith(".dmg")) return "application/x-apple-diskimage";
  if (lowerName.endsWith(".deb")) {
    return "application/vnd.debian.binary-package";
  }
  if (lowerName.endsWith(".rpm")) return "application/x-rpm";
  if (lowerName.endsWith(".appimage")) return "application/vnd.appimage";
  if (lowerName.endsWith(".sig")) return "application/octet-stream";
  return "application/octet-stream";
}

async function sha256(path) {
  const hash = createHash("sha256");
  for await (const chunk of createReadStream(path)) hash.update(chunk);
  return hash.digest("hex");
}

function stableValue(value) {
  if (Array.isArray(value)) return value.map(stableValue);
  if (!isObject(value)) return value;
  return Object.fromEntries(
    Object.keys(value)
      .sort()
      .map((key) => [key, stableValue(value[key])]),
  );
}

function prettyJson(value) {
  return `${JSON.stringify(stableValue(value), null, 2)}\n`;
}

async function writeOutput(path, contents) {
  await mkdir(dirname(path), { recursive: true });
  const temporaryPath = `${path}.tmp-${process.pid}`;
  try {
    await writeFile(temporaryPath, contents, { flag: "wx" });
    await rename(temporaryPath, path);
  } finally {
    await rm(temporaryPath, { force: true });
  }
}

const releaseTag = process.env.RELEASE_TAG?.trim();
const tagMatch = RELEASE_TAG_PATTERN.exec(releaseTag ?? "");
if (!tagMatch) {
  throw new Error(
    "RELEASE_TAG must match vMAJOR.MINOR.PATCH with numeric parts.",
  );
}
const candidateVersion = tagMatch.slice(1).join(".");
const candidateVersionParts = parseVersion(candidateVersion, "Release version");

const releaseAssetsPath = requiredEnvironmentPath("RELEASE_ASSETS_PATH");
const releaseAssetsDirectory = requiredEnvironmentPath("RELEASE_ASSETS_DIR");
const cdnPlanPath = requiredEnvironmentPath("CDN_PLAN_PATH");
const cdnManifestPath = requiredEnvironmentPath("CDN_MANIFEST_PATH");
if (cdnPlanPath === cdnManifestPath) {
  throw new Error(
    "CDN_PLAN_PATH and CDN_MANIFEST_PATH must be different files.",
  );
}

const rawAssets = parseJson(
  await readFile(releaseAssetsPath, "utf8"),
  "Release assets metadata",
);
if (!Array.isArray(rawAssets) || rawAssets.length === 0) {
  throw new Error("Release assets metadata must be a nonempty JSON array.");
}

const assetsById = new Map();
const assetsByName = new Map();
const assetsByBrowserUrl = new Map();
for (const rawAsset of rawAssets) {
  if (!isObject(rawAsset))
    throw new Error("Every release asset must be an object.");
  const {
    id,
    name,
    size,
    state,
    digest,
    browser_download_url: browserDownloadUrl,
  } = rawAsset;
  if (!Number.isSafeInteger(id) || id <= 0) {
    throw new Error(
      `Release asset ${JSON.stringify(name)} has an invalid numeric ID.`,
    );
  }
  validateAssetName(name);
  if (!Number.isSafeInteger(size) || size < 0) {
    throw new Error(
      `Release asset ${JSON.stringify(name)} has an invalid size.`,
    );
  }
  if (state !== "uploaded") {
    throw new Error(
      `Release asset ${JSON.stringify(name)} is not in the uploaded state.`,
    );
  }
  if (typeof digest !== "string" || !/^sha256:[0-9a-f]{64}$/u.test(digest)) {
    throw new Error(
      `Release asset ${JSON.stringify(name)} has no valid SHA-256 digest.`,
    );
  }
  if (typeof browserDownloadUrl !== "string" || !browserDownloadUrl) {
    throw new Error(
      `Release asset ${JSON.stringify(name)} has no download URL.`,
    );
  }
  let parsedBrowserUrl;
  try {
    parsedBrowserUrl = new URL(browserDownloadUrl);
  } catch {
    throw new Error(
      `Release asset ${JSON.stringify(name)} has an invalid download URL.`,
    );
  }
  if (parsedBrowserUrl.protocol !== "https:") {
    throw new Error(
      `Release asset ${JSON.stringify(name)} download URL must use HTTPS.`,
    );
  }
  if (assetsById.has(id)) throw new Error(`Duplicate release asset ID ${id}.`);
  if (assetsByName.has(name)) {
    throw new Error(`Duplicate release asset name ${JSON.stringify(name)}.`);
  }
  if (assetsByBrowserUrl.has(browserDownloadUrl)) {
    throw new Error(
      `Duplicate release asset download URL for ${JSON.stringify(name)}.`,
    );
  }
  const asset = {
    id,
    name,
    size,
    expectedSha256: digest.slice("sha256:".length),
    browserDownloadUrl,
  };
  assetsById.set(id, asset);
  assetsByName.set(name, asset);
  assetsByBrowserUrl.set(browserDownloadUrl, asset);
}

const latestAssets = rawAssets.filter((asset) => asset.name === "latest.json");
if (latestAssets.length !== 1) {
  throw new Error("The release must contain exactly one latest.json asset.");
}

const downloadedEntries = await readdir(releaseAssetsDirectory, {
  withFileTypes: true,
});
const downloadedNames = downloadedEntries.map((entry) => entry.name).sort();
const expectedNames = [...assetsByName.keys()].sort();
if (
  downloadedNames.length !== expectedNames.length ||
  downloadedNames.some((name, index) => name !== expectedNames[index])
) {
  throw new Error(
    "RELEASE_ASSETS_DIR must contain exactly the assets listed in RELEASE_ASSETS_PATH.",
  );
}
for (const entry of downloadedEntries) {
  const asset = assetsByName.get(entry.name);
  const localPath = resolve(releaseAssetsDirectory, entry.name);
  const file = await lstat(localPath);
  if (!entry.isFile() || !file.isFile() || file.isSymbolicLink()) {
    throw new Error(
      `Downloaded asset ${JSON.stringify(entry.name)} is not a regular file.`,
    );
  }
  if (file.size !== asset.size) {
    throw new Error(
      `Downloaded asset ${JSON.stringify(entry.name)} has size ${file.size}; expected ${asset.size}.`,
    );
  }
  asset.sha256 = await sha256(localPath);
  if (asset.sha256 !== asset.expectedSha256) {
    throw new Error(
      `Downloaded asset ${JSON.stringify(entry.name)} does not match its GitHub SHA-256 digest.`,
    );
  }
  asset.localPath = localPath;
}

const latestAsset = assetsByName.get("latest.json");
const manifest = parseJson(
  await readFile(latestAsset.localPath, "utf8"),
  "Release latest.json",
);
if (!isObject(manifest))
  throw new Error("Release latest.json must be an object.");
if (manifest.version !== candidateVersion) {
  throw new Error(
    `Release latest.json version ${JSON.stringify(manifest.version)} does not match ${candidateVersion}.`,
  );
}
if (
  !isObject(manifest.platforms) ||
  Object.keys(manifest.platforms).length === 0
) {
  throw new Error("Release latest.json platforms must be a nonempty object.");
}
const platformNames = Object.keys(manifest.platforms);
for (const platform of REQUIRED_PLATFORMS) {
  if (!Object.hasOwn(manifest.platforms, platform)) {
    throw new Error(
      `Release latest.json is missing required platform ${platform}.`,
    );
  }
}
const unexpectedPlatforms = platformNames.filter(
  (platform) => !Object.hasOwn(PLATFORM_ASSET_SUFFIXES, platform),
);
if (unexpectedPlatforms.length > 0) {
  throw new Error(
    `Release latest.json has unsupported platforms: ${unexpectedPlatforms.join(", ")}.`,
  );
}

const referencedAssetIds = new Set();
for (const [platform, entry] of Object.entries(manifest.platforms)) {
  if (!isObject(entry)) {
    throw new Error(
      `Updater platform ${JSON.stringify(platform)} must be an object.`,
    );
  }
  if (typeof entry.signature !== "string" || !entry.signature.trim()) {
    throw new Error(
      `Updater platform ${JSON.stringify(platform)} needs a signature.`,
    );
  }
  if (typeof entry.url !== "string" || !entry.url.trim()) {
    throw new Error(
      `Updater platform ${JSON.stringify(platform)} needs a URL.`,
    );
  }

  let parsedUrl;
  try {
    parsedUrl = new URL(entry.url);
  } catch {
    throw new Error(
      `Updater platform ${JSON.stringify(platform)} has an invalid URL.`,
    );
  }
  if (parsedUrl.protocol !== "https:") {
    throw new Error(
      `Updater platform ${JSON.stringify(platform)} URL must use HTTPS.`,
    );
  }

  let asset = assetsByBrowserUrl.get(entry.url);
  if (!asset) {
    const idMatch = /\/releases\/assets\/(\d+)\/?$/u.exec(parsedUrl.pathname);
    if (idMatch) {
      const id = Number(idMatch[1]);
      if (Number.isSafeInteger(id)) asset = assetsById.get(id);
    }
  }
  if (!asset) {
    throw new Error(
      `Updater platform ${JSON.stringify(platform)} URL does not resolve to a release asset.`,
    );
  }
  if (asset.name === "latest.json") {
    throw new Error("latest.json cannot be used as an updater payload.");
  }
  const requiredSuffix = PLATFORM_ASSET_SUFFIXES[platform];
  if (!asset.name.endsWith(requiredSuffix)) {
    throw new Error(
      `Updater platform ${JSON.stringify(platform)} must reference an ${requiredSuffix} asset.`,
    );
  }

  referencedAssetIds.add(asset.id);
  entry.url = `${CDN_BASE}/v${candidateVersion}/${encodePathSegment(asset.name)}`;
}

const currentManifestPath = process.env.CURRENT_MANIFEST_PATH?.trim();
if (currentManifestPath) {
  const currentManifest = parseJson(
    await readFile(resolve(currentManifestPath), "utf8"),
    "Current CDN manifest",
  );
  if (!isObject(currentManifest)) {
    throw new Error("Current CDN manifest must be an object.");
  }
  const currentVersionParts = parseVersion(
    currentManifest.version,
    "Current CDN manifest version",
  );
  if (compareVersions(candidateVersionParts, currentVersionParts) <= 0) {
    throw new Error(
      `Candidate version ${candidateVersion} must be newer than current version ${currentManifest.version}.`,
    );
  }
}

const manifestContents = prettyJson(manifest);
const manifestSha256 = createHash("sha256")
  .update(manifestContents)
  .digest("hex");
const versionPrefix = `recall/v${candidateVersion}`;
const planAssets = [];
for (const asset of [...assetsByName.values()].sort((left, right) => {
  if (left.name < right.name) return -1;
  if (left.name > right.name) return 1;
  return 0;
})) {
  if (asset.name === "latest.json") continue;
  const encodedName = encodePathSegment(asset.name);
  planAssets.push({
    localPath: asset.localPath,
    key: `${versionPrefix}/${asset.name}`,
    url: `${CDN_BASE}/v${candidateVersion}/${encodedName}`,
    sha256: asset.sha256,
    size: asset.size,
    contentType: contentTypeFor(asset.name),
    referenced: referencedAssetIds.has(asset.id),
  });
}

const plan = {
  candidateVersion,
  versionPrefix,
  manifestKey: "recall/latest.json",
  manifestSha256,
  assets: planAssets,
};

await writeOutput(cdnManifestPath, manifestContents);
await writeOutput(cdnPlanPath, prettyJson(plan));
console.log(
  `Prepared CDN release v${candidateVersion} with ${planAssets.length} immutable assets.`,
);
