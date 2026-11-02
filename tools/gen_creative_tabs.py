#!/usr/bin/env python3
"""Generate mixins/mekanism/core/src/creative_tabs.rs: the item order of Mekanism's four creative tabs.

Each tab lists, as `MekanismCreativeTabs`/`GeneratorsCreativeTabs`/... do, the module's ITEMS register, then its
BLOCKS register's items, then its FLUIDS register's buckets, each in registration order. That order is the
textual order of the fields in the `*Items`/`*Blocks`/`*Fluids` classes, with the loops (processed resources,
factories, ores, the 16 colours, tool materials) expanded where their `static {}` block or collection field
stands. Only ids that have an item definition in src/datagen/generated are kept. Run from the repo root.
"""
import os, re, sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SRC = ROOT / "src"
GEN = SRC / "datagen/generated"
MODS = [  # namespace, source folder, class prefix, package, title, icon
    ("mekanism", "main", "Mekanism", "mekanism/common/registries", "Mekanism", "metallurgic_infuser"),
    ("mekanismgenerators", "generators", "Generators", "mekanism/generators/common/registries", "Mekanism: Generators", "heat_generator"),
    ("mekanismadditions", "additions", "Additions", "mekanism/additions/common/registries", "Mekanism: Additions", "lime_balloon"),
    ("mekanismtools", "tools", "Tools", "mekanism/tools/common/registries", "Mekanism: Tools", "diamond_paxel"),
]


def enum_names(path):
    """Constant names of an enum, in order (the lines `NAME(` of its head)."""
    out = []
    for line in Path(path).read_text().splitlines():
        m = re.match(r"\s{4}([A-Z][A-Z0-9_]*)[(,;]", line)
        if m:
            out.append(m.group(1))
        elif out and line.strip().startswith(("private", "public", "static")):
            break
    return out


def colors():
    pre = []
    text = (SRC / "api/java/mekanism/api/text/EnumColor.java").read_text()
    for m in re.finditer(r"^    ([A-Z_]+)\((DyeColor\.([A-Z_]+)|\"([a-z_]+)\")", text, re.M):
        pre.append((m.group(3) or m.group(4)).lower())
    return pre


def expansions(ns, ids):
    """Loop expansions keyed by the marker that holds their place."""
    ex = {}
    if ns == "mekanism":
        rtypes = [n.lower() for n in enum_names(SRC / "main/java/mekanism/common/resource/ResourceType.java")]
        res = [n.lower() for n in enum_names(SRC / "main/java/mekanism/common/resource/PrimaryResource.java")]
        ex["PROCESSED_RESOURCES"] = [f"{t}_{r}" for t in rtypes for r in res]
        tiers = ["basic", "advanced", "elite", "ultimate"]
        ftypes = [n.lower() for n in enum_names(SRC / "main/java/mekanism/common/content/blocktype/FactoryType.java")]
        fac = [f"{t}_{f}_factory" for t in tiers for f in ftypes]
        blocks = []
        for r in res:
            blocks += [f"block_{r}", f"block_raw_{r}"]
        ores = [n.lower() for n in enum_names(SRC / "main/java/mekanism/common/resource/ore/OreType.java")]
        ore = []
        for o in ores:
            ore += [f"{o}_ore", f"deepslate_{o}_ore"]
        ex["BLOCKS_STATIC"] = fac + blocks + ore
    elif ns == "mekanismadditions":
        cs = colors()
        suffixes = {
            "GLOW_PANELS": "glow_panel", "PLASTIC_BLOCKS": "plastic", "SLICK_PLASTIC_BLOCKS": "slick_plastic",
            "PLASTIC_GLOW_BLOCKS": "plastic_glow", "REINFORCED_PLASTIC_BLOCKS": "reinforced_plastic",
            "PLASTIC_ROADS": "plastic_road", "TRANSPARENT_PLASTIC_BLOCKS": "plastic_transparent",
            "PLASTIC_STAIRS": "plastic_stairs", "PLASTIC_SLABS": "plastic_slab", "PLASTIC_FENCES": "plastic_fence",
            "PLASTIC_FENCE_GATES": "plastic_fence_gate", "PLASTIC_GLOW_STAIRS": "plastic_glow_stairs",
            "PLASTIC_GLOW_SLABS": "plastic_glow_slab", "TRANSPARENT_PLASTIC_STAIRS": "plastic_transparent_stairs",
            "TRANSPARENT_PLASTIC_SLABS": "plastic_transparent_slab", "BALLOONS": "balloon",
        }
        for k, s in suffixes.items():
            ex[k] = [f"{c}_{s}" for c in cs]
        babies = enum_names(SRC / "additions/java/mekanism/additions/common/entity/baby/BabyType.java")
        ex["BABY_SPAWN_EGGS"] = [f"baby_{b.lower()}_spawn_egg" for b in babies]
    elif ns == "mekanismtools":
        for m in ["bronze", "lapis_lazuli", "osmium", "refined_glowstone", "refined_obsidian", "steel"]:
            M = m.upper()
            ex[f"{M}_ARMOR"] = [f"{m}_{p}" for p in ("helmet", "chestplate", "leggings", "boots", "horse_armor", "nautilus_armor")]
            ex[f"{M}_TOOLS"] = [f"{m}_{p}" for p in ("axe", "hoe", "paxel", "pickaxe", "shield", "shovel", "sword", "spear")]
    return ex


