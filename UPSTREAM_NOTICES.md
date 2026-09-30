# Upstream source notices

Attribution for source code **copied into this repository** from another project.

`THIRD_PARTY_NOTICES.md` is the neighbouring file and covers a different obligation: components this
product *embeds or depends on*, each shipped under its own licence. Copied source changes what this
repository itself contains, has a different lifetime, and is reviewed whenever
`docs/upstream-sources.toml` changes — so it is recorded separately here, matching redrob-canvas and
redrob-query.

Only upstreams whose `kind` is `code` in `docs/upstream-sources.toml` belong here. An `algorithm`
entry must not appear: reading a program to reimplement its behaviour creates no attribution duty,
and listing one would misstate what this repository contains. `scripts/verify-upstream.mjs` enforces
both directions.

## bloop

Copyright the bloop contributors, BloopAI.

Pinned at `431e9e82c5a293c40f22aadb3615c3aa387af82e` (branch `oss`) of
<https://github.com/BloopAI/bloop>.

Licensed under the Apache License 2.0 — the same licence this product carries, so no relicense is
required and none was performed. The full text is at <https://www.apache.org/licenses/LICENSE-2.0>
and in this repository as `LICENSE`.

bloop ships a single root `LICENSE` and no `NOTICE` file, so Apache section 4(d) — which would oblige
us to carry and propagate upstream's own notice text — imposes nothing. Section 4(b) still applies:
copied files carry a notice stating that they were changed.

The upstream repository is **archived**, last commit 2024-12-04. For a pin that is an advantage: the
tree cannot move underneath us, and Apache-2.0 source remains usable whether or not anyone maintains
it. The consequence is that no upstream fix will ever arrive, so anything taken from here is ours to
maintain.

### Which subsystems

Filled in as porting lands, one row per subsystem, so this file states what is actually here rather
than what was planned.

| Subsystem | Upstream path | Our path | Landed |
|---|---|---|---|
| _(no copied source yet)_ | | | |

**The query language is REIMPLEMENTED, not copied, which is why the table above is still empty.**
`src-tauri/src/query.rs` answers the same question bloop's `query/grammar.pest` answers -- what does a
person's search string mean -- and shares no code with it. bloop parses with pest into a Tantivy query;
this is a hand-written tokeniser and recursive-descent parser producing an FTS5 `MATCH` expression. The
grammar it implements is a reduced version of bloop's, and the reductions are recorded in
`docs/compatibility.md`: `regex` is out of scope because FTS5 cannot walk a term dictionary, and
`repo:`, `org:`, `branch:`, `symbol:` and `open:` have no meaning in a product that searches a folder of
documents rather than git repositories.

Reading a grammar to learn what a feature means is not copying, and bloop's Apache-2.0 licence would
permit the copying anyway -- this entry exists so the distinction is on the record rather than inferred
from an empty table.

### Boundary

The index **storage** layer is not settled. bloop indexes with Tantivy; this product uses SQLite
FTS5. Both use Qdrant for vectors. Copying bloop's storage layer as-is would leave two full-text
indexes in one product — a correctness problem before it is a size problem, because two indexes
disagree and the disagreement shows up as search results that depend on which path answered. Search
and chunking come first; storage waits on that decision.

## Trademarks

"Redrob" and the Redrob logo are trademarks of Redrob; the Apache License grants no rights to use
them, per its section 6. bloop's and Qdrant's names are likewise their own and are used here only to
identify their projects.
