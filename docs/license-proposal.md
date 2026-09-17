# Code licence — applied

**Applied:** MIT for the application source. `LICENSE` at the repository root carries the
full text, copyright held by Bruce Hoppe (2026). The `Cargo.toml` workspace and
`package.json` both declare `license = "MIT"` to match.

**Third-party notices:** see `THIRD-PARTY.md` at the repository root — generated from
`cargo metadata` (Rust workspace, 540 resolved crates) and `npx license-checker`
(npm production dependencies). All resolved licences are permissive (MIT, Apache-2.0,
BSD, ISC, Zlib, MPL-2.0, Unicode-3.0, CDLA-Permissive-2.0, Unlicense); none impose
copyleft obligations on this application's own source.

| Component | Licence | Obligation |
|---|---|---|
| NOAA SWPC data | U.S. Government work, public domain | Attribution provided (Sources page, footer). |
| NASA SDO/AIA imagery via Helioviewer | NASA data policy (public), Helioviewer API | Credit line shown with every image. |
| `world-atlas` / Natural Earth coastlines | Natural Earth public domain; package ISC | Credit shown on Aurora view and Sources page. |
| `topojson-client` | ISC | Listed in `THIRD-PARTY.md`. |
| Tauri, reqwest, rusqlite (bundled SQLite: public domain), chrono, serde, tokio, sha2, dirs | MIT / Apache-2.0 | Listed in `THIRD-PARTY.md`. |

Regenerate `THIRD-PARTY.md` before each release — dependency versions and the resolved
licence set can change between releases:

```bash
cargo metadata --format-version 1
npx --yes license-checker --production --json
```
