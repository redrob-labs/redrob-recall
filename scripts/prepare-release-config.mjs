import { mkdir, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

const publicKey = process.env.TAURI_UPDATER_PUBLIC_KEY?.trim();
if (!publicKey)
  throw new Error("TAURI_UPDATER_PUBLIC_KEY is required for a release build.");
const endpoint = "https://cdn.redrob.ai/vectordb/latest.json";

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

if (process.platform === "win32") {
  const thumbprint = process.env.WIN_CSC_CERTIFICATE_THUMBPRINT?.trim();
  if (!thumbprint) {
    throw new Error(
      "WIN_CSC_CERTIFICATE_THUMBPRINT is required for a Windows release.",
    );
  }
  config.bundle.windows = {
    certificateThumbprint: thumbprint,
    digestAlgorithm: "sha256",
    timestampUrl: "http://timestamp.digicert.com",
  };
}

const output = resolve("src-tauri/tauri.release.conf.json");
await mkdir(resolve("src-tauri"), { recursive: true });
await writeFile(output, `${JSON.stringify(config, null, 2)}\n`, {
  mode: 0o600,
});
console.log(`Prepared release configuration for ${process.platform}.`);
