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


def block_template(name, blockstate):
    """The vanilla block a Mekanism block copies, from what it is."""
    states = blockstate_properties(blockstate)
    if name.endswith("_ore"):
        return "minecraft:deepslate_iron_ore" if name.startswith("deepslate_") else "minecraft:iron_ore"
    if name in MACHINES or name.endswith(TIERED_MACHINES):
        # No vanilla block with a block entity has no state at all; these wait for their milestone.
        return "minecraft:furnace" if "facing" in states else "minecraft:iron_block"
    if name.endswith(TRANSMITTERS):
        return "minecraft:iron_bars"
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
        write_json(declared / "block" / f"{name}.json", {"template": template})
    items = 0
    for path in sorted((out / "assets" / ns / "items").glob("*.json")):
        if path.stem in blocks:
            continue
        write_json(declared / "item" / f"{path.stem}.json", {"template": item_template(path.stem)})
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
