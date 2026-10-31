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
- **Energy cubes keep their energy** when broken outside creative. The cube drops itself with the energy
  in `minecraft:custom_data` (`mekanism:energy`), and placing it puts the energy back. The tooltip shows
  "Stored Energy" with SI prefixes, as Mekanism does.

## Generators and cables (milestone 4, first part)

- **Solar Generator** (2 FE/t) and **Advanced Solar Generator** (12 FE/t) work the way
  `TileEntitySolarGenerator` does. They make power only in the overworld, by day, under open sky. Output
  scales with the sky light factor (rain lowers it) and with the biome: cool biomes help, while hot or
  humid ones hurt.
- **Wind Generator**, 1 to 5 FE/t depending on the height of its top (four blocks up) between Y 24 and
  the top of the level. It needs open sky and is recalculated once a second, as Mekanism does. Here it
  is one block and not five.
- **Universal cables**, all four tiers (8 k to 8.192 MFE/t). They use the iron bars template, so they
  join up on the four sides without new Horizon shapes. A cable holds one tick of what it carries. It
  feeds the machines and cubes beside it, then lets its energy run on to emptier cables, so energy never
  flows back. Mekanism pools a whole cable network instead, and the throughput per cable is the same.
  Proper six-way shapes wait for H2.

## Chemicals (milestone 5, first part)

`mekanism-core`'s `chemical` module holds chemical tanks and the rules below; the runtime keeps each tank in
the block entity's NBT (`id` and `amount`). Chemicals move only between touching blocks, like energy
before cables: machines push to every side, tanks out of their bottom.

- **Chemical Oxidizer**: `oxidizing` recipes (25, sulfur dust → sulfur dioxide and the like), 15 FE/t over
  100 ticks, into a 10,000 mB tank. The furnace menu's first slot takes the item.
- **Electrolytic Separator** on water: put water buckets in the first slot (the empty bucket comes out of the
  last). Each tick splits 2 mB of water into 2 mB hydrogen and 1 mB oxygen for 4 FE, into two 2,400 mB
  tanks. Flame = water, arrow = hydrogen.
- **Gas-Burning Generator**: burns hydrogen (2 FE/mB) or ethene (8 FE/mB), up to 10 mB a tick scaled by how full its 1,000 mB tank is,
  into a 2,000 FE store and pushes the energy out like the other generators.
- **Chemical Tanks**, basic to ultimate (64k to 8.192M mB, 1k to 512k mB/t out of the bottom). A tank takes
  chemicals on every side but its bottom.
- **Pressurized tubes**, all four tiers (750 to 256,000 mB/t), work like universal cables: on the iron
  bars template, holding one tick of what they carry, feeding the blocks beside them and passing the rest
  on to emptier tubes with the same chemical.

Not yet: the separator's other recipes (brine, sodium), side configuration, and showing
tank contents in a screen.

## Ore tripling (milestone 6)

- **Purification Chamber**: `purifying` recipes (28, ore or raw ore → 3 clumps), 10 FE/t over 200 ticks,
  using 1 mB of oxygen a tick from a 210 mB tank. Oxygen comes from an Electrolytic Separator, a chemical
  tank or a pressurized tube beside it. Slots: input, (unused), output.
- The rest of the chain already runs: the Crusher turns clumps into dirty dust and the Enrichment Chamber
  turns dirty dust into dust, so one ore gives three ingots.
- Upgrades do not apply to the chamber yet.

## Metallurgic infusing (milestone 7)

- **Metallurgic Infuser**: `metallurgic_infusing` recipes (31: enriched iron, steel, alloys, circuits,
  mycelium, podzol...), 5 FE/t over 200 ticks. Slots: the item to infuse, the infuse item, the output.
  The infuse item (coal, redstone, diamond dust, bio fuel... 24 `chemical_conversion` recipes) turns into
  infuse chemical in a 1,000 mB tank, one item at a time when it all fits. The recipe takes its infuse
  chemical at the end of the operation. Flame = infuse tank, arrow = progress.
- Mekanism names infuse chemicals in recipes by tag (`#mekanism:carbon`), and each such tag holds the one
  chemical of the same name, so `gen_recipe_table.py` writes the chemical itself.
- This makes Mekanism's crafting chain playable: alloys and control circuits come from the infuser.

## Compressing and chemical infusing (milestone 8)

- **Osmium Compressor**: `compressing` recipes (refined obsidian and glowstone dust → ingots), 15 FE/t
  over 200 ticks with 1 mB of osmium a tick. It and the Purification Chamber are one advanced-machine
  kind now (`chemical::ADVANCED_MACHINES`), and both take their chemical from items in the middle slot
  (`chemical_conversion`: osmium ingot → 200 mB osmium, flint → oxygen) as well as from the blocks beside
  them.
- **Chemical Infuser**: `chemical_infusing` recipes (5), one operation a tick at 15 FE, three 10,000 mB
  tanks. Each input tank takes one of the two chemicals, in either order. With an oxidizer making sulfur
  dioxide and a separator making oxygen beside it, it makes sulfur trioxide. Hydrogen chloride waits for
  chlorine (brine and the Thermal Evaporation Plant).
