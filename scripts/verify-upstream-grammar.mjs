#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0
//
// Check this product's search grammar against bloop's, read from the pinned upstream.
//
// WHY. `src-tauri/src/query.rs` says its grammar "is a reduced version of bloop's", and
// UPSTREAM_NOTICES.md lists which constructs were dropped and why. Nothing verified either statement. A
// reduction claim is checkable: every construct in bloop's `grammar.pest` either survives here in some
// form or was dropped for a stated reason, and nothing may be unaccounted for.
//
// It also checks the direction nobody thinks to check. Measured while writing this: bloop's grammar has
// NO NEGATION AT ALL -- no `-term`, no `NOT`. So this product's exclusions are an ADDITION, not a
// reduction, and so is the trailing-`*` prefix match. Describing the whole grammar as "bloop's, reduced"
// understates what is ours and overstates what is theirs.
//
// Not part of `npm test`: it needs a bloop checkout at the pinned commit, and a check that passes when
// its subject is absent is worse than no check.
//
// Usage:
//   REDROB_BLOOP_CHECKOUT=/path/to/bloop node scripts/verify-upstream-grammar.mjs

import { readFileSync, existsSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { join } from 'node:path';

const fail = (message) => {
  console.error(message);
  process.exit(1);
};

const checkout = process.env.REDROB_BLOOP_CHECKOUT;
if (!checkout) {
  fail(
    'REDROB_BLOOP_CHECKOUT is not set, so there is no grammar to compare against and nothing would be\n' +
      'verified. This is a FAILURE, not a skip. Fetch recipe: docs/upstream-grammar.md',
  );
}
if (!existsSync(checkout)) fail(`no bloop checkout at ${checkout}`);

const pinned = (readFileSync('docs/upstream-sources.toml', 'utf8').match(
  /\[bloop\][\s\S]*?commit = "([0-9a-f]{40})"/,
) ?? [])[1];
if (!pinned) fail('could not read bloop\u2019s pinned commit from docs/upstream-sources.toml');
let head;
try {
  head = execFileSync('git', ['-C', checkout, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
} catch {
  fail(`${checkout} is not a git checkout, so its commit cannot be established`);
}
if (head !== pinned) {
  fail(
    `the checkout is at ${head}, not the pinned ${pinned}.\n` +
      'A grammar read from the wrong commit says nothing about what was reduced.',
  );
}

const queryDir = join(checkout, 'server/bleep/src/query');
const grammarPath = join(queryDir, 'grammar.pest');
const stopwordsPath = join(queryDir, 'stopwords.txt');
for (const path of [grammarPath, stopwordsPath]) {
  if (!existsSync(path)) fail(`${path} is not in the checkout; the sparse paths are wrong`);
}
const grammar = readFileSync(grammarPath, 'utf8');
const ours = readFileSync('src-tauri/src/query.rs', 'utf8');

// --------------------------------------------------------------- the classification
//
// Every construct bloop's grammar declares, and what became of it here. `kept` means this product has the
// same construct, possibly with different field names, because its domain is documents rather than git
// repositories. `dropped` means it is deliberately absent and the reason is recorded.
const disposition = {
  // Labels.
  content: { state: 'kept', note: 'a field-scoped term; our fields are content, heading, name, path' },
  path: { state: 'kept', note: 'documents have paths, and documents_fts indexes them' },
  repo: { state: 'dropped', note: 'no git repositories; this searches a folder of documents' },
  org: { state: 'dropped', note: 'no git forge' },
  branch: { state: 'dropped', note: 'no git repositories' },
  symbol: { state: 'dropped', note: 'no code symbols; this indexes prose' },
  lang: { state: 'dropped', note: 'no source language to filter by' },
  // Modes.
  case: { state: 'dropped', note: 'FTS5 matching is case-insensitive; there is nothing to switch' },
  open: { state: 'dropped', note: 'no editor state to scope a search to' },
  global_regex: {
    state: 'dropped',
    note: 'FTS5 cannot walk a term dictionary, so a regex cannot be answered by the index',
  },
  // Literal forms.
  quoted_literal: { state: 'kept', note: 'Node::Phrase, a run of words that must appear in order' },
  unquoted_literal: { state: 'kept', note: 'Node::Term' },
  single_quoted_literal: {
    state: 'dropped',
    note: "one quoting form is enough for prose; ' appears in ordinary English words",
  },
  regex_quoted_literal: { state: 'dropped', note: 'same reason as global_regex' },
  // Structure.
  intersection: { state: 'kept', note: 'Node::All, implicit AND between adjacent terms' },
  or: { state: 'kept', note: 'Node::Any' },
  group: { state: 'kept', note: 'brackets, with EmptyGroup and UnclosedGroup as typed errors' },
  escape: { state: 'dropped', note: 'no escape character; a quote is closed or the query is refused' },
  // Rules that are grammar plumbing rather than user-visible constructs.
  query: { state: 'plumbing' },
  element: { state: 'plumbing' },
  literal: { state: 'plumbing' },
  label: { state: 'plumbing' },
  mode: { state: 'plumbing' },
  quote: { state: 'plumbing' },
  single_quote: { state: 'plumbing' },
  regex_quote: { state: 'plumbing' },
  terminator: { state: 'plumbing' },
  group_start: { state: 'plumbing' },
  group_end: { state: 'plumbing' },
  boolean: { state: 'plumbing' },
  case_ignore: { state: 'plumbing' },
  case_sensitive: { state: 'plumbing' },
  WHITESPACE: { state: 'plumbing' },
  raw_text: { state: 'plumbing', note: "bloop's separate natural-language entry point" },
  nl_query: { state: 'plumbing', note: 'the same; this product has one grammar, not two' },
};

// Additions: ours, not bloop's. Stated explicitly so "reduced version of bloop's" cannot be read as
// covering the whole grammar.
const additions = [
  {
    name: 'exclusions',
    evidence: /exclude/,
    note: "bloop's grammar has no negation of any kind -- no -term, no NOT",
  },
  {
    name: 'prefix match',
    evidence: /prefix/,
    note: 'a trailing * is an FTS5 feature bloop does not expose in its grammar',
  },
  {
    name: 'heading field',
    evidence: /heading/,
    note: 'prose has headings; bloop has no such label',
  },
];

// --------------------------------------------------------------- checks
const problems = [];

// 1. Completeness. Every rule bloop declares must be classified, or a construct could be silently
//    unconsidered -- and a claim of reduction is only meaningful if the set being reduced is known.
const declared = [...grammar.matchAll(/^([A-Za-z_]+)\s*=/gm)].map((match) => match[1]);
const unique = [...new Set(declared)];
const unclassified = unique.filter((rule) => !(rule in disposition));
if (unclassified.length) {
  problems.push(`bloop grammar rules with no recorded disposition: ${unclassified.join(', ')}`);
}
// And the reverse: a disposition for a rule bloop does not have is a stale record.
const stale = Object.keys(disposition).filter((rule) => !unique.includes(rule));
if (stale.length) {
  problems.push(`recorded dispositions for rules bloop no longer declares: ${stale.join(', ')}`);
}

// 2. bloop really has no negation. If a future pin adds one, this product's exclusions stop being an
//    addition and the note above becomes wrong.
if (/\bnot\b|"-"|negat/i.test(grammar)) {
  problems.push('bloop\u2019s grammar now mentions negation; the "exclusions are ours" note is stale');
}

// 3. Each addition must actually be present here, or we are claiming a feature we do not have.
for (const { name, evidence } of additions) {
  if (!evidence.test(ours)) problems.push(`claimed addition ${name} is not present in query.rs`);
}

// 4. Every stopword this product drops must be one bloop drops too. Ours is a short list for a prose
//    index; if one were NOT in bloop's, the list would be our own invention rather than a reduction.
const bloopStopwords = new Set(
  readFileSync(stopwordsPath, 'utf8')
    .split('\n')
    .map((word) => word.trim().toLowerCase())
    .filter(Boolean),
);
const block = ours.match(/const LOOSE_STOPWORDS: &\[&str\] = &\[([\s\S]*?)\];/);
if (!block) problems.push('could not find LOOSE_STOPWORDS in query.rs');
const ourStopwords = block
  ? [...block[1].matchAll(/"([^"]+)"/g)].map((match) => match[1].toLowerCase())
  : [];
if (!ourStopwords.length) problems.push('LOOSE_STOPWORDS parsed as empty, so nothing would be compared');
const notInBloop = ourStopwords.filter((word) => !bloopStopwords.has(word));
if (notInBloop.length) {
  problems.push(`stopwords we drop that bloop does not: ${notInBloop.join(', ')}`);
}

// --------------------------------------------------------------- report
const counts = { kept: 0, dropped: 0, plumbing: 0 };
for (const { state } of Object.values(disposition)) counts[state] += 1;

console.log(`bloop grammar read at pin ${pinned}\n`);
console.log(`${unique.length} rules declared; ${counts.kept} kept, ${counts.dropped} dropped, ${counts.plumbing} plumbing\n`);
for (const [rule, { state, note }] of Object.entries(disposition)) {
  if (state === 'plumbing') continue;
  console.log(`${state.padEnd(8)} ${rule.padEnd(22)} ${note}`);
}
console.log('\nours, not bloop\u2019s:');
for (const { name, note } of additions) console.log(`  ${name.padEnd(14)} ${note}`);
console.log(`\nstopwords: ${ourStopwords.length} of ours, all ${ourStopwords.length - notInBloop.length} present in bloop\u2019s ${bloopStopwords.size}`);

if (problems.length) {
  console.error('\nproblems:');
  for (const problem of problems) console.error(`  ${problem}`);
  process.exit(1);
}
console.log('\ngrammar reduction verified against the pinned upstream');
