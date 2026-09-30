# Decision: keep SQLite FTS5, do not adopt Tantivy

Item 1b.0 of the porting plan. **Decided: FTS5 stays.** Port bloop's query language
and ranking on top of it; do not port its index.

Reproduce the evidence with `research/fts5-vs-bloop-grammar.py` and
`research/fts5-schema-reach.py` in the workspace.

## The question

bloop indexes with Tantivy. This product uses SQLite FTS5. The porting plan recorded
the constraint "do not end up with two indexes" and left the choice open.

Reading the code narrowed it first: **this product already runs hybrid retrieval.**
`storage.rs` maintains a `chunks_fts` FTS5 table ranked by `bm25()`, `search.rs`
queries Qdrant Edge for vectors, and the two are fused by reciprocal rank at k=60.
That is the same architecture bloop arrives at, reached with a different full-text
engine. So adopting Tantivy replaces a **working** component, and the burden is on
the replacement.

The reason offered was "a richer query language and better ranking controls". Both
halves were tested rather than argued.

## Can FTS5 express bloop's query grammar?

Every feature in bloop's `query/grammar.pest`, run against FTS5 on SQLite 3.46.1:

| bloop feature | FTS5 | Result |
|---|---|---|
| implicit AND (intersection) | `rust AND runtime` | works |
| explicit `or` | `tokio OR embeddings` | works |
| grouping with `()` | `(a OR b) AND (c OR d)` | works |
| phrase, `"double quoted"` | `"full-text search"` | works |
| `content:` label | `content: bm25` | works |
| other column filters | `heading: embeddings` | works |
| **regex `/…/`** | `/handle[A-Z]\w+/` | **refused: syntax error** |
| `case:sensitive` | — | folds case by default |

And FTS5 has three things bloop's grammar does **not**: `NOT`, `NEAR(a b, 5)`, and
prefix matching `asynchr*`.

Four of bloop's labels do not apply at all. `repo:`, `org:` and `branch:` exist
because bloop searches git repositories; this product searches a folder of
documents. `symbol:` needs code intelligence, already `planned` in the
compatibility matrix. `path:` is not an FTS5 term and does not need to be —
`search.rs` already filters by path prefix in SQL after fusion.

**So exactly one feature is out of reach: regex.** FTS5 is a tokenised inverted
index; a regex needs the term dictionary walked, which Tantivy's `RegexQuery` does.

## Does that one feature justify the migration?

No, and the reason is what each product is for.

bloop needs regex because developers grep **code**. This product indexes PDF, DOCX,
HTML and prose. Its compatibility matrix lists source code as `planned` — if it ever
indexes code, this question reopens honestly, and the answer may differ then.

Case sensitivity is the other gap and it is not a Tantivy question: FTS5's
`unicode61` tokenizer folds case, and a case-sensitive mode is a different tokenizer
or a post-filter over the fused results.

## Does FTS5 offer ranking control?

Yes, and the measurement is worth keeping because the scores are easy to misread.
Same query, only the column weights changed:

```
default (all 1.0)    b.md -0.000002  a.md -0.000001
heading x10          a.md -0.000002  b.md -0.000002
content x10          b.md -0.000002  a.md -0.000001
heading only         a.md -0.000001  b.md -0.000000
```

`bm25()` returns **negative** scores, more negative being a better match, which is
why ordering ascending is correct. The order flips with the weights, so the control
is real.

## What the reading actually found, which was not in the plan

**`chunks_fts` indexes ONE column.**

```sql
CREATE VIRTUAL TABLE chunks_fts USING fts5(
    content,
    content='chunks',          -- an external-content OPTION, not a column
    content_rowid='id',
    tokenize='unicode61 remove_diacritics 2'
);
```

`content='chunks'` and `content_rowid='id'` are options. There is one indexed
column. Measured consequences:

| Query | content only | path + heading + content |
|---|---|---|
| a word in the body | 1 hit | 1 hit |
| a word only in the **heading** | **0 hits** | 1 hit |
| a word only in the **filename** | **0 hits** | 1 hit |
| `heading:` filter | **refused: no such column** | 1 hit |
| `path:` filter | **refused: no such column** | 1 hit |

**A document whose title matches your search does not come back unless the words are
also in its body.** That is a user-visible gap in a document search tool, and it has
nothing to do with Tantivy.

So a meaningful part of the case for a "richer query language" is a case for using
FTS5 properly: index `heading` and `path` as columns. That is a schema change and a
re-index, not an engine migration.

## What migrating would have cost

For the one feature it buys:

- A second on-disk index format alongside what FTS5 already stores.
- Two things for `create_backup` and `integrity_check` to keep consistent. Today
  both are one SQLite call on one file.
- A migration path for every existing library.
- A new dependency tree in `Cargo.lock` and the distribution notices.

## Consequences for 1b.3

When porting bloop's search layer:

1. **Take the query grammar, not the index.** bloop's `query/parser.rs`,
   `query/compiler.rs` and `query/planner.rs` translate a parsed query into index
   operations. The parser and the planner's shape port; the target is FTS5 `MATCH`
   plus the existing SQL filters.
2. **Take the ranking and stopword work.** `query/ranking.rs` and
   `query/stopwords.rs` are engine-independent.
3. **Take snippet selection.** `snippet.rs` picks context lines around a hit;
   `search.rs` currently returns whole chunks.
4. **Add `heading` and `path` as indexed columns first.** The column filters have
   nothing to filter on until then, and it fixes a real gap on its own.
5. **Drop `repo:`, `org:`, `branch:` and `symbol:`.** Three do not apply; the fourth
   waits on code intelligence.
6. **Regex is out of scope** while this product indexes documents. Record it as the
   one thing FTS5 cannot do, so that a future decision to index code reopens the
   question with the reason already written down.

## A small inconsistency noted while reading

`storage.rs:259` selects `bm25(chunks_fts, 1.0)` and orders by `bm25(chunks_fts)`
without the weight. Identical for one column at weight 1.0, so harmless today —
and wrong the moment a second column is added, which step 4 above does.
