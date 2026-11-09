#!/usr/bin/env python3
"""Build Mekanism's content mod (`.hmod`) for Horizon from this repository's own data.

A content mod carries data and assets, no code (Horizon's docs/mods.md, "Content mods (API 0.2)"). This
script reads the generated data (`src/datagen/generated/<ns>/`) and the hand-written resources
(`src/<module>/resources/`) of all four modules and writes:

- `assets/<ns>/` and `data/<ns>/` as they are (blockstates, models, textures, lang, recipes, loot, tags);
- additions to vanilla and convention tags (`data/minecraft/tags/`, `data/c/tags/`), the only files
  outside its own namespaces a content mod may hold;
- one block declaration per blockstate file (`data/<ns>/horizon/block/<name>.json`), copying a vanilla
  template chosen from what the block is;
- one item declaration per item definition that is not a block's item;
- Mekanism's ore features rewritten as vanilla `minecraft:ore` features with the default sizes from
  `OreType.java`, since Horizon has no `mekanism:ore` feature type.

Horizon declares a content mod's blocks and items only under its own id, so each of Mekanism's four
modules becomes its own content mod, named after its namespace, as the Java build registers them.
Everything else (other mods' namespaces: create, projecte, framedblocks...; NeoForge's own files) is
left out, and the report says how much. Output: `target/content/<ns>/` (each unpacked mod) and
`target/content/<ns>-<version>.hmod`.
"""
import argparse
import json
import shutil
import sys
import zipfile

from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import block_shapes
import bounding  # noqa: E402
import flatten_models  # noqa: E402
import obj_models  # noqa: E402
import worn_models  # noqa: E402

ROOT = Path(__file__).resolve().parents[1]
SRC = ROOT / "src"
VERSION = "0.1.0"
NAMESPACES = ["mekanism", "mekanismgenerators", "mekanismadditions", "mekanismtools"]
MODULE_OF = {"mekanism": "main", "mekanismgenerators": "generators",
             "mekanismadditions": "additions", "mekanismtools": "tools"}
# Shared namespaces a content mod may add tags to (docs/mods.md, "The fence").
TAG_NAMESPACES = ["minecraft", "c"]

# Blocks whose Java class has a block entity and a facing/active state: the furnace template gives them
# a block entity, a menu and the same `facing` property (its `lit` stands in for `active`).
MACHINES = {
    "enrichment_chamber", "crusher", "energized_smelter", "osmium_compressor", "combiner",
    "purification_chamber", "chemical_injection_chamber", "metallurgic_infuser", "precision_sawmill",
    "chemical_crystallizer", "chemical_dissolution_chamber", "chemical_infuser", "chemical_oxidizer",
    "chemical_washer", "electrolytic_separator", "isotopic_centrifuge", "pressurized_reaction_chamber",
    "rotary_condensentrator", "antiprotonic_nucleosynthesizer", "pigment_extractor", "pigment_mixer",
    "painting_machine", "nutritional_liquifier", "solar_neutron_activator", "formulaic_assemblicator",
    "electric_pump", "fluidic_plenisher", "digital_miner", "seismic_vibrator", "resistive_heater",
    "fuelwood_heater", "laser", "laser_amplifier", "laser_tractor_beam", "logistical_sorter",
    "oredictionificator", "modification_station", "dimensional_stabilizer", "quantum_entangloporter",
    "teleporter", "security_desk", "qio_dashboard", "qio_drive_array", "qio_importer", "qio_exporter",
    "qio_redstone_adapter", "chargepad", "industrial_alarm", "personal_barrel",
    # Generators
    "heat_generator", "solar_generator", "advanced_solar_generator", "gas_burning_generator",
    "bio_generator", "wind_generator",
}
TIERED_MACHINES = ("_factory", "_energy_cube", "_bin", "_chemical_tank", "_fluid_tank", "_induction_cell",
                   "_induction_provider")
TRANSMITTERS = ("_universal_cable", "_mechanical_pipe", "_pressurized_tube", "_logistical_transporter",
                "_thermodynamic_conductor", "restrictive_transporter", "diversion_transporter")
TOOL_TEMPLATES = [("_paxel", "minecraft:iron_pickaxe"), ("_pickaxe", "minecraft:iron_pickaxe"),
                  ("_axe", "minecraft:iron_axe"), ("_shovel", "minecraft:iron_shovel"),
                  ("_hoe", "minecraft:iron_hoe"), ("_sword", "minecraft:iron_sword"),
                  ("_helmet", "minecraft:iron_helmet"), ("_chestplate", "minecraft:iron_chestplate"),
                  ("_leggings", "minecraft:iron_leggings"), ("_boots", "minecraft:iron_boots"),
                  ("_shield", "minecraft:shield"), ("_bucket", "minecraft:water_bucket")]

# Transmitters copy the mushroom stem: its six direction properties carry the connections, `false` where a
# side joins (a stem's face is false where the same block touches it, so the template's own updates agree),
# and the bundle sets the sides that join a machine. `occludes: false` (Horizon content mods) makes it
# see-through, and `shapes` its centre box.
PIPE_TEMPLATE = "minecraft:mushroom_stem"
# Centre box per size, in pixels (transmitter_small.obj and transmitter_large.obj).
PIPE_SIZES = {"small": (5, 11), "large": (4, 12)}
DIRECTIONS = ("north", "east", "south", "west", "up", "down")
# The arm model points north; the blockstate turns it to each side.
ARM_ROTATION = {"north": {}, "east": {"y": 90}, "south": {"y": 180}, "west": {"y": 270}, "up": {"x": 270},
                "down": {"x": 90}}


def pipe_declaration(size):
    lo, hi = (v / 16 for v in PIPE_SIZES[size])
    box = [[lo, lo, lo, hi, hi, hi]]
    return {"template": PIPE_TEMPLATE,
            "properties": {"occludes": False, "hardness": 1.0, "sounds": "minecraft:iron_bars"},
            "shapes": {"property": "up", "values": {"true": box, "false": box}}}


