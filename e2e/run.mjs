#!/usr/bin/env node
// End-to-end: the real desktop build, driven over W3C WebDriver through tauri-driver.
//
//   onboarding -> Choose folders -> indexing finishes -> search a word only one fixture has
//   -> that fixture is the result on screen
//
// Elements are reached with `execute/sync` (a script run in the page), not WebDriver element
// lookup. On WebKitWebDriver every lookup failed on this app: XPath answered "no such element" for a
// button present in the page source it returned, and CSS selectors -- quoted or not -- were rejected
// as "invalid selector: The string did not match the expected pattern". The script path is the same
// DOM the user sees, and screenshots still come from the driver.
//
// No test framework: WebdriverIO 9.32.0 carried 13 high-severity advisories through extract-zip that
// no override reaches, for what is a handful of HTTP calls.
//
// Needs: tauri-driver on WEBDRIVER_URL (default http://127.0.0.1:4444), an app built with
// `--features e2e` at APP, and REDROB_E2E_FOLDER pointing at the fixture library. Screenshots go to
// SHOTS (default e2e/screenshots).
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

let session;
/** Run `body` in the page with `args`; returns its JSON-serialisable result. */
const js = (body, ...args) =>
  wd("POST", `/session/${session}/execute/sync`, { script: body, args });

/** Text of the element with this data-testid, or null when it is not on screen. */
const text = (id) =>
  js(
    "const el = document.querySelector('[data-testid=' + arguments[0] + ']'); return el ? el.textContent : null;",
    id,
  );
/** Click the element with this data-testid; false when it is not on screen. */
const click = (id) =>
  js(
    "const el = document.querySelector('[data-testid=' + arguments[0] + ']'); if (!el || el.disabled) return false; el.click(); return true;",
    id,
  );
/** Type into a React-controlled field: set through the native setter, then fire `input`. */
const fill = (id, value) =>
  js(
    `const el = document.querySelector('[data-testid=' + arguments[0] + ']');
     if (!el) return false;
     const proto = el instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
     Object.getOwnPropertyDescriptor(proto, 'value').set.call(el, arguments[1]);
     el.dispatchEvent(new Event('input', { bubbles: true }));
     return true;`,
    id,
    value,
  );

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

  await until(
    "the onboarding screen",
    async () => (await text("choose-folders")) !== null,
  );
  await shot("1-onboarding");
  await until("Choose folders to accept a click", () =>
    click("choose-folders"),
  );

  // The main shell replaces onboarding once a folder is saved.
  await until(
    "the main shell",
    async () => (await text("search-input")) !== null,
  );
  // Indexing is finished when the sidebar counts the six fixtures and its status is idle.
  await until(
    "indexing to finish",
    () =>
      js(
        "const c = document.querySelector('[data-testid=library-count]'); const s = document.querySelector('[data-testid=library-status]'); return !!c && !!s && c.textContent.trim() === '6 files' && s.dataset.status === 'idle';",
      ),
    300,
  );
  await shot("2-indexed");

  // "saffron" is in minutes.docx and in no other fixture.
  await until("the search box to take input", () =>
    fill("search-input", "saffron"),
  );
  await until("Search to accept a click", () => click("search-submit"));
  const first = await until("a search result", () =>
    js(
      "const el = document.querySelector('[data-testid=result-card]'); return el ? el.textContent : null;",
    ),
  );
  await shot("3-results");
  if (!first.includes("minutes.docx"))
    throw new Error(`first result is not minutes.docx: ${first}`);
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
