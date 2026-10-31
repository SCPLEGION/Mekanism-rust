# Mekanism on Horizon

This fork ports Mekanism to [Horizon](https://github.com/SCPLEGION/horizon), the Rust rewrite of Minecraft
26.3. The Java sources in `src/` stay as the reference: they run on NeoForge 26.3, the same game version,
so they are the oracle the port is checked against. The plan, milestones and the Horizon hooks each one
needs: [Plan: running Mekanism on Horizon](https://claude.ai/artifact/NG2CYKPraydFoQ1bG16JYq).

## Layout

| Path | What it is |
|---|---|
| `mixins/mekanism/` | the native mod: a [horizon-mixins](https://github.com/horizon-rust/horizon-mixins) bundle |
| `mixins/mekanism/horizon-mixin.toml` | its manifest: the pinned Horizon commit, the crate, the module and the hooks |
| `mixins/mekanism/core/` | `mekanism-core`, all game logic as a plain crate with no Horizon dependency |
| `mixins/mekanism/runtime.rs` | glue compiled inside Horizon's game crate as `game::mekanism` |
| `mixins/mekanism/*.rs` (others) | one small helper per hook point, calling into `game::mekanism` |
| `compatibility.json` | the Horizon and horizon-mixins revisions, and hashes of the files the hooks touch |
| `tools/horizon.py` | stage the bundle with horizon-mixins, then `check`, `test` or `build` the game |
| `tools/manual_stage.py` | type-check the glue without horizon-mixins |
| `tools/package.py` | write `target/mekanism-<version>.hnmod` |

Each milestone lives on its own branch (`milestone/m0-setup`, `milestone/m1-content`,
`milestone/m2-ore-doubling`...).

## Rules for the port

- **Logic in `mekanism-core`, glue in the bundle.** Anything that can be tested without the game goes in
  the crate, with the unit tests next to it. The bundle's helpers and `runtime.rs` only convert between
  Horizon's types and the crate's.
- **Only stable hook points.** Hooks are named (`hook = "..."`, `hook_version`) from Horizon's
  `native-hooks.json`, never raw targets, so the mod keeps building on newer Horizons. A hook Mekanism needs
  and Horizon lacks is added to the fork of Horizon as a general hook point, written so it can go upstream.
- **Behaviour comes from the Java source in this repository** (MIT), and numbers (tiers, rates, costs)
  are copied from it with the Java file named in a comment.

## Build and check

```sh
cargo test                                 # mekanism-core's tests
cargo clippy --all-targets -- -D warnings

# Type-check the glue against the pinned Horizon (no horizon-mixins needed):
STAGE=$(mktemp -d)
git -C ../horizon archive "$(python3 -c 'import json;print(json.load(open("compatibility.json"))["horizon_revision"])')" | tar -x -C "$STAGE"
python3 tools/manual_stage.py "$STAGE" mixins/mekanism
(cd "$STAGE" && cargo check -p horizon-client --bin horizon-client --no-default-features)

# The real staging, with horizon-mixins checked out at compatibility.json's revision beside this repo:
python3 tools/horizon.py check --horizon ../horizon
python3 tools/horizon.py build --horizon ../horizon

python3 tools/package.py                   # target/mekanism-0.1.0.hnmod
```

Install the `.hnmod` from the Horizon launcher's Mods page (or `horizon-launcher native-build <folder>
--trust-native-code`). It is native code: the launcher builds a game for the instance it is installed in.