def pipe_models(ns, out, name, blockstate, report=None):
    """Vanilla element models for a transmitter whose blockstate uses Mekanism's OBJ model, taken from the OBJ
    itself: per side the arm of `<side>NORMAL` and the centre face of `<side>NONE`, as a multipart blockstate.
    Returns its size, or None."""
    try:
        model_id = blockstate["variants"][""]["model"]
    except (KeyError, TypeError):
        return None
    size = "large" if "/large/" in model_id else "small"
    models = out / "assets" / ns / "models"
    model = json.loads((models / (model_id.split(":", 1)[1] + ".json")).read_text())
    textures = model.get("textures", {})
    center = textures.get("center_opaque") or textures.get("center_down")
    side = textures.get("side_opaque") or textures.get("side")
    if not center or not side:
        return None
    obj_path = next(iter(SRC.glob(f"**/models/block/transmitter_{size}.obj.mek")), None)
    if obj_path is None:
        return None
    mtl_path = obj_path.with_name(f"transmitter_{size}.mtl")
    text, mtl_text = obj_path.read_text(), mtl_path.read_text()
    mtl = obj_models.parse_mtl(mtl_text)
    lo, hi = (v / 16 for v in PIPE_SIZES[size])
    arms, caps = {}, {}
    half = (hi - lo) / 2
    for obj in obj_models.parse(text):
        for d in DIRECTIONS:
            if obj["name"] == d + "NORMAL":
                el, why = obj_models.element(obj, True, (0.0, 0.0, 0.0), mtl)
                if el is not None:
                    arms[d] = el
            elif obj["name"] == d + "NONE":
                quad = obj["faces"][0]
                local = [((p[0] - 0.5, p[1] - 0.5, p[2] - 0.5), uv) for p, uv in quad]
                face = obj_models.face_of(d, local, (-half,) * 3, (half,) * 3, True)
                if face is not None:
                    if face.get("rotation") == 0:
                        face.pop("rotation")
                    var = mtl.get(obj["materials"][0], ("#" + str(obj["materials"][0]), False))[0]
                    face["texture"] = var
                    caps[d] = {"from": [lo * 16] * 3, "to": [hi * 16] * 3, "faces": {d: face}}
    if set(arms) != set(DIRECTIONS) or set(caps) != set(DIRECTIONS):
        return None

    def remap(el):
        for f in el["faces"].values():
            f["texture"] = "#center" if f["texture"].startswith("#center") else "#side"
        return el

    # The push and pull arms (`<side>PUSH`, `<side>PULL`) are not boxes: the game draws their quads as world models
    # (north's, turned to each side by the bundle) where a side is set to push or pull.
    side_png = next(iter(SRC.glob(f"**/textures/{side.split(':', 1)[1]}.png")), None)
    if side_png is not None:
        for mode in ("PUSH", "PULL"):
            obj = next((o for o in obj_models.parse(text) if o["name"] == "north" + mode), None)
            if obj is None:
                continue
            quads = [([(p[0] - 0.5, p[1] - 0.5, p[2] - 0.5) for p, _ in quad], [(uv[0], 1 - uv[1]) for _, uv in quad])
                     for quad in obj["faces"] if len(quad) == 4]
            worn_models.build_quads_world_model(ns, f"{name}_{mode.lower()}", quads, side_png, out, report if report is not None else {})

    tex = {"particle": center, "center": center, "side": side}
    base = f"{ns}:block/transmitter/horizon/{name}"
    for d in DIRECTIONS:
        write_json(models / "block" / "transmitter" / "horizon" / f"{name}_arm_{d}.json",
                   {"textures": tex, "elements": [remap(arms[d])]})
        write_json(models / "block" / "transmitter" / "horizon" / f"{name}_cap_{d}.json",
                   {"textures": tex, "elements": [remap(caps[d])]})
    parts = []
    for d in DIRECTIONS:
        parts.append({"when": {d: "false"}, "apply": {"model": f"{base}_arm_{d}"}})
        parts.append({"when": {d: "true"}, "apply": {"model": f"{base}_cap_{d}"}})
    write_json(out / "assets" / ns / "blockstates" / f"{name}.json", {"multipart": parts})
    # The item shows the bare centre, as Mekanism's item model does.
    write_json(models / "block" / "transmitter" / "horizon" / f"{name}_center.json",
               {"parent": "block/block", "textures": tex, "elements": [remap(json.loads(json.dumps(caps[d]))) for d in DIRECTIONS]})
    write_json(models / "item" / f"{name}.json", {"parent": f"{base}_center"})
    return size


# Items that hold energy in custom_data (mekanism:energy) stack to one, as Mekanism's do.
SINGLE_ITEMS = {"energy_tablet", "atomic_disassembler", "electric_bow", "portable_teleporter", "free_runners", "free_runners_armored",
                "meka_tool", "seismic_reader", "qio_drive_base", "qio_drive_hyper_dense", "qio_drive_time_dilating", "qio_drive_supermassive"}
# ItemAtomicDisassembler's Tool: no drops from #mekanism:incorrect_for_disassembler, drops from every other
# block (vanilla's tool rules take tags, so "every block" is the four mineable tags). Its speed comes from
# the bundle (native_destroy_speed), from the energy it holds.
DISASSEMBLER_TOOL = {
    "rules": [{"blocks": "#mekanism:incorrect_for_disassembler", "correct_for_drops": False}]
    + [{"blocks": f"#minecraft:mineable/{t}", "correct_for_drops": True} for t in ("pickaxe", "axe", "shovel", "hoe")],
    "default_mining_speed": 1.0,
}


_SHAPES = None
FACING4 = ("north", "east", "south", "west")
ALL6 = FACING4 + ("up", "down")
SIX_FACING_TEMPLATE = "minecraft:barrel"


def faces_vertically(blockstate):
    return any("facing=up" in key or "facing=down" in key for key in blockstate.get("variants", {}))
# Blocks Mekanism registers with noOcclusion(): they hide nothing behind them.
SEE_THROUGH = ("structural_glass", "reactor_glass", "laser_focus_matrix", "nutritional_liquifier")


def shape_table():
    """Mekanism's shapes read from the Java source, turned to the orientation of each block's model."""
    global _SHAPES
    if _SHAPES is None:
        _SHAPES = block_shapes.collect(SRC)
        block_shapes.fit(_SHAPES, block_shapes.assets_of(ROOT), [])
    return _SHAPES


def see_through(name):
    return name in SEE_THROUGH or name.endswith("glass") or "_transparent" in name or "_glass_" in name