def fields(path, ns, ids, ex, kind):
    """Item ids of a registry class, in textual order."""
    src = path.read_text()
    events = []
    for m in re.finditer(r"public static final [^=;]*?\b([A-Z][A-Z0-9_]*)\s*=\s*([^;]*?);", src, re.S):
        events.append((m.start(), "field", m.group(1), m.group(2)))
    for m in re.finditer(r"^    static \{", src, re.M):
        events.append((m.start(), "static", None, None))
    events.sort()
    out = []
    for _, k, name, body in events:
        if k == "static":
            out += ex.get("PROCESSED_RESOURCES" if kind == "Items" else "BLOCKS_STATIC", [])
            continue
        if name in ex:
            out += ex[name]
            continue
        if name == "ENRICHED_OBSIDIAN":
            out.append("enriched_refined_obsidian")
            continue
        toks = name.lower().split("_")
        cands = []
        q = re.search(r'"([a-z0-9_]+)"', body)
        if q:
            cands.append(q.group(1))
        if kind == "Fluids":
            cands.append(name.lower() + "_bucket")
        cands += ["_".join(toks[i:] + toks[:i]) for i in range(len(toks))]
        hit = next((c for c in cands if c in ids), None)
        if hit:
            out.append(hit)
    return out


def main():
    check = "--check" in sys.argv
    lines = ["// Generated by tools/gen_creative_tabs.py: the order of Mekanism's creative tabs (Java registration order).", ""]
    lines.append("/// One creative tab: its id, English title, icon item and items (ids with their namespace).")
    lines.append("pub struct CreativeTab {\n    pub id: &'static str,\n    pub title: &'static str,\n    pub icon: &'static str,\n    pub items: &'static [&'static str],\n}\n")
    lines.append("pub const CREATIVE_TABS: &[CreativeTab] = &[")
    for ns, d, pre, pkg, title, icon in MODS:
        ids = {f[:-5] for f in os.listdir(GEN / ns / "assets" / ns / "items")}
        ex = expansions(ns, ids)
        order = []
        for kind in ("Items", "Blocks", "Fluids"):
            p = SRC / d / "java" / pkg / f"{pre}{kind}.java"
            if p.exists():
                order += fields(p, ns, ids, ex, kind)
        seen, items = set(), []
        for i in order:
            if i in ids and i not in seen and i != "module":  # module stacks need components
                seen.add(i)
                items.append(i)
        left = sorted(ids - seen - {"module"})
        print(f"{ns}: {len(items)} of {len(ids)} item definitions placed; left out: {left}", file=sys.stderr)
        items += left  # anything unplaced goes last rather than missing
        lines.append(f'    CreativeTab {{\n        id: "{ns}:{ns}",\n        title: "{title}",\n        icon: "{ns}:{icon}",\n        items: &[')
        lines += [f'            "{ns}:{i}",' for i in items]
        lines.append("        ],\n    },")
    lines.append("];\n")
    out = ROOT / "mixins/mekanism/core/src/creative_tabs.rs"
    out.write_text("\n".join(lines))


main()
