// Release asset names Redrob Launcher can read.
//
// The launcher installs an asset only when its name is
//     <stem>-<version>-<os>-<arch>[-portable].<ext>
// with the stem `redrob-recall`, the version without a leading "v", and os-arch one of the tokens
// in TARGET_TOKENS below (linux-x86_64, macos-aarch64, windows-x64, ...). Tauri's bundler names
// files `Redrob Recall_0.2.0_amd64.AppImage` (GitHub turns the space into a dot on upload), which
// the launcher cannot parse: wrong stem, `_` separators, Debian's `amd64` instead of `x86_64`, and
// no os token at all. So each build leg copies its bundles into the release artifact under the
// name `launcherAssetName` gives (scripts/stage-release-assets.mjs) and the updater-manifest job
// asserts the whole set with `checkReleaseAssetNames`, so a bundler upgrade that changes the file
// names cannot silently make the app uninstallable again.
//
// `redrob-launcher.json` and `SHA256SUMS.txt` are not this repository's to write: the shared
// redrob-labs release-publish workflow adds both when it creates the release.
//
// This module is pure (no I/O) so it can be unit-tested; stage-release-assets.mjs and
// assemble-updater-manifest.mjs do the file work.

export const STEM = "redrob-recall";
export const PRODUCT_NAME = "Redrob Recall";

// Files the shared publish job adds beside the installers, and the updater manifest this
// repository assembles itself. None of them is an installer, so none must parse as one.
export const NON_INSTALLER_ASSETS = new Set([
  "latest.json",
  "redrob-launcher.json",
  "SHA256SUMS.txt",
]);

// Rust target triple -> { token the launcher expects, arch tokens Tauri puts in the file name,
// the updater platform key (`<os>-<arch>` as the Tauri updater resolves `{{target}}-{{arch}}`) }.
// Tauri's arch token differs per bundle: `amd64`/`arm64` on .deb, `x86_64`/`aarch64` on .rpm and
// .AppImage, `x64`/`arm64` on Windows, `aarch64`/`x64`/`universal` on macOS.
export const TARGET_TOKENS = {
  "x86_64-unknown-linux-gnu": {
    token: "linux-x86_64",
    tauriArches: ["amd64", "x86_64"],
    updater: "linux-x86_64",
  },
  "aarch64-unknown-linux-gnu": {
    token: "linux-aarch64",
    tauriArches: ["arm64", "aarch64"],
    updater: "linux-aarch64",
  },
  "aarch64-apple-darwin": {
    token: "macos-aarch64",
    tauriArches: ["aarch64"],
    updater: "darwin-aarch64",
  },
  "x86_64-apple-darwin": {
    token: "macos-x86_64",
    tauriArches: ["x64"],
    updater: "darwin-x86_64",
  },
  "universal-apple-darwin": {
    token: "macos-universal",
    tauriArches: ["universal"],
    updater: "darwin-universal",
  },
  "x86_64-pc-windows-msvc": {
    token: "windows-x64",
    tauriArches: ["x64"],
    updater: "windows-x86_64",
  },
  "aarch64-pc-windows-msvc": {
    token: "windows-arm64",
    tauriArches: ["arm64"],
    updater: "windows-aarch64",
  },
  "x86_64-unknown-freebsd": {
    token: "freebsd-x86_64",
    tauriArches: ["amd64", "x86_64"],
    updater: "freebsd-x86_64",
  },
};

// Exactly the os-arch tokens the launcher's Platform::tokens produces; `linux-x64` or
// `windows-x86_64` parse there but never match a platform, so they are rejected here.
const LAUNCHER_TOKENS = new Set(
  Object.values(TARGET_TOKENS)
    .map((leg) => leg.token)
    .concat(["windows-x86"]),
);
// Which file suffixes each OS's bundler produces. A leg renames only its own OS's files, so a
// Linux aarch64 leg can never claim the macOS `_aarch64.dmg`.
const OS_SUFFIXES = {
  linux: /^\.(AppImage|deb|rpm|flatpak|tar\.gz)(\.sig)?$/,
  freebsd: /^\.tar\.gz(\.sig)?$/,
  macos: /^\.(dmg|app\.tar\.gz)(\.sig)?$/,
  windows: /^(-setup\.exe|\.msi|-portable\.zip)(\.sig)?$/,
};
// Extensions the launcher installs, plus the two Tauri updater payloads that ride along under the
// same naming (`.app.tar.gz` for macOS, `-setup.exe` for the NSIS installer). Neither of those
// two parses as installable in the launcher -- `.app.tar.gz` yields the arch "aarch64.app", and
// `.exe` is not a kind it knows -- which is the intended outcome: they exist for the updater.
const EXTENSIONS = [
  "msi",
  "zip",
  "dmg",
  "AppImage",
  "deb",
  "rpm",
  "flatpak",
  "tar.gz",
  "exe",
  "app.tar.gz",
];

