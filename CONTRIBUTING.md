# Contributing

Thanks for your interest in Space Weather Observatory. This is a small, source-grounded
desktop app; contributions that keep it small, tested and honest about its data are welcome.

## Before you start

- Read `docs/architecture.md` for the shape of the codebase (`crates/swo-core` is pure
  scientific logic with no I/O; `src-tauri` is the desktop backend; `src/` is the frontend).
- Read `docs/sources.md` and `docs/solar-wind-alert.md` if your change touches parsing,
  units, timestamps, flux classification or the alert state machine — this project treats
  scientific correctness as a hard requirement, not a nice-to-have.
- For anything non-trivial, open an issue first to discuss the approach before writing code.

## Development setup

```bash
npm ci
cargo test --workspace         # Rust tests, fixture-only, no network
npm test                       # frontend unit tests
npm run tauri dev              # desktop app with hot reload
```

Browser-only preview (no backend): `npm run dev` → <http://localhost:5173/>.

## Making a change

- Add or update tests alongside any change to parsing, units, time handling, flux
  classification or the alert logic — `crates/swo-core` is fixture-driven and has no
  network access in tests by design.
- Keep `crates/swo-core` free of I/O. If a change needs network, filesystem or clock
  access, it belongs in `src-tauri`.
- Run `cargo test --workspace` and `npm test` before opening a pull request.
- Match existing formatting: `cargo fmt` for Rust, existing TypeScript style in `src/`.
- Keep changes scoped — this project favours small, reviewable diffs over sweeping ones.

## Reporting issues

Use the issue templates. For anything that looks like it could be a data-correctness bug
(wrong units, misclassified flux, a timestamp in the wrong zone), include the exact
product, time and value you observed, and if possible a link to the equivalent NOAA SWPC
page so it can be checked against the source.

## Licence

By contributing, you agree your contribution is licensed under the project's MIT licence
(see `LICENSE`).
