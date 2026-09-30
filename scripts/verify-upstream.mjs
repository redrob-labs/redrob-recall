#!/usr/bin/env node
//
// Verifies the upstream pins in docs/upstream-sources.toml.
//
// The same invariants redrob-canvas and redrob-query check, with ONE deliberate difference: the
// inbound licence rule is inverted, because this product is Apache-2.0 rather than GPL. See
// INBOUND_OK below. Three products read from upstream authorities; if each invents its own vocabulary and
// its own checks, the one that is weakest is the one that ships a licence mistake.
//
// The checks, and why each exists rather than being left to review:
//
//   * `kind` is present and recognised. It is what separates source we COPY (upstream's licence
//     binds our distribution) from source we only READ (it does not). Getting that wrong is the one
//     mistake in this file that cannot be walked back after shipping.
//   * a `code` source has an attribution section in UPSTREAM_NOTICES.md, and that section records
//     its pinned commit. A pin bumped without touching the notices leaves the notices describing a
//     tree nobody has.
//   * a `code` source declares a `boundary`. If you copy from a project you must say where copying
//     stops, because the interesting failures are at the edge -- a renderer that fights ours, or a
//     directory under a licence we may not use.
//   * a `code` licence is recognised as inbound-compatible. Widening that list is a deliberate act.
//
// No network access. Commit reachability is a separate concern from pin well-formedness, and this
// runs in `npm run check` on every change.

import { readFileSync, existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const pinsPath = join(root, "docs/upstream-sources.toml");
const noticesPath = join(root, "UPSTREAM_NOTICES.md");

const KINDS = ["code", "algorithm", "library", "protocol"];
// PERMISSIVE ONLY, and this is where this file deliberately differs from redrob-query's otherwise
// identical validator. That product is GPL-3.0-or-later and can absorb copyleft; this one is
// Apache-2.0 and cannot. Copying GPL or LGPL source in here would force this whole product to GPL --
// a one-way door redrob-query walked through on purpose for Beekeeper Studio, and one there is no
// reason to walk through here. So it is refused rather than discouraged.
//
// Copying query's regex across unchanged would have silently permitted exactly that, which is the
// single most likely way this check gets broken later. If a copyleft source ever genuinely needs to
// come in, the relicense is the decision and widening this list is its consequence -- in that order.
const INBOUND_OK = /Apache-2\.0|MIT|BSD-2-Clause|BSD-3-Clause|ISC|Zlib|Unlicense|CC0/;
const COPYLEFT = /GPL|AGPL|LGPL|MPL|SSPL|EUPL|CDDL|OSL/;

const problems = [];

if (!existsSync(pinsPath)) {
  console.error("error: docs/upstream-sources.toml is missing");
  process.exit(1);
}
const pins = readFileSync(pinsPath, "utf8");
const notices = existsSync(noticesPath) ? readFileSync(noticesPath, "utf8") : "";
if (!notices) problems.push("UPSTREAM_NOTICES.md is missing");

// A deliberately small TOML reader: section headers and `key = "value"` lines. The file is ours and
// its shape is fixed, so a dependency for this would be a supply-chain surface bought for nothing.
const sections = new Map();
let current = null;
for (const line of pins.split("\n")) {
  const header = /^\[(\w+)\]\s*$/.exec(line);
  if (header) {
    current = header[1];
    sections.set(current, {});
    continue;
  }
  if (!current) continue;
  const pair = /^(\w+)\s*=\s*"([^"]*)"\s*$/.exec(line);
  if (pair) sections.get(current)[pair[1]] = pair[2];
}

const required = ["bloop"];
for (const name of required) {
  if (!sections.has(name)) problems.push(`[${name}] section is missing`);
}

