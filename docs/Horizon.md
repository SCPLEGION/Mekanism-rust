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
  Six-way shapes came in milestone 17.

## Chemicals (milestone 5, first part)

`mekanism-core`'s `chemical` module holds chemical tanks and the rules below; the runtime keeps each tank in
the block entity's NBT (`id` and `amount`). Chemicals move only between touching blocks, like energy
before cables: machines push to every side, tanks out of their bottom.

- **Chemical Oxidizer**: `oxidizing` recipes (25, sulfur dust → sulfur dioxide and the like), 15 FE/t over
  100 ticks, into a 10,000 mB tank. The furnace menu's first slot takes the item.
- **Electrolytic Separator**: put water or brine buckets in the first slot (the empty bucket comes out of the
  last). Brine (10 mB → 1 sodium + 1 chlorine) works since milestone 9. Each tick splits 2 mB of water into 2 mB hydrogen and 1 mB oxygen for 4 FE, into two 2,400 mB
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

## Thermal Evaporation (milestone 9)

- **Thermal Evaporation Plant**: a 4x4 tower of Thermal Evaporation Blocks, 3 to 18 high: a full floor,
  then hollow rings, with the controller and any valves in the rings. The controller checks the tower
  once a second. Its water tank holds 64 buckets per quarter of the tower's volume.
- **Heat**: each Advanced Solar Generator on a top corner that sees the sun adds 0.2 K a tick, and the
  tower loses 0.02 K per square root of its difference from 300 K, so four solars settle at 1,900 K
  (`EvaporationMultiblockData.simulateEnvironment`). The tower makes `(T − 300) × 0.4 × height / 18`
  operations a tick of 10 mB water → 1 mB brine, or one every few ticks below 1.
- **Slots** (furnace menu): water buckets in the first (the empty bucket comes out of the last), an empty
  bucket in the middle one fills with brine. Valves pass brine to an Electrolytic Separator beside them,
  which makes sodium and chlorine; hydrogen and chlorine in a Chemical Infuser make hydrogen chloride.
- Differences: heat leaks towards 300 K, not the biome's own temperature; water comes only from buckets
  (no fluid pipes or pumps yet); the lithium recipe is not run; there is no tower screen.

## Ore quadrupling (milestone 10)

- **Chemical Injection Chamber**: `injecting` recipes (109; ore → 4 shards on hydrogen chloride, and the
  water-vapour recipes), 15 FE/t over 200 ticks, 1 mB a tick. It is the third advanced machine and takes
  either chemical; with one in its tank, only that chemical's recipes run. Hydrogen chloride comes from
  a Chemical Infuser (hydrogen + chlorine) beside it, or from salt in its middle slot.
- With the Purification Chamber (shard → clump), Crusher and Enrichment Chamber, one ore gives four dusts.
  Water vapour has no source yet (Rotary Condensentrator).

## Rotary Condensentrator (milestone 11)

- Decondensentrating only: a fluid from buckets in the first slot (the empty bucket comes out of the last)
  turns into its chemical, 1 mB a tick at 2 FE, from a 10,000 mB tank into another (`rotary` recipes, 20;
  water → water vapour). The chemical goes to the blocks beside it, so a condensentrator beside a
  Chemical Injection Chamber runs its water-vapour recipes.
- Not yet: the condensentrating mode (chemical → fluid), its mode button, and speed upgrades
  (`2^upgrades` operations a tick in Mekanism).

## Side configuration (milestone 12)

- Right click a face of any Mekanism block with the **Configurator** to cycle it: Default (what the block
  does unconfigured), None, Input, Output. Chat says the face and its new mode. The modes are kept in the
  block entity's NBT (`sides`, two bits per face).
- One mode per face covers energy and chemicals: a face set to None or Input gives nothing out, and one
  set to None or Output takes nothing in. Cables and tubes respect their neighbours' faces too.
- Simpler than Mekanism's `TileComponentConfig`: there is no separate setting per energy, chemical and
  item, no colours on the block and no configuration screen, and hoppers still use the furnace's fixed
  faces. The chat message goes through the game's `system_message`; whether it reaches a LAN or server player is untested.

## Induction Matrix (milestone 13)

- A hollow box of **Induction Casing** and **Induction Ports**, 3 to 18 blocks on each side, with
  **Induction Cells** (8 G to 4 T FE each) and **Providers** (256 k to 131 MFE/t each) inside; the rest of
  the inside may be air. Each port checks the matrix once a second.
- Ports take energy in on every face. Set a port's face to Output with the Configurator and it gives energy
  out of that face, up to the matrix's rate (all providers together). Mekanism applies the rate to the
  whole matrix; here each output face may move that much.
- The energy lives in the matrix's first port (the lowest by x, then y, then z). Other ports buffer one tick
  and hand it on. Breaking that port loses the stored energy, unlike Mekanism, which keeps it in the
  multiblock's saved data. There is no matrix screen yet.

## Logistical transporters (milestone 14)

- **Logistical transporters**, four tiers (1, 16, 32 or 64 items at a time, `TransporterTier`), on the
  iron bars template like cables. Twice a second a transporter beside a Mekanism machine pulls from the
  machine's first filled output slot, when that face gives out (side configuration), and sends the items
  along the joined transporters to the nearest container that takes them: a chest, barrel, furnace,
  hopper or another machine's input, through the side the transporter touches, as a hopper would
  (`native_insert_item`). Whatever fits nowhere goes back into the machine.
- Simplified: items arrive at once instead of travelling along the pipe; there is no colour sorting, no
  diversion or restrictive transporters, and no Logistical Sorter.