def shape_declaration(ns, name, declaration, report):
    """`declaration` with the block's outline and collision shape from Mekanism's Java, and `occludes: false` where
    Mekanism calls noOcclusion()."""
    template = declaration["template"]
    shape = shape_table().get(ns, {}).get(name)
    if shape and "shapes" not in declaration and template != "minecraft:lever":
        if "all" in shape:
            declaration["shape"] = shape["all"]
        elif template == SIX_FACING_TEMPLATE and all(d in shape["facing"] for d in ALL6):
            declaration["shapes"] = {"property": "facing", "values": {d: shape["facing"][d] for d in ALL6}}
        elif template == "minecraft:furnace":
            declaration["shapes"] = {"property": "facing", "values": {d: shape["facing"][d] for d in FACING4}}
        elif template == "minecraft:amethyst_cluster":
            declaration["shapes"] = {"property": "facing", "values": shape["facing"]}
        else:
            # A template without a facing: the shape of the model the blockstate shows.
            declaration["shape"] = shape["facing"]["north"]
        report["shapes"] = report.get("shapes", 0) + 1
    if name == "laser_amplifier":
        # Its redstone output is graded (0 to 15): the bundle sets it with native_set_block_signal.
        declaration.setdefault("properties", {})["signal_source"] = True
    if see_through(name):
        declaration.setdefault("properties", {})["occludes"] = False
        report["see_through"] = report.get("see_through", 0) + 1
    return declaration


def block_template(name, blockstate):
    """The vanilla block a Mekanism block copies, from what it is."""
    states = blockstate_properties(blockstate)
    if name.endswith("_ore"):
        return "minecraft:deepslate_iron_ore" if name.startswith("deepslate_") else "minecraft:iron_ore"
    if name == "qio_redstone_adapter":
        # A flat disc on the face it was placed on that powers its neighbours: a lever, which the bundle
        # switches (its `powered`) instead of the player.
        return "minecraft:lever"
    if name in MACHINES or name.endswith(TIERED_MACHINES):
        # No vanilla block with a block entity has no state at all; these wait for their milestone.
        return "minecraft:furnace" if "facing" in states else "minecraft:iron_block"
    if name.endswith(TRANSMITTERS):
        return PIPE_TEMPLATE
    if "glass" in name:
        return "minecraft:glass"
    if name.endswith("_slab"):
        return "minecraft:stone_slab"
    if name.endswith("_stairs"):
        return "minecraft:stone_stairs"
    if name.endswith("_fence"):
        return "minecraft:oak_fence"
    if name.endswith("_fence_gate"):
        return "minecraft:oak_fence_gate"
    if name.endswith("_glow_panel") or name.endswith("glow_panel"):
        return "minecraft:glowstone"
    if "plastic" in name or name.endswith("_road"):
        return "minecraft:stone"
    if "facing" in states:
        return "minecraft:furnace"
    return "minecraft:iron_block"


def lever_blockstate(name, blockstate, ns):
    """Mekanism's six `facing` variants as a lever's `face`, `facing` and `powered`: on the floor it
    faces up, on the ceiling down, on a wall away from it; powered shows the lit model."""
    out = {}
    for face in ("floor", "wall", "ceiling"):
        for facing in ("north", "south", "west", "east"):
            mek = {"floor": "up", "ceiling": "down"}.get(face, facing)
            base = blockstate["variants"][f"active=false,facing={mek}"]
            for powered in ("false", "true"):
                variant = {k: v for k, v in base.items() if k in ("x", "y")}
                variant["model"] = f"{ns}:block/{name}_{'lit' if powered == 'true' else 'unlit'}"
                out[f"face={face},facing={facing},powered={powered}"] = variant
    return {"variants": out}


def blockstate_properties(blockstate):
    props = set()
    for key in blockstate.get("variants", {}):
        for part in key.split(","):
            if "=" in part:
                props.add(part.split("=")[0])
    for part in blockstate.get("multipart", []):
        when = part.get("when", {})
        for clause in when.get("OR", [when]):
            props.update(k for k in clause if k != "OR")
    return props


def rename_property(blockstate, old, new):
    """A blockstate definition with state property `old` called `new`."""
    def key(variant):
        return ",".join(new + part[len(old):] if part.startswith(old + "=") else part
                        for part in variant.split(","))
    if "variants" in blockstate:
        blockstate["variants"] = {key(k): v for k, v in blockstate["variants"].items()}
    for part in blockstate.get("multipart", []):
        when = part.get("when")
        if when:
            for clause in when.get("OR", [when]):
                if old in clause:
                    clause[new] = clause.pop(old)
    return blockstate


def item_template(name):
    for suffix, template in TOOL_TEMPLATES:
        if name.endswith(suffix):
            return template
    return "minecraft:iron_ingot"


HEIGHT = {"absolute": "absolute", "aboveBottom": "above_bottom", "belowTop": "below_top"}
# OreType.java: (perChunk, discardChanceOnAirExposure, maxVeinSize, shape, min, max, plateau)
ORES = {
    "tin_small": (14, 0, 4, "trapezoid", ("absolute", -20), ("absolute", 94), 0),
    "tin_large": (12, 0, 9, "trapezoid", ("absolute", -32), ("absolute", 72), 0),
    "osmium_upper": (65, 0, 7, "trapezoid", ("absolute", 72), ("belowTop", -24), 8),
    "osmium_middle": (6, 0, 9, "trapezoid", ("absolute", -32), ("absolute", 56), 0),
    "osmium_small": (8, 0, 4, "uniform", ("aboveBottom", 0), ("absolute", 64), 0),
    "uranium_small": (4, 0, 4, "trapezoid", ("aboveBottom", 0), ("absolute", 8), 0),
    "uranium_buried": (7, 0.75, 9, "trapezoid", ("aboveBottom", -24), ("aboveBottom", 56), 16),
    "fluorite_normal": (5, 0, 5, "uniform", ("aboveBottom", 0), ("absolute", 23), 0),
    "fluorite_buried": (3, 1, 13, "trapezoid", ("aboveBottom", 0), ("absolute", 4), 0),
    "lead_normal": (8, 0.25, 9, "trapezoid", ("aboveBottom", -24), ("absolute", 64), 0),
}


