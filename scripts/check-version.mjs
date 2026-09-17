import { readFile } from "node:fs/promises";

const [packageJson, cargoToml, tauriConfig] = await Promise.all([
  readFile(new URL("../package.json", import.meta.url), "utf8").then(
    JSON.parse,
  ),
  readFile(new URL("../src-tauri/Cargo.toml", import.meta.url), "utf8"),
  readFile(
    new URL("../src-tauri/tauri.conf.json", import.meta.url),
    "utf8",
  ).then(JSON.parse),
]);

const cargoVersion = cargoToml.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
const versions = new Set([
  packageJson.version,
  cargoVersion,
  tauriConfig.version,
]);
if (versions.size !== 1 || versions.has(undefined)) {
  throw new Error(
    `Version mismatch: package=${packageJson.version}, cargo=${cargoVersion}, tauri=${tauriConfig.version}`,
  );
}

const version = packageJson.version;
const tag =
  process.env.GITHUB_REF_TYPE === "tag"
    ? process.env.GITHUB_REF_NAME
    : undefined;
if (tag && tag !== `v${version}`) {
  throw new Error(
    `Release tag ${tag} must match application version v${version}`,
  );
}
console.log(`Version ${version} is consistent across all manifests.`);

// The AppImage bundler needs a square icon and finds it only through
// `bundle.icon`. That key was absent, so the first release run compiled the whole
// application in eight minutes and then died at packaging with "couldn't find a square
// icon to use as AppImage icon" -- a defect that only a real bundle build can reveal,
// which is exactly why it survived until the first tag.
//
// PNG geometry and colour type come straight out of the IHDR: bytes 16..24 are width and
// height, byte 25 is the colour type (6 = RGBA). No imaging library needed.
const iconPaths = tauriConfig.bundle?.icon;
if (!Array.isArray(iconPaths) || iconPaths.length === 0) {
  throw new Error(
    "src-tauri/tauri.conf.json must set bundle.icon; without it the AppImage bundler cannot find a square icon and packaging fails after a full release build",
  );
}

const iconProblems = [];
let squarePngCount = 0;
for (const relative of iconPaths) {
  const path = new URL(`../src-tauri/${relative}`, import.meta.url);
  let bytes;
  try {
    bytes = await readFile(path);
  } catch {
    iconProblems.push(`${relative} is listed in bundle.icon but does not exist`);
    continue;
  }
  if (!relative.endsWith(".png")) continue;
  if (bytes.subarray(0, 8).toString("latin1") !== "\x89PNG\r\n\x1a\n") {
    iconProblems.push(`${relative} is not a PNG`);
    continue;
  }
  const width = bytes.readUInt32BE(16);
  const height = bytes.readUInt32BE(20);
  if (width !== height) {
    iconProblems.push(`${relative} is ${width}x${height}, not square`);
  } else {
    squarePngCount += 1;
  }
  if (bytes[25] !== 6) {
    iconProblems.push(`${relative} is not RGBA (PNG colour type ${bytes[25]})`);
  }
}
if (squarePngCount === 0) {
  iconProblems.push(
    "bundle.icon lists no square PNG; the AppImage bundler has nothing to use",
  );
}
if (iconProblems.length > 0) {
  throw new Error(`Bundle icon problems:\n- ${iconProblems.join("\n- ")}`);
}

console.log(
  `Bundle icons are usable: ${squarePngCount} square RGBA PNG(s) in bundle.icon.`,
);

// A lockfile that DECLARES an optional platform dependency but carries no package block for it
// installs correctly only on the platform it was generated on: `npm ci` skips an optional
// dependency it has no entry for, without a word, and the build then dies much later at the
// point the missing native binary is needed. The sibling product shipped exactly that state --
// one of eleven Tauri CLI platform blocks, and no esbuild or rollup blocks at all -- so its
// macOS legs could never have worked. This lockfile is currently complete; the check exists
// because Dependabot rewrites this file constantly, which is when it would regress.
const lockPackages =
  JSON.parse(
    await readFile(new URL("../package-lock.json", import.meta.url), "utf8"),
  ).packages ?? {};
const missingPlatformBlocks = [];
for (const [key, meta] of Object.entries(lockPackages)) {
  for (const dependency of Object.keys(meta.optionalDependencies ?? {})) {
    if (`node_modules/${dependency}` in lockPackages) continue;
    missingPlatformBlocks.push(
      `${dependency} (declared by ${key === "" ? "the root package" : key})`,
    );
  }
}
// fsevents is the one legitimate absence: a macOS-only file WATCHER for dev mode, declared as
// a version range rather than pinned, and no production bundle step needs it.
const unexplainedMissing = missingPlatformBlocks.filter(
  (entry) => !entry.startsWith("fsevents "),
);
if (unexplainedMissing.length > 0) {
  throw new Error(
    "package-lock.json declares optional platform dependencies with no package block, so " +
      `npm ci cannot install them off this platform:\n- ${unexplainedMissing.join("\n- ")}`,
  );
}
console.log(
  "Lockfile carries a package block for every declared optional platform dependency.",
);