function escapeRegExp(text) {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/**
 * The launcher-readable name for one Tauri-named release asset, or null when the asset is not
 * one this build leg produced (already renamed, latest.json, another leg's arch, ...).
 *
 * Tauri writes the macOS updater payload as `Redrob Recall.app.tar.gz` with no version or arch
 * in the name (bundle/macos/); that one is accepted for a macOS leg and named as if it carried
 * the leg's arch, the way tauri-action used to rename it on upload.
 *
 * @param {string} assetName name as the bundler wrote it or as GitHub lists it, e.g.
 *   "Redrob Recall_0.2.1_amd64.AppImage.sig" or "Redrob.Recall_0.2.1_amd64.AppImage.sig"
 * @param {string} version   release version without the leading "v"
 * @param {string} target    Rust target triple of the leg that built it
 */
export function launcherAssetName(assetName, version, target) {
  const leg = TARGET_TOKENS[target];
  if (!leg)
    throw new Error(
      `unknown release target ${target}; add it to TARGET_TOKENS`,
    );
  const os = leg.token.split("-")[0];
  const bare = assetName.match(/^Redrob[ .]Recall(\.app\.tar\.gz(?:\.sig)?)$/);
  if (bare && os === "macos") {
    return `${STEM}-${version}-${leg.token}${bare[1]}`;
  }
  // "Redrob Recall" locally, "Redrob.Recall" once GitHub has normalised the space.
  const match = assetName.match(
    new RegExp(
      `^Redrob[ .]Recall_${escapeRegExp(version)}_([A-Za-z0-9]+)(.*)$`,
    ),
  );
  if (!match) return null;
  const [, tauriArch, rest] = match;
  if (!leg.tauriArches.includes(tauriArch)) return null;
  // Drop the WiX language tag (`_en-US.msi`); keep `-setup.exe`, `.app.tar.gz`, `.sig`, ....
  const suffix = rest.replace(/^_[a-z]{2}-[A-Z]{2}(?=\.)/, "");
  if (!OS_SUFFIXES[os].test(suffix)) return null;
  return `${STEM}-${version}-${leg.token}${suffix}`;
}

const NAME_PATTERN = new RegExp(
  `^${escapeRegExp(STEM)}-(\\d[0-9A-Za-z.+-]*?)-([a-z]+)-([a-z0-9_]+)(-portable|-setup)?\\.(${EXTENSIONS.map(escapeRegExp).join("|")})(\\.sig)?$`,
);

/**
 * Parse a release asset name the way the launcher does. Returns null when the launcher would not
 * read it. `installable` is false for the updater-only shapes (.sig, .app.tar.gz, -setup.exe).
 */
export function parseLauncherAssetName(assetName) {
  const match = assetName.match(NAME_PATTERN);
  if (!match) return null;
  const [, version, os, arch, variant, ext, sig] = match;
  if (!LAUNCHER_TOKENS.has(`${os}-${arch}`)) return null;
  const installable = !sig && ext !== "exe" && ext !== "app.tar.gz";
  return {
    version,
    token: `${os}-${arch}`,
    variant: variant ?? "",
    ext,
    installable,
  };
}

/**
 * Every asset on the release must carry a launcher-readable name for this version, except the
 * updater manifest and the files the shared publish job adds (NON_INSTALLER_ASSETS). Returns the
 * list of problems; empty means the release is installable.
 *
 * @param {string[]} assetNames
 * @param {string} version
 * @param {string[]} [requiredTokens] os-arch tokens that must each have at least one installable
 *   asset -- the legs the release built, so a leg that produced only a .sig is caught.
 */
export function checkReleaseAssetNames(
  assetNames,
  version,
  requiredTokens = [],
) {
  const problems = [];
  const installableTokens = new Set();
  for (const name of assetNames) {
    if (NON_INSTALLER_ASSETS.has(name)) continue;
    const parsed = parseLauncherAssetName(name);
    if (!parsed) {
      problems.push(
        `${name}: not <stem>-<version>-<os>-<arch>[-portable].<ext>, Redrob Launcher cannot install it`,
      );
      continue;
    }
    if (parsed.version !== version) {
      problems.push(
        `${name}: version ${parsed.version} is not the release version ${version}`,
      );
      continue;
    }
    if (parsed.installable) installableTokens.add(parsed.token);
  }
  for (const token of requiredTokens) {
    if (!installableTokens.has(token)) {
      problems.push(`no installable asset for ${token}`);
    }
  }
  return problems;
}
