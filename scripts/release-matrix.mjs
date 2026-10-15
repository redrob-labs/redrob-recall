// Emit the release build matrix, and the updater platform keys, launcher tokens and asset
// suffixes that must therefore appear on the release, from ONE list.
//
// The updater-manifest job used to hardcode the platform keys it required, so adding or removing
// a leg meant editing two places and getting a correct gate to fail for the wrong reason. Now a
// platform that is deliberately not built is not demanded, and one that is built cannot be
// quietly missing from the manifest.
//
// Windows is signed through Azure Artifact Signing with the job's OpenID Connect login (the shared
// redrob-labs sign-windows recipe), so there is no certificate to wait for and the leg always
// builds. On a tag the workflow refuses to run the leg unsigned; see release.yml.
//
// macOS Intel is absent entirely, and that is a product decision rather than a gap to fill:
// this product embeds ONNX Runtime through fastembed, and `ort-sys` publishes no prebuilt
// binary for x86_64-apple-darwin -- the release build failed with
//   error: ort-sys@2.0.0-rc.13: no prebuilt binaries available for target x86_64-apple-darwin
// after compiling the whole dependency tree. Supporting Intel Macs would mean building and
// vendoring libonnxruntime ourselves for a platform Apple no longer sells. If that changes,
// add the leg back here and darwin-x86_64 flows through to the updater contract automatically.

import { appendFileSync } from "node:fs";
import { TARGET_TOKENS } from "./release-asset-names.mjs";

const legs = [
  {
    label: "Linux x64",
    platform: "ubuntu-24.04",
    target: "x86_64-unknown-linux-gnu",
    bundles: "appimage,deb",
  },
  {
    label: "macOS Apple Silicon",
    platform: "macos-15",
    target: "aarch64-apple-darwin",
    bundles: "app,dmg",
  },
  {
    label: "Windows x64",
    platform: "windows-2025",
    target: "x86_64-pc-windows-msvc",
    bundles: "nsis,msi",
  },
];

// Which file each bundle type leaves on the release.
const bundleAssetSuffixes = {
  appimage: [".AppImage"],
  deb: [".deb"],
  dmg: [".dmg"],
  // The .app bundle ships as the updater payload, which the signature check already covers.
  app: [],
  nsis: [".exe"],
  msi: [".msi"],
};

const matrix = legs.map(({ label, platform, target, bundles }) => ({
  label,
  platform,
  target,
  bundles,
}));
const tokens = legs.map((leg) => {
  const entry = TARGET_TOKENS[leg.target];
  if (!entry) {
    console.error(
      `No Redrob Launcher token for target ${leg.target}; add it to release-asset-names.mjs`,
    );
    process.exit(1);
  }
  return entry;
});
// The updater key each leg must write into latest.json, and the os-arch token Redrob Launcher
// expects in each leg's asset names; the updater-manifest job requires both for every leg.
const platforms = tokens.map((entry) => entry.updater);
const launcherTokens = tokens.map((entry) => entry.token);
const assetSuffixes = [
  ...new Set(
    legs.flatMap((leg) =>
      leg.bundles.split(",").flatMap((bundle) => {
        const suffixes = bundleAssetSuffixes[bundle.trim()];
        if (suffixes === undefined) {
          console.error(
            `Unknown bundle type "${bundle.trim()}" on ${leg.label}`,
          );
          process.exit(1);
        }
        return suffixes;
      }),
    ),
  ),
];

console.log(`Building: ${legs.map((leg) => leg.label).join(", ")}`);
console.log(
  `Updater platforms required on the release: ${platforms.join(" ")}`,
);
console.log(
  `Asset suffixes required on the release: ${assetSuffixes.join(" ")}`,
);
console.log(
  `Redrob Launcher tokens required on the release: ${launcherTokens.join(" ")}`,
);

const output = process.env.GITHUB_OUTPUT;
if (output) {
  appendFileSync(output, `matrix=${JSON.stringify(matrix)}\n`);
  appendFileSync(output, `platforms=${platforms.join(" ")}\n`);
  appendFileSync(output, `asset_suffixes=${assetSuffixes.join(" ")}\n`);
  appendFileSync(output, `launcher_tokens=${launcherTokens.join(" ")}\n`);
}
