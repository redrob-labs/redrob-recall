import { access, mkdir, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

const publicKey = process.env.TAURI_UPDATER_PUBLIC_KEY?.trim();
if (!publicKey)
  throw new Error("TAURI_UPDATER_PUBLIC_KEY is required for a release build.");
// GitHub Releases IS the update channel -- there is no CDN in front of it.
//
// `releases/latest/download/<asset>` resolves to the newest published, non-prerelease
// release. The shared publish job publishes a tag's release as latest the moment every
// build leg has passed the updater-manifest job, which fails the run if latest.json is
// missing, names a platform that was built but has no signed entry, or points anywhere but
// this repository's own release assets. Without that guard the failure is silent: the app
// installs and simply never updates again.
const endpoint =
  "https://github.com/redrob-labs/redrob-recall/releases/latest/download/latest.json";

const config = {
  bundle: {
    createUpdaterArtifacts: true,
  },
  plugins: {
    updater: {
      pubkey: publicKey,
      endpoints: [endpoint],
      windows: { installMode: "passive" },
    },
  },
};

// A dry run (workflow_dispatch with a version) builds under that version so the asset names and
// latest.json carry it; a tag run passes the tag's version, which check-version.mjs has already
// matched against the manifests. Tauri's merged config takes this over tauri.conf.json.
const version = process.env.RELEASE_VERSION?.trim();
if (version) config.version = version;

if (process.platform === "win32") {
  // Authenticode through Azure Artifact Signing, with the credential azure/login left on the
  // runner: one shared copy of the recipe (redrob-labs/.github, checked out at .redrob-shared),
  // run by the bundler for the executable and for each installer. No certificate file, no
  // thumbprint, no secret in this configuration -- the three ARTIFACT_SIGNING_* variables are
  // read from the job environment by the script.
  const sharedDir = process.env.REDROB_SHARED_DIR?.trim() || ".redrob-shared";
  const signScript = resolve(sharedDir, "actions/sign-windows/sign.ps1");
  try {
    await access(signScript);
  } catch {
    throw new Error(
      `${signScript} is missing: check out redrob-labs/.github at ${sharedDir} before a Windows release build.`,
    );
  }
  config.bundle.windows = {
    signCommand: {
      cmd: "pwsh",
      args: ["-NoProfile", "-File", signScript, "%1"],
    },
  };
}

const output = resolve("src-tauri/tauri.release.conf.json");
await mkdir(resolve("src-tauri"), { recursive: true });
await writeFile(output, `${JSON.stringify(config, null, 2)}\n`, {
  mode: 0o600,
});
console.log(
  `Prepared release configuration for ${process.platform}${version ? ` (version ${version})` : ""}.`,
);
