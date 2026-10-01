#!/usr/bin/env node
// Fail if a built binary contains the end-to-end test hook.
//
// The `e2e` cargo feature lets a WebDriver test supply the folder that the native picker would
// otherwise ask for (src-tauri/src/commands.rs, choose_library_folder). A shipped app with that hook
// would take its library folder from an environment variable, so a release must prove the hook is
// absent from the bytes it ships -- not that a build flag was probably left off.
//
// Usage: node scripts/check-e2e-feature.mjs <binary> [--expect-present]
//   --expect-present inverts the check; the e2e workflow uses it to prove the probe can see the hook.
import { readFileSync } from "node:fs";

const MARKER = "REDROB_E2E_FOLDER";
const [binary, flag] = process.argv.slice(2);
if (!binary) {
  console.error("usage: check-e2e-feature.mjs <binary> [--expect-present]");
  process.exit(2);
}
const present = readFileSync(binary).includes(Buffer.from(MARKER));
const expectPresent = flag === "--expect-present";
if (present !== expectPresent) {
  console.error(
    present
      ? `error: ${binary} contains the e2e test hook (${MARKER}); it must not ship`
      : `error: ${binary} lacks the e2e test hook, so this probe cannot see it`,
  );
  process.exit(1);
}
console.log(
  `${binary}: e2e test hook ${present ? "present" : "absent"}, as expected`,
);