def anchor(a):
    kind, value = a
    return {HEIGHT[kind]: value}


def vanilla_ore(name, feature):
    """`mekanism:ore` → `minecraft:ore`, and its placed feature with vanilla count and height."""
    per_chunk, discard, size, shape, lo, hi, plateau = ORES[name]
    configured = {"type": "minecraft:ore", "config": {
        "targets": feature["targets"], "size": size, "discard_chance_on_air_exposure": discard}}
    height = {"type": f"minecraft:{shape}", "min_inclusive": anchor(lo), "max_inclusive": anchor(hi)}
    if shape == "trapezoid" and plateau:
        height["plateau"] = plateau
    placed = {"feature": f"mekanism:ore_{name}", "placement": [
        {"type": "minecraft:count", "count": per_chunk},
        {"type": "minecraft:in_square"},
        {"type": "minecraft:height_range", "height": height},
        {"type": "minecraft:biome"}]}
    return configured, placed


def vanilla_salt():
    """`mekanism:disk` -> `minecraft:disk` with SaltConfig's defaults (radius 2 to 3, half height 1, 2 per chunk)."""
    feature = {"type": "minecraft:disk", "config": {
        "state_provider": {"type": "minecraft:simple_state_provider", "state": {"Name": "mekanism:block_salt"}},
        "target": {"predicate_type": "minecraft:matching_blocks", "blocks": ["minecraft:dirt", "minecraft:clay"]},
        "radius": {"type": "minecraft:uniform", "min_inclusive": 2, "max_inclusive": 3},
        "half_height": 1}}
    placed = {"feature": "mekanism:salt", "placement": [
        {"type": "minecraft:count", "count": 2},
        {"type": "minecraft:in_square"},
        {"type": "minecraft:heightmap", "heightmap": "OCEAN_FLOOR_WG"},
        {"type": "minecraft:block_predicate_filter", "predicate": {"type": "minecraft:matching_fluids", "fluids": "minecraft:water"}},
        {"type": "minecraft:biome"}]}
    return feature, placed


def copy_tree(src, dst, report, key):
    if not src.is_dir():
        return
    for path in src.rglob("*"):
        if path.is_file():
            rel = path.relative_to(src).as_posix()
            if rel != rel.lower():
                # Resource paths are lowercase (vanilla's ResourceLocation rules). Language files keep
                # their content under the lowercase code (ry_UA -> ry_ua); anything else is left out.
                if not rel.startswith("lang/"):
                    report["left_out_uppercase"] = report.get("left_out_uppercase", 0) + 1
                    continue
                rel = rel.lower()
            out = dst / rel
            out.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(path, out)
            report[key] = report.get(key, 0) + 1


DESCRIPTIONS = {
    "mekanism": "Mekanism's blocks, items, recipes and ores. Machines need the Mekanism native mod.",
    "mekanismgenerators": "Mekanism: Generators' blocks, items and recipes.",
    "mekanismadditions": "Mekanism: Additions' plastic blocks, glow panels and other blocks and items.",
    "mekanismtools": "Mekanism: Tools' armour and tool sets.",
}


