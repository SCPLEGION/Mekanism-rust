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

## Fluid tanks and mechanical pipes (milestone 21)

- Fluid Tanks (basic to ultimate, plus creative) hold 32,000 to 256,000 mB (`FluidTankTier`). They take
  fluid from every side and give it out of their bottom at 1,000 to 64,000 mB a tick. A right click with a
  bucket fills or empties one (water, lava and Mekanism's fluid buckets), and a right click with an empty
  hand shows what it holds on the action bar.
- Mechanical Pipes carry 250 to 32,000 mB a tick (`PipeTier`'s pull rate). They feed the fluid tanks,
  Electrolytic Separators and Rotary Condensentrators beside them, then share the rest with emptier pipes,
  as pressurized tubes do with chemicals. They join (M17's shapes) the blocks they serve and the Thermal
  Evaporation Plant's valves.
- The plant's valves now take water from the pipes and tanks beside them, and give brine to pipes and
  tanks as well as separators.
- Simplified: no fluid screens. Milestone 22 keeps tank contents on breaking and feeds the Heat Generator
  from pipes.

## Tanks keep their contents, lava by pipe (milestone 22)

- Breaking a fluid tank or chemical tank that holds something (outside creative) drops it with its
  contents in its custom data (`mekanism:fluid`, `mekanism:chemical`), as energy cubes keep their energy.
  Placing it again restores them, and the item's tooltip shows "Stored: <amount> mB <id>".
- The Heat Generator takes lava from the mechanical pipes and fluid tanks beside it, and pipes join it.

## Jetpack (milestone 23)

Needs the Horizon fork's `milestone/h6-payload-as-player` (API 26: a mod packet runs as its sender).

- The Jetpack and Armored Jetpack are worn on the chest (the plain one gives no armour). They hold 24,000 mB
  of hydrogen in custom data (`mekanism:chemical`, shown in the tooltip). A right click on a chemical tank
  that holds hydrogen, with a jetpack in hand, fills it.
- Holding jump (Space) while wearing a fuelled jetpack thrusts in Mekanism's normal mode
  (`IJetpackItem.handleJetpackMotion`: 0.15 up per tick, fading as you rise) and stops the fall. The
  window moves its player and tells the server on the `mekanism:jetpack` channel, and the server spends a
  mB of the wearer's hydrogen a tick and clears their fall too.
- Simplified: normal mode only (no hover or vector modes, no mode key), no flame particles or sound,
  and a worn jetpack does not show on the player. A dedicated server with
  `allow-flight=false` can kick a long flight for floating, as vanilla does.

## Nuclear fuel chain (milestone 24)

The steps from uranium to fissile fuel all run now (`mekanism-core`'s `nuclear`):

1. The Enrichment Chamber turns a uranium ingot into 2 yellow cake uranium (milestone 2's table).
2. The Chemical Oxidizer turns yellow cake into 250 mB of uranium oxide.
3. The new **Chemical Dissolution Chamber** takes a fluorite gem (or a fluorite block) in its slot and
   sulfuric acid piped in, 1 mB a tick (9 for a block) for 100 ticks at 16 FE a tick. It makes 1,000 mB
   (9,000) of hydrofluoric acid and pushes it to the tanks and tubes beside it.
4. The Chemical Infuser turns hydrofluoric acid and uranium oxide into 2 mB of uranium hexafluoride per mB.
5. The new **Isotopic Centrifuge** turns 1 mB of uranium hexafluoride into 1 mB of fissile fuel each tick
   at 8 FE a tick (and 10 mB of nuclear waste into 1 mB of plutonium). Pipe it in, and it pushes the
   output to its neighbours.

Simplified: no screens of their own (the dissolution chamber uses the furnace menu, with its progress on
the arrow); no upgrades; the ore slurry recipes came with milestone 27. The fission reactor that burns the
fuel comes next.

## Fission Reactor (milestone 25)

`mekanism-core`'s `fission`, simplified from Mekanism's reactor:

- Build a box, 3 to 18 blocks on each side, of Fission Reactor Casing and Reactor Glass with at least one
  Fission Reactor Port in its walls, and Fission Fuel Assemblies (and Control Rod Assemblies, or air)
  inside. Ports check the structure once a second, and the first port runs the reactor.
- It holds 8,000 mB of fissile fuel and of waste per assembly, 100 buckets of water and 1,000 buckets of
  steam per block of its volume. Fissile fuel comes in from the tubes and chemical tanks beside its ports,
  and water from the pipes and fluid tanks beside them. Steam and nuclear waste go out of every port into
  tubes and chemical tanks.
- A right click with an empty hand on a port steps the burn rate (0.1, 0.5, 1, 2, 5, 10, 20 mB a tick, up
  to 1 mB a tick per assembly), and sneak + right click switches it on or off. The action bar shows its
  state, rate and tanks.
- Each mB of fuel boils 20,000 mB of water into steam: `energyPerFissionFuel` (1 MJ) over water's 50 J a
  mB as coolant. This is the reactor's steady state, without its heat model.
- Simplified: no temperature, damage, meltdown or radiation. Without enough water, or with nowhere for
  the steam or waste, the reactor stops (SCRAM) and says so. There are no port modes, logic adapter or
  screen, and the tanks are lost if the first port is broken. Steam drives the Industrial Turbine, which
  comes next.

## Industrial Turbine (milestone 26)

`mekanism-core`'s `turbine`, with `TurbineMultiblockData.tick`'s formula:

- Build a box, 3 to 17 blocks on each side, of Turbine Casing, Turbine Valves, Turbine Vents and
  Structural Glass. Inside goes a shaft of Turbine Rotors topped by a Rotational Complex in a layer of
  Pressure Dispersers, with Electromagnetic Coils (and Saturating Condensers) above. Each rotor counts as
  carrying its two blades, up to 28. Valves check the structure once a second, and the first valve runs
  it.
- Steam comes in from the tubes and chemical tanks beside the valves (a fission reactor's steam, piped
  over). It holds 64 buckets per block below the dispersers. The flow is the smaller of dispersers ×
  1,280 mB × that volume and vents × 32,000 mB, scaled by how full the steam tank is. Each 100 mB makes
  5/28 × min(blades, 4 × coils) energy, which goes out of every valve into cables and machines.
  Condensers turn up to 64,000 mB of the used steam a tick into water, given to the pipes and fluid tanks
  beside the valves (back to the reactor).
- A right click with an empty hand on a valve shows the steam, production and stored energy.
- Simplified: blades are not placed by hand, the layout inside is not checked beyond which blocks are
  allowed, and nothing spins.

## Ore quintupling (milestone 27)

`tools/gen_slurry_table.py` writes `core/src/slurry_table.rs` from the recipe data: 23 dissolution, 7
washing and 10 crystallizing recipes. With milestone 10's injection chamber, an ore now goes 5×:

1. The Chemical Dissolution Chamber dissolves an ore (or 3 raw ores, or a raw block) in sulfuric acid into
   1,000 mB (2,000 mB, 9,000 mB) of its dirty slurry.
2. The new **Chemical Washer** turns each mB of dirty slurry and 5 mB of water (piped in) into a mB of
   clean slurry, at 5 FE a tick, and pushes it to the tanks and tubes beside it.
3. The new **Chemical Crystallizer** turns 200 mB of clean slurry into a crystal in its last slot over 200
   ticks at 15 FE a tick (brine into salt, antimatter into a pellet the same way). Pipe the slurry in, and
   take the crystals with a hopper or transporter.
4. The injection chamber turns crystals into shards, and the purification chamber turns shards into
   clumps.

Simplified: the washer and crystallizer have no screens of their own and no bucket or upgrade slots.

## Fusion Reactor (milestone 28)

`mekanism-core`'s `fusion`, with `FusionReactorMultiblockData`'s heat model (plasma and casing
temperatures, `burnFuel`, `injectFuel`, `transferHeat`) and Mekanism's default config:

- Build the 5×5×5 reactor of `FusionReactorValidator`: on each face, the corners are left out, the ring
  around the middle cross is Fusion Reactor Frame, and the cross is frame, Fusion Reactor Ports, Reactor
  Glass or a Logic Adapter. The Fusion Reactor Controller goes in the middle of the top; inside is air.
  Ports check the structure once a second, and the first port runs the reactor.
- Deuterium and tritium come in from the tubes and chemical tanks beside its ports (1,000 mB each), and
  water from the pipes and fluid tanks beside them (1,000 buckets per injection). Each tick it injects up
  to the injection rate (2 to 98, even) of D-T fuel. Fuel burns while the plasma is over 100 MK, 10 MJ of
  heat per mB. The plasma heats the casing, the casing's heat goes 5% into energy through the
  thermocouples and, with water in, boils it into steam. Energy (1 GJ buffer) and steam go out of every
  port into cables, tubes and tanks.
- At injection rate 2 and air cooled, the casing settles at 200 MK and makes 1 MJ a tick, Mekanism's
  figures. With water, more of the heat goes to steam for an Industrial Turbine.
- A right click with an empty hand on the controller or a port steps the injection rate; sneak + right
  click ignites it. The action bar shows whether it burns, the rate, temperatures, production and tanks.
- Simplified: ignition stands in for the lasers and the hohlraum (the plasma is heated to the
  temperature it needs at that rate). There is no screen, no damage to things inside, no biome
  temperature, and the tanks are lost if the first port is broken. Deuterium and tritium have no source
  yet: that comes next.

## Fusion fuel (milestone 29)

`mekanism-core`'s `isotopes` and the new separating and evaporating recipes give the fusion reactor its
deuterium and tritium:

- **Electric Pump**: every 19 ticks (fewer with speed upgrades) at 2 FE a tick, it takes the water source
  below it, as a bucket of water or, with a **Filter Upgrade** (right click the pump with it), 10 mB of
  heavy water. With `waterSourceConversion` on (the default), water is an infinite source and the block
  stays, as in Mekanism. The fluid goes up into the pipe or tank above. Filter upgrades go only into
  the pump.
- **Electrolytic Separator**: 2 mB of heavy water → 2 mB deuterium + 1 mB oxygen, at twice the energy
  (`separator/heavy_water.json`).
- **Thermal Evaporation Plant**: a tower fed brine (by bucket, or through its valves while it is not
  making brine itself) makes 1 mB of lithium per 10 mB, as `evaporating/lithium.json` says.
- **Solar Neutron Activator**: while it sees the sun, lithium piped in becomes tritium 1:1, up to 64 mB a
  tick at noon (`maxSolarNeutronActivatorRate` × the sun's brightness), pushed to the tubes beside it.

Simplified: the pump only pumps the block right below it, and only water; liquid lithium goes straight
from the tower into tubes as a chemical, with no rotary condensentrator step; the activator ignores the
biome's temperature and rain.

## Thermoelectric Boiler (milestone 30)

`mekanism-core`'s `boiler`, with `BoilerMultiblockData.tick`'s boiling and Mekanism's default config:

- Build a box, 3 to 18 blocks on each side, of Boiler Casing, Boiler Valves and Structural Glass. Inside
  goes one full layer of Pressure Dispersers, with at least one Superheating Element below it. It holds
  16 buckets of water per block below the dispersers and 160 buckets of steam per block above them.
  Valves check the structure once a second, and the first valve runs it.
- Water comes in from the pipes and fluid tanks beside its valves, and steam goes out of every valve into
  tubes and chemical tanks (an Industrial Turbine's valves).
- Heat comes from **Resistive Heaters** placed against its walls. Each one turns its energy usage (100 FE
  a tick; a right click with an empty hand steps it through 1k, 10k, 100k and 1M) into 60% as much heat.
  The casing holds 50 heat per block of the box. Over 100 °C, 70% of the heat above the boiling point
  (capped at 16 MJ a tick per superheating element) boils water at 50 heat per mB of steam.
- A right click with an empty hand on a valve shows the temperature, boil rate, superheaters and tanks.
- Simplified: no heated coolant (sodium) from a fission reactor, no valve modes or screen, and the tanks
  are lost if the first valve is broken.

## Machine screens (milestone 31)

The electric machines (Enrichment Chamber, Crusher, Energized Smelter, Precision Sawmill, Combiner and
the rest of `Kind::Machine`) open Mekanism's own screen instead of the furnace's, through Horizon's mod
screens (API 19/20: `native_screen`, `native_screen_draw`, `native_screen_tooltip`):

- Mekanism's panel, drawn with its GUI sprites from the content mod (`mekanism:gui/sprites/base`,
  nine-sliced as its `.mcmeta` says), and its slots where `TileEntityElectricMachine` puts them: input
  (64, 17), energy (64, 53, the power slot), output (116, 35).
- The vertical power bar at (164, 16) and the progress bar at (86, 38), as `GuiElectricMachine` places
  them, and the energy tab on the left.
- Tooltips: the power bar shows stored and maximum energy, the progress bar its percent, and the energy
  tab the energy used a tick and the installed speed and energy upgrades.
- The machine's menu data grows past the furnace's four slots: stored energy and capacity (in 15-bit
  pieces, as data slots are 16-bit), the upgrades and the usage.

Simplified: no side-config or upgrade tabs to click yet, no redstone control, and the other blocks
keep the furnace's screen until milestones 32 and 33.

## Gauge screens (milestone 32)

Milestone 31's screen code now works from one `layout` per block (slots, gauges, power bar, progress
bar, at the positions of Mekanism's tile and `Gui*` classes), and more blocks get Mekanism's screen:

- **Chemical Oxidizer**: item (26, 36), output gauge (131, 13), horizontal power bar (115, 75), large
  progress arrow (64, 40).
- **Chemical Dissolution Chamber**: item (28, 36), sulfuric acid gauge (7, 4), output gauge (131, 13),
  horizontal power bar, large progress arrow.
- **Chemical Crystallizer**: input gauge (7, 4), output slot (129, 57), vertical power bar (157, 23),
  large progress arrow (53, 61).
- **Electrolytic Separator**: bucket slot (26, 35) with the empty buckets below it, fluid gauge (5, 10),
  two small gauges (58, 18) and (100, 18), vertical power bar (164, 15).
- **Energy Cubes**: the wide energy gauge (55, 18) between the discharge slot (17, 35) and the charge
  slot (143, 35).

Gauges are Mekanism's frame (`gauge/normal`) and overlay sprites, filled from the bottom in the
chemical's colour. Their tooltip names the chemical with its amount and capacity. Each gauge's tank
goes into the menu's data after the energy (`screen_data`): amount and capacity in 15-bit pieces, and
the chemical's index in `CHEMICALS`.

Simplified: the fill is a flat colour, not the chemical's animated texture. There are no gas dump
buttons on the separator and no item slots for chemical items. Generators, tanks and multiblocks come
next.

## M33: more machine screens

The screens from M31 and M32 now cover the generators and the remaining machines:

- Heat generator: the lava gauge, the fuel and charge slots and the power bar.
- Gas-burning generator: the fuel gauge. Wind and advanced solar generators: the charge slot and the power bar.
- Metallurgic infuser: the infusion bar, its three slots and the progress arrow.
- Purification chamber and osmium compressor: the chemical bar beside the input.
- Rotary condensentrator: the chemical and fluid gauges and the bucket slots.
- Thermal evaporation controller: the water gauge (its capacity follows the tower's height) and the brine gauge.
- Digital miner: the energy slot and the power bar.

Every screen's power bar now reads the stored energy and capacity in FE, sent as 15-bit pieces, so
generators show their real charge. Blocks with no power hide the energy tab.

## M34: tank and multiblock screens

Chemical and fluid tanks, and the ports of the multiblocks, now open a screen on a plain right click
with an empty hand:

- Tanks: one gauge with the chemical or fluid and its amount.
- Fission Reactor port: fuel, water, steam and waste gauges, the status (off, active, SCRAM) and burn
  rate, and two buttons: On/Off and Burn+.
- Fusion Reactor port: deuterium, tritium, water and steam gauges, burning or not, injection, plasma
  temperature, production and stored energy, and two buttons: Ignite and Inject+.
- Thermoelectric Boiler valve: water and steam gauges, the temperature, the boil rate and the
  superheaters.
- Industrial Turbine valve: the steam gauge, the power bar, the production and stored energy.
- Induction Matrix port: the energy gauge and the matrix's maximum input/output.

Every port shows its multiblock's master. A sneaking right click with an empty hand keeps the old
controls: it switches the fission reactor on or off, ignites the fusion reactor, and shows the
turbine's and the boiler's state in the action bar. Clicking the fusion controller still steps the
injection rate.

## M35: the Supercritical Phase Shifter

The SPS turns polonium into antimatter (`SPSMultiblockData`, `SPSValidator`, Mekanism's default config):

- **Structure.** A 7×7×7 box of SPS casing and SPS ports, its faces following `ALLOWED_GRID`, so the
  corners are cut. Inside it holds air and Supercharged Coils, and every coil sits against a port.
- **Ports.** A port takes polonium from tubes and energy from cables in input mode, and gives
  antimatter in output mode. Switch the mode with a sneaking right click while your hand is empty.
- **Process.** A port with a coil behind it hands the coils all the energy it holds, up to an int's
  worth a tick. Each 1,000,000 FE processes 1 mB of polonium, and 1,000 mB make 1 mB of antimatter.
  The input tank holds 2,000 mB and the output tank 1,000 mB.
- **Screen.** It shows the polonium and antimatter gauges, the coils, the progress, the polonium
  processed a tick and the energy the coils took.
- **Polonium.** The Solar Neutron Activator now makes polonium from nuclear waste, 10 mB into 1 mB
  (`processing/lategame/polonium.json`).
- **Antimatter pellets.** The chemical crystallizer already turns 1,000 mB of antimatter into a
  pellet.

Left out: the SPS's damage to entities inside it, and its laser effects.

## M36: free runners

The free runners and the armored free runners are worn on the feet. The armored pair keeps the iron
boots' armour; the plain pair gives none. They hold 64 kFE and charge at 320 FE/t in any charge slot
(`GearConfig` defaults).

- **Modes.** A sneaking right click with the runners in hand cycles them, and the tooltip shows the
  current one (`FreeRunnerMode`):
  - Regular: no fall damage and step assist.
  - Safety: no fall damage.
  - Disabled.
- **Step assist.** In Regular mode the runners add 0.5 to the player's step height while they are not
  sneaking (`CommonPlayerTickHandler.getStepBoost`), so they walk up full blocks.
- **Fall protection.** While a player falls in runners that guard falls and hold energy, their client
  tells the server once a tick. The server then pays for the fall so far from the runners, at 50 FE a
  block (`fallEnergyCost`), and resets the fall distance. When the energy runs out, the fall counts
  again.

Mekanism instead charges once, on landing, for the blocks past the safe distance. Charging every tick
here costs a little more, and the last tick before a very fast landing can still count.

## M37: the Dynamic Tank

The Dynamic Tank (`TankValidator`, `TankMultiblockData`) is a box of dynamic tank casing, dynamic
valves and structural glass, 3 to 18 blocks on each side, with air inside. Its edges must be casing.

- **Capacity.** The tank holds one fluid or one chemical, 16,000 buckets for each block of the whole
  box (`mBPerTank`). A 3×3×3 tank holds 432,000 buckets.
- **Valves.** Pipes and tubes fill a valve in input mode. Each valve keeps up to 256 buckets a tick,
  and the first valve takes that into the tank. A sneaking right click with an empty hand switches a
  valve to output, and it then gives out to the pipes and tubes beside it.
- **Screen.** A plain right click opens the tank's screen, which shows the gauge and the volume.

Mekanism's valves don't push anything out themselves. Here the output mode stands in for the pumps
and pipes that pull from them in Mekanism, because Horizon's pipes only push.

## M38: the Antiprotonic Nucleosynthesizer

The Antiprotonic Nucleosynthesizer (`TileEntityAntiprotonicNucleosynthesizer`) runs Mekanism's 23
`nucleosynthesizing` recipes, for example coal and 4 mB of antimatter into a diamond over 1,000 ticks.
`tools/gen_recipe_table.py` now writes them to the `NUCLEOSYNTHESIZING` table, each with its own
duration.

- **Slots and antimatter.** The item goes in the first slot. Antimatter pellets in the middle slot
  fill the 10,000 mB antimatter tank (`chemical_conversion`), and tubes can fill it with antimatter
  too.
- **Energy.** It uses 15 FE a tick (`UsageConfig`) and spends the recipe's antimatter once, when the
  operation finishes (`per_tick_usage` false).
- **Screen.** It uses the advanced machines' layout, with the antimatter gauge on the left.
