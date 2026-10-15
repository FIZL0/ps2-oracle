# Agent notes

`ps2-oracle`: an R5900 interpreter that calls a PS2 game's own functions for bit-exact port tests. Used by
opentennis (`../opentennis`, FIZL0/opentennis: `crates/hst-oracle`), FIZL0/ps2-decomp-kit-linux and
FIZL0/ps2-decomp-kit-windows (`crates/kit-oracle`) through tag-pinned git dependencies. Its arithmetic is
FIZL0/ps2-float (`../ps2-float`), pinned by tag.

- **These libraries are ours.** Agents may fix and extend them when a port needs it (new op, missing function,
  wrong rounding): add a test (`tests/interpret.rs` assembles the instruction by hand), tag a release (bump
  `version` in `Cargo.toml`, tag `vX.Y.Z`, push the tag), then bump the tag in every dependent above and run their
  tests. Never patch a vendored copy in a dependent. A fix in the arithmetic itself belongs in ps2-float (release
  it first, then bump it here).
- Game-agnostic: no disc names, serials, load addresses, function addresses or config formats in here, and never
  any game data (no code bytes, no dumps, not in tests either: tests assemble their own programs and disc images).
- One dependency (ps2-float); no `unsafe`.
- Checks: `cargo test`, `cargo clippy --all-targets -- -D warnings`.
- Git identity: FIZL0 only (user + credential helper pinned in `.git/config`).