def build_module(ns, out):
    """One content mod: the namespace's assets and data, its tags, and its declarations."""
    shutil.rmtree(out, ignore_errors=True)
    report, skipped = {}, {}
    module = MODULE_OF[ns]
    for base in (SRC / module / "resources", SRC / "datagen" / "generated" / ns):
        copy_tree(base / "assets" / ns, out / "assets" / ns, report, "assets")
        data = base / "data"
        if not data.is_dir():
            continue
        for owner in sorted(p.name for p in data.iterdir() if p.is_dir()):
            if owner == ns:
                copy_tree(data / owner, out / "data" / owner, report, "data")
            elif owner in TAG_NAMESPACES:
                copy_tree(data / owner / "tags", out / "data" / owner / "tags", report, "tags")
            else:
                skipped[owner] = skipped.get(owner, 0) + sum(1 for p in (data / owner).rglob("*") if p.is_file())

    # NeoForge's convention tags on vanilla items, which Horizon (vanilla) lacks; only the mekanism mod
    # carries them, and only where no carried file defines the tag.
    if ns == "mekanism":
        from convention_tags import TAGS
        for path, values in TAGS.items():
            out_file = out / "data/c/tags/item" / f"{path}.json"
            if not out_file.exists():
                write_json(out_file, {"values": values})
                report["convention_tags"] = report.get("convention_tags", 0) + 1

    # Ores: rewrite the custom feature type; drop retrogen variants (Horizon has no retrogen).
    features = out / "data" / ns / "worldgen/feature"
    placed_dir = out / "data" / ns / "worldgen/placed_feature"
    for path in sorted(features.glob("ore_*.json")) if features.is_dir() else []:
        name = path.stem[len("ore_"):]
        if name.endswith("_retrogen"):
            path.unlink()
            (placed_dir / path.name).unlink(missing_ok=True)
            continue
        configured, placed = vanilla_ore(name, json.loads(path.read_text()))
        path.write_text(json.dumps(configured, indent=2) + "\n")
        (placed_dir / path.name).write_text(json.dumps(placed, indent=2) + "\n")
        report["ores"] = report.get("ores", 0) + 1
    # The salt disk: vanilla's disk feature with the default radius, depth and count. The native mod overrides it
    # with the world config (`mixins/mekanism/core/src/worldgen.rs`).
    if features.is_dir():
        placed_dir.mkdir(parents=True, exist_ok=True)
        salt_feature, salt_placed = vanilla_salt()
        (features / "salt.json").write_text(json.dumps(salt_feature, indent=2) + "\n")
        (placed_dir / "salt.json").write_text(json.dumps(salt_placed, indent=2) + "\n")
        for path in placed_dir.glob("salt_retrogen.json"):
            path.unlink()
        report["salt"] = 1

    # Items told apart by a Mekanism data component (upgrades, modules): Horizon has no mod components, so
    # the type goes in vanilla's custom_data, the model is chosen by custom_model_data, and the name is
    # spelled out with item_name.
    report["gauge_textures"] = gauge_textures(ns, out)
    # NeoForge composite models (factories, jetpack...) become plain element models.
    search = [d for m in MODULE_OF.values() for d in (SRC / m / "resources" / "assets",)]
    search += [SRC / "datagen" / "generated" / n / "assets" for n in NAMESPACES]
    report["composite_models"] = flatten_models.flatten_tree(out / "assets" / ns, search)
    report["sps_textures"] = sps_textures(ns, out, search)
    report["sounds"] = sound_files(ns, out)
    report["hud_sprites"] = hud_sprites(ns, out)
    if ns == "mekanismgenerators":
        # The turbine's blades (`ModelTurbine`): one pair of rings per blade index.
        turbine_png = next((d / ns / "render/turbine.png" for d in search if (d / ns / "render/turbine.png").is_file()), None)
        if turbine_png is not None:
            for index in range(28):
                worn_models.build_turbine_blade(ns, index, turbine_png, out, report)
        # The rotor spins, so the native code draws it (`native_world_models`): the OBJ as a world model.
        rotor = next((d / ns / "models/block/wind_generator_blades.obj" for d in search
                      if (d / ns / "models/block/wind_generator_blades.obj").is_file()), None)
        if rotor is not None:
            worn_models.build_world_model(ns, "wind_generator_rotor", rotor, "mekanismgenerators:block/model/wind_generator",
                                          out, search, report)
    if ns == "mekanism":
        # The energy cube's core (`RenderEnergyCube`): the 16 px `energy_core` cube at 0.4 scale.
        core = next((d / ns / "textures/block/energy_core.png" for d in search
                     if (d / ns / "textures/block/energy_core.png").is_file()), None)
        if core is not None:
            worn_models.build_cube_world_model(ns, "energy_core", 0.2, core, out, report)
            # The fusion reactor's three cores are the same cube at full size (`RenderFusionReactor`).
            worn_models.build_cube_world_model(ns, "fusion_core", 0.5, core, out, report)
    if ns == "mekanism":
        # The liquifier's blade turns, so the native code draws it (`RenderNutritionalLiquifier`).
        blade = next((d / ns / "models/block/liquifier_blade.json" for d in search
                      if (d / ns / "models/block/liquifier_blade.json").is_file()), None)
        texture = next((d / ns / "textures/block/nutritional_liquifier.png" for d in search
                        if (d / ns / "textures/block/nutritional_liquifier.png").is_file()), None)
        if blade is not None and texture is not None:
            worn_models.build_element_world_model(ns, "liquifier_blade", json.loads(blade.read_text()), texture, out, report)
    if ns == "mekanism":
        # The Robit (`RenderRobit`): its element model, the glowing eyes and back light apart from the body.
        # The Robit item (`RenderRobitItem`) draws its base skin model; Horizon has no such special model, so the item
        # shows the same elements as a plain item model with the base texture.
        robit_model = next((d / ns / "models/robit/robit.json" for d in search if (d / ns / "models/robit/robit.json").is_file()), None)
        robit_skin = next((d / ns / "textures/entity/robit/robit.png" for d in search
                           if (d / ns / "textures/entity/robit/robit.png").is_file()), None)
        if robit_model is not None and robit_skin is not None:
            item_model = json.loads(robit_model.read_text())
            item_model["textures"] = {"robit": f"{ns}:item/robit_skin", "particle": f"{ns}:item/robit_skin"}
            write_json(out / "assets" / ns / "models" / "item" / "robit_flat.json", item_model)
            (out / "assets" / ns / "textures" / "item").mkdir(parents=True, exist_ok=True)
            shutil.copyfile(robit_skin, out / "assets" / ns / "textures" / "item" / "robit_skin.png")
            write_json(out / "assets" / ns / "items" / "robit.json", {"model": {"type": "minecraft:model", "model": f"{ns}:item/robit_flat"}})
            report["robit_item"] = 1
        # The skins (`MekanismRobitSkins`, their datapack JSON): per skin and texture the body and the glowing parts,
        # `robit_body_<skin>[_<n>]` and `robit_glow_<skin>[_<n>]` (the Robit in the world, the previews in its screen);
        # a skin's `custom_model` replaces the base model. The texture index moves while the Robit walks.
        body_glow = {}
        for skin_json in sorted({f for d in search for f in (d.parent / "data" / ns / "mekanism" / "robit_skin").glob("*.json")}, key=lambda f: f.name):
            skin_data = json.loads(skin_json.read_text())
            model_id = skin_data.get("custom_model", f"{ns}:robit/robit")
            model_ns, model_path = model_id.split(":")
            model_file = next((d / model_ns / "models" / f"{model_path}.json" for d in search
                               if (d / model_ns / "models" / f"{model_path}.json").is_file()), None)
            if model_file is None:
                continue
            elements = json.loads(model_file.read_text())["elements"]
            for index, texture_id in enumerate(skin_data.get("textures", [])):
                tex_ns, tex_name = texture_id.split(":")
                png = next((d / tex_ns / "textures/entity/robit" / f"{tex_name}.png" for d in search
                            if (d / tex_ns / "textures/entity/robit" / f"{tex_name}.png").is_file()), None)
                if png is None:
                    continue
                suffix = "" if index == 0 else f"_{index}"
                for part_name, glow in (("body", False), ("glow", True)):
                    part = {"elements": [e for e in elements if bool(e.get("light_emission")) == glow]}
                    worn_models.build_element_world_model(ns, f"robit_{part_name}_{skin_json.stem}{suffix}", part, png, out, report, (8, 0, 8))
    if ns == "mekanism":
        # The skin previews (`GuiRobitSkinSelectScroll`) turn each skin's body, one model per texture of the skin.
        robit = next((d / ns / "models/robit/robit.json" for d in search if (d / ns / "models/robit/robit.json").is_file()), None)
        if robit is not None:
            body = {"elements": [e for e in json.loads(robit.read_text())["elements"] if not e.get("light_emission")]}
            for skin_json in sorted({f for d in search for f in (d.parent / "data" / ns / "mekanism" / "robit_skin").glob("*.json")}, key=lambda f: f.name):
                for index, texture_id in enumerate(json.loads(skin_json.read_text()).get("textures", [])):
                    tex_ns, tex_name = texture_id.split(":")
                    png = next((d / tex_ns / "textures/entity/robit" / f"{tex_name}.png" for d in search
                                if (d / tex_ns / "textures/entity/robit" / f"{tex_name}.png").is_file()), None)
                    if png is not None:
                        suffix = "" if index == 0 else f"_{index}"
                        worn_models.build_element_world_model(ns, f"robit_preview_{skin_json.stem}{suffix}", body, png, out, report, (8, 0, 8))
    if ns == "mekanism":
        # The Personal Chest (`RenderPersonalChest`): vanilla's chest parts in Mekanism's skin, the lid turned.
        chest = next((d / ns / "textures/block/models/personal_chest.png" for d in search
                      if (d / ns / "textures/block/models/personal_chest.png").is_file()), None)
        if chest is not None:
            worn_models.build_vanilla_world_model(ns, "personal_chest", "minecraft:chest#main", (-8, 0, -8), chest, out, report)
    if ns == "mekanism":
        # The Industrial Alarm's bulb (`RenderIndustrialAlarm`), centred on its axis.
        bulb = next((d / ns / "models/block/industrial_alarm_active_bulb.json" for d in search
                     if (d / ns / "models/block/industrial_alarm_active_bulb.json").is_file()), None)
        tex = next((d / ns / "textures/block/models/industrial_alarm_active.png" for d in search
                    if (d / ns / "textures/block/models/industrial_alarm_active.png").is_file()), None)
        if bulb is not None and tex is not None:
            worn_models.build_element_world_model(ns, "alarm_bulb", json.loads(bulb.read_text()), tex, out, report, (0, 0, 0))
    if ns == "mekanism":
        # Moving parts of machines (`RenderSeismicVibrator`, `RenderPigmentMixer`): their shafts are world models.
        for model_name, name, center in (("vibrator_shaft", "vibrator_shaft", (8, 8, 8)),
                                         ("pigment_mixer_shaft", "pigment_mixer_shaft", (1, 0, 1))):
            f = next((d / ns / "models/block" / f"{model_name}.json" for d in search
                      if (d / ns / "models/block" / f"{model_name}.json").is_file()), None)
            if f is None:
                continue
            data = json.loads(f.read_text())
            tex = next(v for k, v in data["textures"].items() if k != "particle")
            png = next((d / tex.split(":")[0] / "textures" / (tex.split(":")[1] + ".png") for d in search
                        if (d / tex.split(":")[0] / "textures" / (tex.split(":")[1] + ".png")).is_file()), None)
            if png is not None:
                worn_models.build_element_world_model(ns, name, data, png, out, report, center)
    worn = set()
    if ns == "mekanism":
        for item in worn_models.WORN:
            if worn_models.build(ns, item, search, out, search, report):
                worn.add(item)
        for item in worn_models.MEKASUIT:
            if worn_models.build_mekasuit(ns, item, search, out, search, report):
                worn.add(item)
    report["component_items"] = component_items(ns, out)
    report["component_recipes"] = component_recipes(out / "data" / ns / "recipe")

    # Declarations: every blockstate is a block; every item definition without a block is an item.
    declared = out / "data" / ns / "horizon"
    blocks = set()
    for path in sorted((out / "assets" / ns / "blockstates").glob("*.json")):
        name = path.stem
        if name == "bounding_block":
            continue  # Mekanism's invisible helper block for multi-block machines: native code
        blocks.add(name)
        blockstate = json.loads(path.read_text())
        template = block_template(name, blockstate)
        if template == "minecraft:furnace" and faces_vertically(blockstate):
            # Up and down need six facings: the barrel has them (and `open` where the furnace has `lit`).
            template = SIX_FACING_TEMPLATE
            report["six_facings"] = report.get("six_facings", 0) + 1
        if template in ("minecraft:furnace", SIX_FACING_TEMPLATE):
            # The template's `lit` (a barrel's `open`) stands in for Mekanism's `active`: same meaning, same two values.
            state = "lit" if template == "minecraft:furnace" else "open"
            path.write_text(json.dumps(rename_property(blockstate, "active", state), indent=2) + "\n")
            report["active_as_lit"] = report.get("active_as_lit", 0) + 1
        if template == "minecraft:lever":
            path.write_text(json.dumps(lever_blockstate(name, blockstate, ns), indent=2) + "\n")
        declaration = {"template": template}
        if template == PIPE_TEMPLATE:
            size = pipe_models(ns, out, name, blockstate, report)
            if size:
                declaration = pipe_declaration(size)
                report["pipes"] = report.get("pipes", 0) + 1
        if name.endswith("_glow_panel"):
            # Six facings (the side it sits on) and a light: the amethyst cluster has both.
            declaration = {"template": "minecraft:amethyst_cluster", "properties": {"light": 15}}
        declaration = shape_declaration(ns, name, declaration, report)
        write_json(declared / "block" / f"{name}.json", declaration)
    # Bounding blocks: the cells beyond a multi-block machine's own (native code places and clears them).
    for (bns, bname), decl in bounding.declarations(shape_table()).items():
        if bns != ns:
            continue
        write_json(out / "assets" / ns / "blockstates" / f"{bname}.json",
                   {"variants": {"": {"model": "mekanism:block/nothing"}}})
        write_json(declared / "block" / f"{bname}.json", decl)
        blocks.add(bname)
        report["bounding_blocks"] = report.get("bounding_blocks", 0) + 1
    items = 0
    for path in sorted((out / "assets" / ns / "items").glob("*.json")):
        if path.stem in blocks:
            continue
        item = {"template": item_template(path.stem)}
        if path.stem in SINGLE_ITEMS:
            item["max_stack_size"] = 1
        if path.stem == "atomic_disassembler":
            item["components"] = {"minecraft:tool": DISASSEMBLER_TOOL}
        # The Meka-Tool mines as the disassembler does and hits with `GearConfig`'s base damage (4) and
        # attack speed (-2.4) in the main hand.
        if path.stem == "meka_tool":
            item["components"] = {"minecraft:tool": DISASSEMBLER_TOOL, "minecraft:attribute_modifiers": [
                {"type": "minecraft:attack_damage", "id": "minecraft:base_attack_damage", "amount": 4.0,
                 "operation": "add_value", "slot": "mainhand"},
                {"type": "minecraft:attack_speed", "id": "minecraft:base_attack_speed", "amount": -2.4,
                 "operation": "add_value", "slot": "mainhand"}]}
        # Jetpacks are worn on the chest: the iron chestplate's slot, without its iron look; the plain
        # jetpack gives no armour (the armored one keeps the chestplate's).
        if path.stem in ("jetpack", "jetpack_armored"):
            item["template"] = "minecraft:iron_chestplate"
            item["components"] = {"minecraft:equippable": {"slot": "chest"}}
            if path.stem == "jetpack":
                item["components"]["minecraft:attribute_modifiers"] = []
        # Free runners are worn on the feet; the plain pair gives no armour, the armored pair keeps the
        # iron boots'.
        if path.stem in ("free_runners", "free_runners_armored"):
            item["template"] = "minecraft:iron_boots"
            item["components"] = {"minecraft:equippable": {"slot": "feet"}}
            if path.stem == "free_runners":
                item["components"]["minecraft:attribute_modifiers"] = []
        # The MekaSuit's armour, toughness and knockback resistance are netherite's (`MekanismStartupConfig`).
        suit = {"mekasuit_helmet": "helmet", "mekasuit_bodyarmor": "chestplate", "mekasuit_pants": "leggings",
                "mekasuit_boots": "boots"}
        if path.stem in suit:
            item["template"] = f"minecraft:netherite_{suit[path.stem]}"
        if path.stem in worn:
            # Worn: the slot, and the equipment asset that draws the item's own model on the wearer.
            equippable = item.setdefault("components", {}).setdefault("minecraft:equippable", {})
            equippable["slot"] = worn_models.SLOT[path.stem]
            equippable["asset_id"] = f"{ns}:{path.stem}"
            if path.stem == "scuba_mask":
                item["template"] = "minecraft:iron_helmet"
                item["components"]["minecraft:attribute_modifiers"] = []
        write_json(declared / "item" / f"{path.stem}.json", item)
        items += 1
    report["blocks"] = len(blocks)
    report["items"] = items

    name = {"mekanism": "Mekanism"}.get(ns, "Mekanism: " + ns[len("mekanism"):].capitalize())
    (out / "horizon-mod.toml").write_text("\n".join([
        f'id = "{ns}"',
        f'name = "{name}"',
        f'version = "{VERSION}"',
        f'description = "{DESCRIPTIONS[ns]}"',
        'authors = ["Aidan C. Brady and the Mekanism contributors", "Horizon port"]',
        'api = "0.2"',
        'side = "both"',
        'capabilities = ["registries"]',
        "",
    ]))
    return report, skipped


