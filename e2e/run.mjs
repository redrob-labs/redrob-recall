#!/usr/bin/env node
// End-to-end: the real desktop build, driven over W3C WebDriver through tauri-driver.
//
//   onboarding -> Choose folders -> indexing finishes -> search a word only one fixture has
//   -> that fixture is the result on screen
//
// No test framework. WebdriverIO was tried first and brings 13 high-severity advisories through
// extract-zip that no override reaches; the W3C protocol needs a handful of HTTP calls, so this
// speaks it directly with fetch.
//
// Needs: tauri-driver listening on WEBDRIVER_URL (default http://127.0.0.1:4444), an app binary
// built with `--features e2e` at APP, and REDROB_E2E_FOLDER pointing at the fixture library (the
// e2e feature makes Choose folders answer with it instead of opening the native picker).
// Writes screenshots to SHOTS (default e2e/screenshots).
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const DRIVER = process.env.WEBDRIVER_URL ?? "http://127.0.0.1:4444";
const APP = process.env.APP;
const SHOTS = process.env.SHOTS ?? "e2e/screenshots";
if (!APP || !process.env.REDROB_E2E_FOLDER) {
  console.error("APP and REDROB_E2E_FOLDER must be set");
  process.exit(2);
}
mkdirSync(SHOTS, { recursive: true });

async function wd(method, path, body) {
  const response = await fetch(`${DRIVER}${path}`, {
    method,
    headers: { "content-type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const json = await response.json();
  if (!response.ok || json.value?.error) {
    throw new Error(
      `${method} ${path}: ${json.value?.error} ${json.value?.message ?? ""}`,
    );
  }
  return json.value;
}

const ELEMENT = "element-6066-11e4-a52f-4ce07a7cf2a8";
let session;
const find = async (xpath) =>
  (
    await wd("POST", `/session/${session}/element`, {
      using: "xpath",
      value: xpath,
    })
  )[ELEMENT];
const findAll = async (xpath) =>
  (
    await wd("POST", `/session/${session}/elements`, {
      using: "xpath",
      value: xpath,
    })
  ).map((e) => e[ELEMENT]);
const click = (id) => wd("POST", `/session/${session}/element/${id}/click`, {});
const type = (id, text) =>
  wd("POST", `/session/${session}/element/${id}/value`, { text });
const textOf = (id) => wd("GET", `/session/${session}/element/${id}/text`);
async function shot(name) {
  const png = await wd("GET", `/session/${session}/screenshot`);
  writeFileSync(join(SHOTS, `${name}.png`), Buffer.from(png, "base64"));
}
async function until(what, check, seconds = 120) {
  const deadline = Date.now() + seconds * 1000;
  let last = "the check returned nothing";
  for (;;) {
    try {
      const value = await check();
      if (value) return value;
    } catch (error) {
      // Kept, because "timed out" alone hid the reason on the first CI run.
      last = error.message;
    }
    if (Date.now() > deadline)
      throw new Error(`timed out waiting for ${what}; last: ${last}`);
    await new Promise((resolve) => setTimeout(resolve, 500));
  }
}

let failed = false;
try {
  session = (
    await wd("POST", "/session", {
      capabilities: { alwaysMatch: { "tauri:options": { application: APP } } },
    })
  ).sessionId;

  const choose = await until("the onboarding screen", () =>
    find("//button[.//text()[contains(., 'Choose folders')]]"),
  );
  await shot("1-onboarding");
  await click(choose);

  // The main shell replaces onboarding once a folder is saved.
  await until("the main shell", () =>
    find("//textarea[@placeholder='Describe what you remember…']"),
  );
  // Indexing is finished when the sidebar shows the six fixtures and its status reads ready
  // (CSS uppercases it, so compare case-insensitively).
  await until(
    "indexing to finish",
    async () => {
      const count = await find("//strong[normalize-space(.)='6 files']");
      const status = await find(
        "//*[contains(translate(normalize-space(.), 'LIBRAYDE', 'librayde'), 'library ready')][not(*)]",
      );
      return count && status;
    },
    300,
  );
  await shot("2-indexed");

  const box = await find(
    "//textarea[@placeholder='Describe what you remember…']",
  );
  // "saffron" is in minutes.docx and in no other fixture.
  await type(box, "saffron");
  await click(
    await find(
      "//button[contains(@class, 'search-submit') and @aria-label='Search']",
    ),
  );
  const first = await until("a search result", async () => {
    const cards = await findAll("//*[contains(@class, 'result-card')]");
    return cards.length ? cards[0] : false;
  });
  await shot("3-results");
  const text = await textOf(first);
  if (!text.includes("minutes.docx"))
    throw new Error(`first result is not minutes.docx: ${text}`);
  console.log(
    "e2e: onboarding -> folder -> indexed 6 files -> 'saffron' -> minutes.docx",
  );
} catch (error) {
  failed = true;
  console.error(`e2e failed: ${error.message}`);
  if (session) {
    await shot("failure").catch(() => {});
    const source = await wd("GET", `/session/${session}/source`).catch(
      (e) => `source unavailable: ${e.message}`,
    );
    writeFileSync(join(SHOTS, "failure.html"), String(source));
  }
} finally {
  if (session) await wd("DELETE", `/session/${session}`).catch(() => {});
}
process.exit(failed ? 1 : 0);