- Since milestone 15 (Horizon API 24, `native_extract_item`), transporters pull from any container beside
  them as a hopper would: chests, barrels, a furnace's output, a machine's outputs. The bundle is pinned to
  the Horizon fork's `milestone/h3-extract-item` commit from then on.

## Gear and item energy (milestone 16)

- **Items hold energy** in `minecraft:custom_data` under `mekanism:energy`, as before for energy cubes: the
  Energy Tablet and Atomic Disassembler hold 1 MFE and charge at 5 kFE/t, the Electric Bow 120 kFE at
  600 FE/t (`GearConfig` defaults), an energy cube item what its tier holds. The three gear items stack to
  one. Their tooltip shows the stored energy.
- **Energy cubes** now open a furnace-style menu: the top slot charges the item in it from the cube, the
  bottom slot empties its item into the cube (`EnergyInventorySlot`). The flame shows the cube's fill.
- **Machines' energy slot**: the middle slot of the single machines (enrichment chamber, crusher, smelter,
  sawmill; not the combiner, whose middle slot is its extra input) runs the machine off an energy tablet or
  an energy cube item in it. Tools only spend their energy, so a disassembler there gives nothing.
- **Atomic Disassembler** mines at efficiency 20 (`DisassemblerMode.NORMAL`) while it holds the 200 FE a
  block costs (100 FE for blocks that break at once), slower as it runs out and not at all when empty, and
  each block it breaks outside creative spends that energy. This uses Horizon API 25
  (`native_destroy_speed`, added on the fork's `milestone/h4-destroy-speed` branch), so the bundle is pinned
  to that commit.
- The disassembler gets Mekanism's tool rules as a `minecraft:tool` component: every mineable block drops
  its loot, except those in `#mekanism:incorrect_for_disassembler`.
- Simplified: no attack damage from energy (modes came in milestone 18);
  generators and advanced machines have no energy slot.

## Six-way transmitters (milestone 17)

- Every transmitter (universal cables, pressurized tubes, logistical transporters, and the mechanical pipes
  and thermodynamic conductors that do nothing yet) is now a pipe: a centre cube and an arm toward each side
  it joins, drawn by plain block models that `tools/carry_content.py` writes in place of Mekanism's OBJ
  models (which Horizon does not load). Small transmitters are 6 pixels across, large ones 8, as the OBJ
  models are.
- The block copies the mushroom stem for its six direction properties. A side is `false` where it joins,
  because a stem's own updates already set a face to `false` where the same block touches it. Twice a
  second the bundle sets the rest: a side joins another transmitter of its kind (any tier), or a block it
  serves (cables: anything with energy; tubes: chemical machines and tanks; transporters: any block with
  slots, and vanilla chests, barrels, furnaces, hoppers, droppers, dispensers and shulker boxes).
- The block is see-through and lets light pass, and its outline and collision are the centre cube. This
  uses `"occludes": false`, which the Horizon fork's `milestone/h5-see-through-blocks` branch adds to
  content-mod block declarations, so the bundle is pinned to that commit.
- Simplified: the arms are not part of the outline or collision; a transmitter still conducts redstone as
  the stem does; side configuration does not cut arms off.

## Disassembler modes and generator energy slots (milestone 18)

- **Atomic Disassembler modes**: sneak + right click cycles Normal (20), Slow (8), Fast (128) and Off (0),
  as Mekanism's mode key does with `GearConfig` defaults (vein mining off). The mode is kept in
  `custom_data` under `mekanism:disassembler_mode`, named on the action bar when it changes and in the
  tooltip. A block costs 10 FE per point of the mode's efficiency, half for blocks that break at once; short
  of that, it mines as normal mode would with what it has (`getDestroySpeed`).
- **Generators charge items**: the Heat, Advanced Solar, Wind and Gas-Burning Generators charge an energy
  tablet, cube or tool in their first slot from what they make. The plain Solar Generator (an iron block with
  no menu) has no slot.
- Simplified: no radial menu or mode key (sneak + right click instead); no vein mining.

## Side configuration per type (milestone 19)

- Each face now has a mode per transmission type: items, chemicals and energy, as Mekanism's
  `TileComponentConfig` keeps them. Energy keeps the `sides` NBT key from milestone 12; items and chemicals
  use `sides_item` and `sides_chemical`. Energy out of machines, cubes and induction ports reads the energy
  faces, chemicals (and the separator's and rotary's fluids) the chemical faces, and transporter pulls the
  item faces.
- The Configurator configures one type at a time: sneak + right click in the air switches between items,
  chemicals and energy (Mekanism's mode key), shown on the action bar and in its tooltip. A right click on a
  face cycles that face for the chosen type, and chat names the type.
- Simplified: still four modes per face (Default, None, Input, Output) with no colours or screen; item faces
  only limit transporter pulls, not hoppers.

## Digital Miner (milestone 20)

- The Digital Miner (`mekanism-core`'s `miner`) mines every block in Mekanism's `c:ores` tag in a 21 × 21
  area centred on it, from y 60 down to y 0, one block every 80 ticks (speed upgrades shorten it), using
  40 FE per tick out of a 50,000 FE buffer (energy upgrades lower it). Its scan resumes where it stopped and
  checks at most 4,096 positions a tick.
- Each mined block's vanilla drops (no tool, no enchantments) go into the container above the miner;
  what does not fit is dropped above it. Slot 1 discharges energy items.
- Simplified: no filters, no silk touch, no radius or height settings and no screen; the area and depths
  are Mekanism's defaults.