# Mekanism's item components that only tell items of one id apart, by the lang key of their values'
# names ("upgrade.mekanism.speed"), to custom_data in carried recipes.
COMPONENT_NAMES = {"mekanism:upgrade_type": "upgrade", "mekanism:module_type": "module"}


def first_frame_png(data, size):
    """An animated strip's first `size`-row frame, as a PNG: its first scanlines are a valid image of
    their own (the first row's filter has no row above it either way)."""
    import struct
    import zlib
    assert data[:8] == b"\x89PNG\r\n\x1a\n"
    chunks, at = [], 8
    while at < len(data):
        n, kind = struct.unpack(">I4s", data[at:at + 8])
        chunks.append((kind, data[at + 8:at + 8 + n]))
        at += 12 + n
    ihdr = dict(chunks)[b"IHDR"]
    width, _, depth, color, _, _, interlace = struct.unpack(">IIBBBBB", ihdr)
    assert interlace == 0
    channels = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}[color]
    row = 1 + (width * channels * depth + 7) // 8
    pixels = zlib.decompress(b"".join(c for k, c in chunks if k == b"IDAT"))[:row * size]

    def chunk(kind, body):
        return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body))

    out = data[:8] + chunk(b"IHDR", struct.pack(">II", width, size) + ihdr[8:])
    for kind, body in chunks:
        if kind in (b"PLTE", b"tRNS"):
            out += chunk(kind, body)
    return out + chunk(b"IDAT", zlib.compress(pixels)) + chunk(b"IEND", b"")


