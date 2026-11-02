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


def pipe_models(ns, out, name, blockstate):
    """Vanilla element models for a transmitter whose blockstate uses Mekanism's OBJ model: a centre cube
    and an arm per joined side, as a multipart blockstate. Returns its size, or None."""
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
    lo, hi = PIPE_SIZES[size]
    cube = {d: {"texture": "#center"} for d in DIRECTIONS}
    write_json(models / "block" / "transmitter" / "horizon" / f"{name}_center.json", {
        "parent": "block/block",
        "textures": {"particle": center, "center": center},
        "elements": [{"from": [lo, lo, lo], "to": [hi, hi, hi], "faces": cube}],
    })
    arm_faces = {d: {"texture": "#side", "uv": [0, 0, 16, 16]} for d in ("east", "west", "up", "down")}
    arm_faces["north"] = {"texture": "#center", "cullface": "north"}
    write_json(models / "block" / "transmitter" / "horizon" / f"{name}_arm.json", {
        "textures": {"particle": center, "center": center, "side": side},
        "elements": [{"from": [lo, lo, 0], "to": [hi, hi, lo], "faces": arm_faces}],
    })
    base = f"{ns}:block/transmitter/horizon/{name}"
    parts = [{"apply": {"model": f"{base}_center"}}]
    for d in DIRECTIONS:
        parts.append({"when": {d: "false"}, "apply": {"model": f"{base}_arm", **ARM_ROTATION[d]}})
    write_json(out / "assets" / ns / "blockstates" / f"{name}.json", {"multipart": parts})
    # The item shows the bare centre, as Mekanism's item model does.
    write_json(models / "item" / f"{name}.json", {"parent": f"{base}_center"})
    return size


# Items that hold energy in custom_data (mekanism:energy) stack to one, as Mekanism's do.
SINGLE_ITEMS = {"energy_tablet", "atomic_disassembler", "electric_bow", "portable_teleporter", "free_runners", "free_runners_armored",
                "qio_drive_base", "qio_drive_hyper_dense", "qio_drive_time_dilating", "qio_drive_supermassive"}
# ItemAtomicDisassembler's Tool: no drops from #mekanism:incorrect_for_disassembler, drops from every other
# block (vanilla's tool rules take tags, so "every block" is the four mineable tags). Its speed comes from
# the bundle (native_destroy_speed), from the energy it holds.
DISASSEMBLER_TOOL = {
    "rules": [{"blocks": "#mekanism:incorrect_for_disassembler", "correct_for_drops": False}]
    + [{"blocks": f"#minecraft:mineable/{t}", "correct_for_drops": True} for t in ("pickaxe", "axe", "shovel", "hoe")],
    "default_mining_speed": 1.0,
}


def block_template(name, blockstate):
    """The vanilla block a Mekanism block copies, from what it is."""
    states = blockstate_properties(blockstate)
    if name.endswith("_ore"):
        return "minecraft:deepslate_iron_ore" if name.startswith("deepslate_") else "minecraft:iron_ore"
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
    # The salt disk uses Mekanism's own feature and height types; it waits for a native feature.
    for folder in (features, placed_dir):
        for path in folder.glob("salt*.json") if folder.is_dir() else []:
            path.unlink()

    # Items told apart by a Mekanism data component (upgrades, modules): Horizon has no mod components, so
    # the type goes in vanilla's custom_data, the model is chosen by custom_model_data, and the name is
    # spelled out with item_name.
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
        if template == "minecraft:furnace":
            # The furnace's `lit` stands in for Mekanism's `active`: same meaning, same two values.
            path.write_text(json.dumps(rename_property(blockstate, "active", "lit"), indent=2) + "\n")
            report["active_as_lit"] = report.get("active_as_lit", 0) + 1
        declaration = {"template": template}
        if template == PIPE_TEMPLATE:
            size = pipe_models(ns, out, name, blockstate)
            if size:
                declaration = pipe_declaration(size)
                report["pipes"] = report.get("pipes", 0) + 1
        write_json(declared / "block" / f"{name}.json", declaration)
    items = 0
    for path in sorted((out / "assets" / ns / "items").glob("*.json")):
        if path.stem in blocks:
            continue
        item = {"template": item_template(path.stem)}
        if path.stem in SINGLE_ITEMS:
            item["max_stack_size"] = 1
        if path.stem == "atomic_disassembler":
            item["components"] = {"minecraft:tool": DISASSEMBLER_TOOL}
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