for (const [name, body] of sections) {
  if (!body.repository) {
    problems.push(`${name}.repository is missing`);
  } else if (!body.repository.startsWith("https://")) {
    problems.push(`${name}.repository is not https: ${JSON.stringify(body.repository)}`);
  }

  // A pin is either an exact commit or a version floor. `code` and `algorithm` need the commit:
  // copying from a moving target is unreproducible, and so is comparing behaviour against "1.12 or
  // later". A `library` is legitimately pinned by `minimum_version` -- we link whatever the system or
  // registry provides at or above that floor, and no single commit describes it.
  const exact = body.kind === "code" || body.kind === "algorithm";
  if (body.commit) {
    if (!/^[0-9a-f]{40}$/.test(body.commit)) {
      problems.push(`${name}.commit is not a full 40-character sha: ${JSON.stringify(body.commit)}`);
    }
  } else if (exact) {
    problems.push(`${name}.kind is ${JSON.stringify(body.kind)} so it must pin an exact commit`);
  } else if (!body.minimum_version) {
    problems.push(`${name} pins neither a commit nor a minimum_version`);
  }

  if (body.kind === undefined) {
    problems.push(`${name} declares no kind; must be one of ${KINDS.join(", ")}`);
  } else if (!KINDS.includes(body.kind)) {
    problems.push(`${name}.kind is ${JSON.stringify(body.kind)}, not one of ${KINDS.join(", ")}`);
  }

  // A protocol is spoken, not licensed to us, so it needs no licence line. Everything else does.
  if (body.kind !== "protocol" && !body.license) {
    problems.push(`${name} declares no license`);
  }

  if (body.kind !== "code") continue;

  const heading = new RegExp(`^##\\s+${name}\\s*$`, "im");
  if (!heading.test(notices)) {
    problems.push(`${name}.kind is 'code' but UPSTREAM_NOTICES.md has no '## ${name}' section`);
  }
  if (body.commit && !notices.includes(body.commit)) {
    problems.push(
      `${name} is pinned at ${body.commit.slice(0, 12)} but UPSTREAM_NOTICES.md does not record ` +
        `that commit`,
    );
  }
  if (!body.boundary) {
    problems.push(
      `${name}.kind is 'code' but it declares no boundary; say where copying stops`,
    );
  }
  if (body.license && COPYLEFT.test(body.license)) {
    problems.push(
      `${name}.kind is 'code' and its license ${JSON.stringify(body.license)} is copyleft; this ` +
        `product is Apache-2.0 and copying that source in would force the whole product to change ` +
        `licence. Relicense deliberately first, or take the upstream as 'algorithm' instead`,
    );
  } else if (body.license && !INBOUND_OK.test(body.license)) {
    problems.push(
      `${name}.kind is 'code' but its license ${JSON.stringify(body.license)} is not recognised ` +
        `as a permissive licence an Apache-2.0 product may absorb; audit it by hand and widen this ` +
        `check deliberately`,
    );
  }
}

if (problems.length > 0) {
  for (const p of problems) console.error(`error: ${p}`);
  process.exit(1);
}

// The compatibility matrix names an Authority per row, and a row citing an upstream nobody
// registered is how a roadmap starts describing work against a tree with no pin.
//
// The check reads the other way round than is tempting. Scanning the column for REGISTERED names and
// complaining about what is missing catches nothing -- an unregistered name is, by construction, not
// in the list being searched for, so the check passes on exactly the input it exists to reject. (That
// is how it was written first; breaking the file on purpose is what exposed it.) So instead every row
// must match a registered upstream OR one of these explicitly exempt authorities, and anything else
// fails by name.
const EXEMPT_AUTHORITY = [
  /redrob-recall bounded scope/i, // our own scope, not an upstream gap
  /none yet chosen/i, // an honest blank; becomes a real authority when one is picked
  /Redrob design system/i, // internal delivery
  /Redrob Console/i, // our own service
  /redrob.code/i, // sibling product; its API doc is the authority
  /fastembed|ONNX Runtime/i, // a Cargo dependency, inventoried in THIRD_PARTY_NOTICES.md
];
const matrixPath = join(root, "docs/compatibility.md");
if (existsSync(matrixPath)) {
  const matrix = readFileSync(matrixPath, "utf8");
  const rows = matrix
    .split("\n")
    .filter((l) => l.startsWith("| ") && l.split("|").length > 5)
    .filter((l) => !/^\|\s*-+\s*\|/.test(l) && !/\|\s*Authority\s*\|/.test(l));

  const unknown = [];
  for (const row of rows) {
    const cells = row.split("|");
    const authority = cells[3] ?? "";
    const registered = [...sections.keys()].some((n) =>
      new RegExp(`\\b${n}\\b`, "i").test(authority),
    );
    if (registered || EXEMPT_AUTHORITY.some((re) => re.test(authority))) continue;
    unknown.push(`${(cells[2] ?? "").trim().slice(0, 48)} -> ${authority.trim().slice(0, 40)}`);
  }
  if (unknown.length > 0) {
    console.error(
      "error: compatibility.md rows name an authority that is neither a registered upstream in " +
        "docs/upstream-sources.toml nor an exempt one:",
    );
    for (const u of unknown) console.error(`  ${u}`);
    process.exit(1);
  }

  // No row may claim parity before a golden-output check exists to justify it. `native` means we
  // have an implementation; `parity` means we matched the authority, and nothing here has ever run
  // such a check. This is what stops the word drifting into the matrix by optimism.
  const parity = rows.filter((r) => /\|\s*parity\s*\|?\s*$/.test(r));
  if (parity.length > 0) {
    console.error(
      `error: ${parity.length} row(s) in compatibility.md claim 'parity', but no golden-output ` +
        `harness exists in this repository yet. Build the harness first, or use 'native'.`,
    );
    process.exit(1);
  }
  console.log(
    `compatibility matrix: ${rows.length} rows, every authority accounted for, 0 parity claims`,
  );
}

const byKind = (k) =>
  [...sections]
    .filter(([, b]) => b.kind === k)
    .map(([n]) => n)
    .sort()
    .join(", ") || "none";

console.log(`upstream pins well-formed: ${sections.size} sources`);
console.log(`  copied source (licence binds us): ${byKind("code")}`);
console.log(`  behaviour only (nothing copied):  ${byKind("algorithm")}`);
