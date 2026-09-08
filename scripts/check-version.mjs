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