def sps_textures(ns, out, search):
    """The SPS's effects (`RenderSPS`) as GUI sprites for the native quads: the core's 4 x 4 sheet cut into
    sixteen 64 px frames (`sps_core_N`) and the orbiting speck (`sps_orbit`)."""
    if ns != "mekanism":
        return 0
    sheet = next((d / ns / "render/energy_effect.png" for d in search if (d / ns / "render/energy_effect.png").is_file()), None)
    orbit = next((d / ns / "render/sps_orbit_effect.png" for d in search if (d / ns / "render/sps_orbit_effect.png").is_file()), None)
    if sheet is None or orbit is None:
        return 0
    from PIL import Image
    dst = out / "assets" / ns / "textures/gui"
    dst.mkdir(parents=True, exist_ok=True)
    image = Image.open(sheet).convert("RGBA")
    cell = image.width // 4
    for n in range(16):
        x, y = n % 4 * cell, n // 4 * cell
        image.crop((x, y, x + cell, y + cell)).resize((64, 64), Image.LANCZOS).save(dst / f"sps_core_{n}.png")
    shutil.copyfile(orbit, dst / "sps_orbit.png")
    # The Flamethrower's flames (`RenderFlame`) draw this sprite.
    flame = next((d / ns / "render/flame.png" for d in search if (d / ns / "render/flame.png").is_file()), None)
    if flame is not None:
        shutil.copyfile(flame, dst / "flame.png")
    # The Laser's beam (`LaserParticle`) draws this sprite.
    beam = SRC / "main/resources/assets/mekanism/textures/particle/laser.png"
    if ns == "mekanism" and beam.is_file():
        shutil.copyfile(beam, dst / "laser_beam.png")
    # The Configurator's overlays (`MekanismRenderer.overlays`) on the face looked at.
    overlays = 0
    for name in ("energy", "items", "fluids", "chemicals", "heat"):
        overlay = SRC / f"main/resources/assets/mekanism/textures/block/overlay/{name}_overlay.png"
        if overlay.is_file():
            shutil.copyfile(overlay, dst / f"overlay_{name}.png")
            overlays += 1
    return 19 + overlays


