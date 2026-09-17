<!-- Keep the title imperative and under 70 characters. -->

## What this changes

<!-- The behaviour that is different, not a restatement of the diff. -->

## Why

<!-- The problem. If it is a bug, say how it reproduced. -->

## How it was verified

<!-- The commands you actually ran, and what they printed. -->

```
npm ci
npm run verify
npm audit
cargo audit --file src-tauri/Cargo.lock
```

## Checklist

- [ ] No credentials, no signing key, no real indexed file content in the diff,
      the tests, or the commit messages.
- [ ] Local-first guarantee preserved: nothing new leaves the machine except the
      excerpts the user explicitly sends with Ask.
- [ ] If a dependency changed, `npm audit` and `cargo audit` are clean, and
      `THIRD_PARTY_NOTICES.md` still identifies every principal component.
- [ ] Versions stay in lockstep (`npm run check:version`).
- [ ] "Qdrant" appears only where it identifies the upstream engine, never as
      Redrob product naming.
