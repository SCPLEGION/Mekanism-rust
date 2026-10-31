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

## Content (milestone 1)

`python3 tools/carry_content.py` turns this repository's data into four Horizon content mods, one per
module and namespace (Horizon declares a content mod's blocks and items only under its own id):
`target/content/{mekanism,mekanismgenerators,mekanismadditions,mekanismtools}-0.1.0.hmod`. Each carries
the module's assets, data, and its additions to vanilla and `c` tags, plus:

- a block declaration per blockstate, copying a vanilla template chosen from what the block is (ores →
  iron ore, machines with a facing → furnace, transmitters → iron bars, slabs, stairs, fences, glass...);
  for furnace-templated blocks the blockstate's `active` property is renamed to the furnace's `lit`;
- an item declaration per item that is not a block's item (tools and armour copy the iron ones);
- Mekanism's ores as vanilla `minecraft:ore` features with `OreType.java`'s default sizes.
- NeoForge's convention (`c`) item tags on vanilla items that Mekanism's recipes use (`#c:ingots/iron`...),
  written by hand in `tools/convention_tags.py`, since vanilla Horizon has none.

`cargo horizon-mod check <file>` (from the Horizon repository) accepts all four. Put them in the game
folder's `horizon-mods/` and turn them on under Options > Mods.

Ores are placed by Mekanism's NeoForge biome modifiers (`data/mekanism/neoforge/biome_modifier/`), which
the Horizon fork reads from content mods since its `milestone/m1-content` branch. Known gaps: the salt disk and the retrogen features use Mekanism's own feature types and are left out; models with
NeoForge loaders (OBJ, transmitters) show as missing; machine recipe types (crushing, enriching...) load
only once the native mod reads them; Tools' items keep the iron templates' damage and durability.

## Machines (milestone 2: ore doubling)

Hooks: `block_entity`, `block_entity_ticks`, `block_entity_container`, `block_entity_tick` (API 15 to 18,
plus the API 20 block-property helpers). Logic in `mekanism-core` (`machine`, `generator`, `heat`,
`recipes`, `energy`); glue in `runtime.rs`.

| Block | What it does | Slots (furnace menu) | Bars |
|---|---|---|---|
| Enrichment Chamber | `enriching` recipes (osmium ore → 2 osmium dust), 5 FE/t, 200 ticks | input, (energy), output | flame = energy, arrow = progress |
| Crusher | `crushing` recipes, 8 FE/t, 200 ticks | input, (energy), output | same |
| Energized Smelter | vanilla smelting recipes, 5 FE/t, 200 ticks | input, (energy), output | same |
| Heat Generator | furnace fuel → lava (1000 mB per lava bucket's burn time), lava → heat → FE | (unused), fuel, (unused) | flame = lava, arrow = heat |
| Energy Cubes (basic to ultimate) | store 4M to 256M FE, output 4k to 256k FE/t out of the front | none | none |

Energy moves only between touching Mekanism blocks: generators push to every side, cubes out of their
front and take from the other sides, machines take from every side. Cables come with milestone 4.

`tools/gen_recipe_table.py` writes `core/src/recipe_table.rs` (223 enriching, 255 crushing recipes) from
the generated data; recipes that need another mod are left out.

Not like Mekanism yet, each waiting for a later milestone or Horizon hook: the screens are the vanilla
furnace's; the middle slot takes no energy items (item energy needs custom item components); no upgrades
or side configuration; the Heat Generator ignores neighbouring lava and the biome's temperature; energy
cubes face only the four horizontal directions (the furnace template's `facing`); machines are not shown
running on clients (block entity sync), only their `lit` state.

## More machines and upgrades (milestone 3)

- **Precision Sawmill** (`sawing`, 157 recipes) and **Combiner** (`combining`, 106 recipes) run like the
  other machines, at 5 FE a tick. The Combiner takes its extra input from the middle slot. The sawmill
  has one output slot here, so a secondary output (sawdust and the like, rolled with the recipe's
  chance) waits inside the machine until the output slot takes it, and the machine stops until then,
  the way a full secondary slot stops it in Mekanism.
- **Speed and energy upgrades.** Right click a machine holding an upgrade to install one, or all that
  fit when sneaking (up to 8 of each). The formulas are `UpgradeUtils`' (ticks, energy per tick, storage),
  and a progress machine stores one operation of energy (`MachineEnergyContainer.updateMaxEnergy`).
- **Upgrade items.** Mekanism tells upgrades apart by its own `mekanism:upgrade_type` item component,
  which Horizon does not have. `carry_content.py` moves it into vanilla `minecraft:custom_data` in the
  carried recipes, sets `minecraft:custom_model_data` so the item model (a vanilla `select`) draws the
  right texture, and spells the name out with `minecraft:item_name`. Modules get the same treatment.
- The furnace menu's flame now shows stored energy as thousandths of capacity, since data slots are
  16 bits.
- **Factories** (basic 3, advanced 5, elite 7, ultimate 9 processes) for enriching, crushing and smelting.
  They show a two-row chest menu: the inputs are on the top row and each process's output is below its
  input. All processes share one energy store sized for one operation of each, and each process uses the
  machine's energy per tick. Upgrades work as they do on machines. Input sorting is not ported yet.
