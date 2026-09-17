## What this changes

## Why

## Testing

- [ ] `cargo test --workspace` passes
- [ ] `npm test` passes
- [ ] `npm run typecheck` passes (if frontend touched)
- [ ] Manually exercised in `npm run tauri dev` (if UI-visible)

## Checklist

- [ ] `crates/swo-core` stays free of I/O (if touched)
- [ ] New/changed parsing, units, time handling or classification has fixture-backed tests
- [ ] Docs updated if behavior, config or data sources changed (`README.md`, `docs/`)