def sound_files(ns, out):
    """The sounds the native mod loops (`native_loop_sound`): the Industrial Alarm and the Flamethrower, with their
    `sounds.json` events, so Horizon's sound registry finds them under the mod's namespace."""
    if ns != "mekanism":
        return 0
    import json
    root = Path(__file__).resolve().parent.parent / "src"
    events_file = root / "datagen/generated/mekanism/assets/mekanism/sounds.json"
    sounds_dir = root / "main/resources/assets/mekanism/sounds"
    if not events_file.is_file():
        return 0
    events = json.loads(events_file.read_text())
    # Every machine's working hum (`tile.machine.*`) and the Flamethrower's loops.
    names = [n for n in events if n.startswith("tile.machine.") or n.startswith("item.flamethrower.")]
    keep = {name: events[name] for name in names}
    files = []
    for value in keep.values():
        for sound in value["sounds"]:
            files.append((sound["name"] if isinstance(sound, dict) else sound).split(":", 1)[1])
    dst = out / "assets" / ns
    for f in files:
        src = sounds_dir / (f + ".ogg")
        if not src.is_file():
            continue
        (dst / "sounds" / f).parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(src, dst / "sounds" / (f + ".ogg"))
    (dst / "sounds.json").write_text(json.dumps(keep, indent=2))
    return len(files)


def hud_sprites(ns, out):
    """The HUD's icons (`textures/gui/sprites/hud/*`, `HUDRenderer`) as `textures/gui/hud_<name>.png`: Horizon's GUI
    atlas holds a content mod's `textures/gui` only."""
    src = SRC / "main/resources/assets/mekanism/textures/gui/sprites/hud"
    if ns != "mekanism" or not src.is_dir():
        return 0
    dst = out / "assets" / ns / "textures/gui"
    dst.mkdir(parents=True, exist_ok=True)
    count = 0
    for icon in sorted(src.glob("*.png")):
        shutil.copyfile(icon, dst / f"hud_{icon.name}")
        count += 1
    return count


def gauge_textures(ns, out):
    """The textures Mekanism's gauges fill with (`mek_liquid/*`, the chemicals' base icon), first frames
    copied under `textures/gui/gauge_fluid/`: Horizon's GUI atlas holds a content mod's `textures/gui` only."""
    src = out / "assets" / ns / "textures/mek_liquid"
    dst = out / "assets" / ns / "textures/gui/gauge_fluid"
    count = 0
    for texture in sorted(src.glob("*.png")) if src.is_dir() else []:
        if texture.stem.endswith("_flow"):
            continue
        dst.mkdir(parents=True, exist_ok=True)
        (dst / texture.name).write_bytes(first_frame_png(texture.read_bytes(), 16))
        count += 1
    portal = out / "assets" / ns / "textures/block/teleporter_portal.png"
    if portal.is_file():
        dst.mkdir(parents=True, exist_ok=True)
        (dst / "teleporter_portal.png").write_bytes(first_frame_png(portal.read_bytes(), 16))
        count += 1
    return count


def component_items(ns, out):
    """`mekanism:component_item` models become vanilla selects on custom_model_data's first string."""
    count = 0
    for path in sorted((out / "assets" / ns / "items").glob("*.json")):
        definition = json.loads(path.read_text())
        model = definition.get("model", {})
        if model.get("type") != "mekanism:component_item":
            continue
        component = model["mek_data"].split(":", 1)[1]
        textures = out / "assets" / ns / "textures/item" / component
        cases = []
        for texture in sorted(textures.glob("*.png")) if textures.is_dir() else []:
            name = f"{ns}:item/{component}/{texture.stem}"
            write_json(out / "assets" / ns / "models/item" / component / f"{texture.stem}.json",
                       {"parent": "minecraft:item/generated", "textures": {"layer0": name}})
            cases.append({"when": f"{ns}:{texture.stem}", "model": {"type": "minecraft:model", "model": name}})
        if not cases:
            continue
        definition["model"] = {"type": "minecraft:select", "property": "minecraft:custom_model_data",
                               "index": 0, "cases": cases, "fallback": cases[0]["model"]}
        path.write_text(json.dumps(definition, indent=2) + "\n")
        count += 1
    return count


def carried_result(result):
    """A recipe result with Mekanism's components, in vanilla components; None when it has none."""
    components = result.get("components", {})
    mek = {k: v for k, v in components.items() if k in COMPONENT_NAMES}
    if not mek:
        return None
    rest = {k: v for k, v in components.items() if k not in mek}
    rest["minecraft:custom_data"] = mek
    (key, value), = mek.items()
    rest["minecraft:custom_model_data"] = {"strings": [value]}
    ns, path = value.split(":", 1)
    kind = COMPONENT_NAMES[key]
    if kind == "upgrade":
        rest["minecraft:item_name"] = [{"translate": f"upgrade.{ns}.{path}"}, " ",
                                       {"translate": f"item.{ns}.upgrade"}]
    else:
        rest["minecraft:item_name"] = {"translate": f"module.{ns}.{path}"}
    return {**result, "components": rest}


def component_recipes(recipes):
    count = 0
    for path in sorted(recipes.rglob("*.json")) if recipes.is_dir() else []:
        recipe = json.loads(path.read_text())
        result = recipe.get("result")
        carried = carried_result(result) if isinstance(result, dict) else None
        if carried is None:
            continue
        recipe["result"] = carried
        path.write_text(json.dumps(recipe, indent=2) + "\n")
        count += 1
    return count


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2) + "\n")


def pack(content, hmod):
    with zipfile.ZipFile(hmod, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for path in sorted(content.rglob("*")):
            if path.is_file():
                info = zipfile.ZipInfo(path.relative_to(content).as_posix(), (2026, 1, 1, 0, 0, 0))
                info.compress_type = zipfile.ZIP_DEFLATED
                info.external_attr = 0o100644 << 16
                archive.writestr(info, path.read_bytes())


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--out", type=Path, default=ROOT / "target" / "content")
    args = parser.parse_args()
    result = {}
    for ns in NAMESPACES:
        report, skipped = build_module(ns, args.out / ns)
        hmod = args.out / f"{ns}-{VERSION}.hmod"
        pack(args.out / ns, hmod)
        result[ns] = {"hmod": str(hmod), "size": hmod.stat().st_size, "carried": report,
                      "left_out_files_by_namespace": skipped}
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    sys.exit(main())
