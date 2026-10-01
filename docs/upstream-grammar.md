# Grammar reduction harness

Stage 2, fourth harness. For the search query language in `src-tauri/src/query.rs`.

## The problem it fixes

`query.rs` says its grammar "is a reduced version of bloop's", and `UPSTREAM_NOTICES.md` lists the dropped
constructs and why. **Nothing verified either statement.** A reduction claim is checkable: every construct in
bloop's `grammar.pest` either survives here in some form or was dropped for a stated reason, and none may be
unaccounted for.

It also checks the direction nobody thinks to check. Measured while writing it: **bloop's grammar has no
negation at all** — no `-term`, no `NOT`. So this product's exclusions are an **addition**, not a reduction, and
so are the trailing-`*` prefix match and the `heading` field. Describing the whole grammar as "bloop's, reduced"
understated what is ours and overstated what is theirs.

## What it reports

35 rules declared at the pin: 7 kept, 11 dropped, 17 grammar plumbing.

| Kept | As |
|---|---|
| `content` | a field-scoped term; our fields are content, heading, name, path |
| `path` | documents have paths, and `documents_fts` indexes them |
| `quoted_literal` | `Node::Phrase` |
| `unquoted_literal` | `Node::Term` |
| `intersection` | `Node::All`, implicit AND |
| `or` | `Node::Any` |
| `group` | brackets, with `EmptyGroup` and `UnclosedGroup` as typed errors |

| Dropped | Why |
|---|---|
| `repo`, `org`, `branch` | no git repositories; this searches a folder of documents |
| `symbol`, `lang` | no code symbols and no source language; this indexes prose |
| `case` | FTS5 matching is case-insensitive, so there is nothing to switch |
| `open` | no editor state to scope a search to |
| `global_regex`, `regex_quoted_literal` | FTS5 cannot walk a term dictionary, so a regex cannot be answered by the index |
| `single_quoted_literal` | one quoting form is enough for prose, and `'` appears in ordinary English words |
| `escape` | no escape character; a quote is closed or the query is refused |

| Ours, not bloop's | |
|---|---|
| exclusions | bloop's grammar has no negation of any kind |
| prefix match | a trailing `*` is an FTS5 feature bloop does not expose |
| `heading` field | prose has headings; bloop has no such label |

Stopwords: 27 of ours, **all 27 present in bloop's 570**. That is the check that makes the list a reduction
rather than our own invention — one absent from bloop's would mean we chose it ourselves.

## The four checks

1. **Completeness.** Every rule `grammar.pest` declares must carry a recorded disposition, and every recorded
   disposition must name a rule bloop actually declares. A reduction claim is only meaningful if the set being
   reduced is known, and a stale record is as misleading as a missing one.
2. **bloop still has no negation.** If a future pin adds one, the "exclusions are ours" note becomes wrong and
   the run fails rather than carrying a stale claim.
3. **Each claimed addition is really present** in `query.rs`, so we cannot claim a feature we do not have.
4. **Every stopword we drop is one bloop drops too.**

## Running it

```bash
mkdir -p "$KIROCREW_SCRATCH/bloop" && cd "$KIROCREW_SCRATCH/bloop"
git init -q && git remote add origin https://github.com/BloopAI/bloop
git sparse-checkout init --cone && git sparse-checkout set server/bleep/src/query
git fetch -q --depth 1 origin 431e9e82c5a293c40f22aadb3615c3aa387af82e && git checkout -q FETCH_HEAD

cd ~/workplace/redrob-recall
REDROB_BLOOP_CHECKOUT="$KIROCREW_SCRATCH/bloop" node scripts/verify-upstream-grammar.mjs
```

13 MB. The upstream is archived, last commit 2024-12-04, so the pin cannot move under us.

## It never passes silently

Reverse-verified, all three behaving as they must:

| Provoked | Result |
|---|---|
| `REDROB_BLOOP_CHECKOUT` unset | `FAILURE, not a skip`, exit 1 |
| checkout at the wrong commit | exit 1 — "a grammar read from the wrong commit says nothing about what was reduced" |
| a stopword added that bloop does not drop | named in the output, exit 1 |

## The classification this cycle also corrected

`docs/upstream-sources.toml` declared bloop as `kind = "code"` while `UPSTREAM_NOTICES.md` said, correctly, that
**nothing is copied** and the subsystem table is empty. Those two statements cannot both be true, and the
registry's was the wrong one.

bloop is now `kind = "translated"`, a kind added to `scripts/verify-upstream.mjs` for this: validated exactly
like `code` — attribution, an exact pin, a declared boundary, an inbound-compatible licence — but separate,
because for `code` the re-sync method is a diff against the upstream file and for `translated` there is no file
to diff. `redrob-canvas` made the same distinction for its Krita work; this is the same reasoning in a repository
whose licence direction is reversed, so the copyleft refusal still applies (this product is Apache-2.0 and must
not absorb copyleft source).

The kind is also printed in the script's summary. Without that, bloop — the one source the licence reasoning
depends on — would have vanished from the report the moment it changed kind, which is exactly the reporting gap
that once hid a `qdrant` entry naming the wrong artefact at the wrong version.
