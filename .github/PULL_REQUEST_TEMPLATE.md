<!-- Keep the title imperative and under 70 characters. -->

## What this changes

<!-- The behaviour that is different, not a restatement of the diff. -->

## Why

<!-- The problem. If it is a bug, say how it reproduced. -->

## How it was verified

<!-- The commands you actually ran, and what they printed. -->

```
cargo fmt --all -- --check
cargo clippy -p redrob-recall -p redrob-recall-core -p redrob-recall-api --all-targets --profile ci -- -D warnings
cargo test -p redrob-recall-core -p redrob-recall-api --profile ci
python3 scripts/check-brand.py && python3 scripts/check-version.py
```

## Checklist

- [ ] No credentials, no signing key, no real indexed file content in the diff,
      the tests, or the commit messages.
- [ ] Local-first guarantee preserved: nothing new leaves the machine except the
      excerpts the user explicitly sends with Ask.
- [ ] If a dependency changed, `cargo audit` is clean, and
      `THIRD_PARTY_NOTICES.md` still identifies every principal component.
- [ ] The workspace version has its dated CHANGELOG section (`python3 scripts/check-version.py`).
- [ ] "Qdrant" appears only where it identifies the upstream engine, never as
      Redrob product naming.
