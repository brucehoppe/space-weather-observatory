# Code licence — proposal (not yet applied)

**Proposed:** MIT for the application source (`Cargo.toml` workspace already declares
`license = "MIT"` as the intent; no `LICENSE` file has been committed).

**Ownership assumption, to be confirmed by the repository owner before publication:** the
owner of this repository holds copyright in the application code and is the licensor. No
institutional endorsement is claimed or implied. Personal attribution text is deliberately
left unspecified per the project brief; supply exact wording if any is wanted.

**Third-party notices to ship with a release:**

| Component | Licence | Obligation |
|---|---|---|
| NOAA SWPC data | U.S. Government work, public domain | Attribution provided (Sources page, footer). |
| NASA SDO/AIA imagery via Helioviewer | NASA data policy (public), Helioviewer API | Credit line shown with every image. |
| `world-atlas` / Natural Earth coastlines | Natural Earth public domain; package ISC | Credit shown on Aurora view and Sources page. |
| `topojson-client` | ISC | Notice in `THIRD-PARTY.md` (generate with `npx license-checker` / `cargo about` at release). |
| Tauri, reqwest, rusqlite (bundled SQLite: public domain), chrono, serde, tokio, sha2, dirs | MIT / Apache-2.0 | Include notices; `cargo about generate` recommended. |

Apply by adding `LICENSE` (MIT, with the confirmed copyright holder line) and generating
`THIRD-PARTY.md` before the first public tag.
