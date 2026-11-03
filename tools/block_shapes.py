#!/usr/bin/env python3
"""Mekanism's block shapes, read from the Java source (`BlockShapes.java` and the block type registries).

`shapes(src)` returns {namespace: {block: Shape}} where a Shape is either {"all": [box...]} (one shape for
every state) or {"facing": {"north": [box...], ...}}; a box is [x0, y0, z0, x1, y1, z1] in blocks, clipped
to the block's own cell (parts that stand in the cells of a multi-block machine's bounding blocks are not
this block's).  The rotations mirror `VoxelShapeUtils` and vanilla's `Shapes.rotateHorizontal/rotateAll`.
"""
import re
import sys
from pathlib import Path

HORIZONTAL = ("north", "east", "south", "west")
ALL_DIRECTIONS = HORIZONTAL + ("up", "down")

# Rotations act on a point's offset from the block centre.
def y_turn(direction):
    # north is -z; east is +x (a quarter turn clockwise seen from above)
    return {"north": lambda x, y, z: (x, y, z),
            "east": lambda x, y, z: (-z, y, x),
            "south": lambda x, y, z: (-x, y, -z),
            "west": lambda x, y, z: (z, y, -x)}[direction]


def toward(direction):
    """The rotation of a shape modelled facing north onto `direction`."""
    if direction in HORIZONTAL:
        return y_turn(direction)
    if direction == "up":
        return lambda x, y, z: (x, -z, y)
    return lambda x, y, z: (x, z, -y)


# A shape modelled facing down, turned to face north (OctahedralGroup.BLOCK_ROT_X_270).
def down_to_north(x, y, z):
    return (x, -z, y)


LEGACY = {  # rotateAllLegacy: a shape modelled facing up
    "north": lambda x, y, z: (-x, -z, -y),     # ROT_180_EDGE_YZ_NEG
    "east": lambda x, y, z: (-z, x, -y),       # ROT_120_PPN
    "south": lambda x, y, z: (x, -z, y),       # ROT_90_X_POS
    "west": lambda x, y, z: (z, -x, -y),       # ROT_120_PNP
    "up": lambda x, y, z: (x, y, z),
    "down": lambda x, y, z: (x, -y, -z),       # ROT_180_FACE_YZ
}


def turn_boxes(boxes, fn):
    out = []
    for b in boxes:
        lo = [b[0] - .5, b[1] - .5, b[2] - .5]
        hi = [b[3] - .5, b[4] - .5, b[5] - .5]
        pts = [fn(x, y, z) for x in (lo[0], hi[0]) for y in (lo[1], hi[1]) for z in (lo[2], hi[2])]
        out.append([round(min(p[i] for p in pts) + .5, 6) for i in range(3)]
                   + [round(max(p[i] for p in pts) + .5, 6) for i in range(3)])
    return out


# ---- a tiny evaluator for the expressions in BlockShapes.java -----------------------------------

TOKEN = re.compile(r"\s*(?:([A-Za-z_][A-Za-z_0-9.]*)|(-?\d+(?:\.\d+)?)|(.))")


def tokens(text):
    pos = 0
    out = []
    while pos < len(text):
        m = TOKEN.match(text, pos)
        if not m:
            break
        pos = m.end()
        if m.group(1):
            out.append(("id", m.group(1)))
        elif m.group(2):
            out.append(("num", float(m.group(2))))
        elif m.group(3) is not None and not m.group(3).isspace():
            out.append(("sym", m.group(3)))
    return out


class Eval:
    def __init__(self, constants):
        self.constants = constants  # name -> expression text
        self.cache = {}

    def value(self, name):
        if name not in self.cache:
            self.cache[name] = self.parse(tokens(self.constants[name]))
        return self.cache[name]

    def parse(self, toks):
        value, i = self.expr(toks, 0)
        return value

    def expr(self, toks, i):
        kind, v = toks[i]
        if kind == "num":
            return v, i + 1
        assert kind == "id", (toks[i:i + 4])
        if i + 1 < len(toks) and toks[i + 1] == ("sym", "("):
            args = []
            j = i + 2
            while toks[j] != ("sym", ")"):
                if toks[j] == ("sym", ","):
                    j += 1
                    continue
                a, j = self.expr(toks, j)
                args.append(a)
            value = self.call(v, args)
            j += 1
        else:
            value = self.constants_or_name(v)
            j = i + 1
        while j + 1 < len(toks) and toks[j] == ("sym", ".") and toks[j + 1] == ("id", "move"):
            nums = [t[1] for t in toks[j + 3:j + 9] if t[0] == "num"][:3]
            value = move_shape(value, *nums)
            j += 3
            while toks[j] != ("sym", ")"):
                j += 1
            j += 1
        return value, j

    def constants_or_name(self, name):
        short = name.split(".")[-1]
        if short in self.constants:
            return self.value(short)
        return ("name", name)

    def call(self, name, args):
        short = name.split(".")[-1]
        if short == "box":
            return ("shape", [[a / 16 for a in args]])
        if short == "combine":
            return ("shape", [b for a in args for b in a[1]])
        if short == "move":
            raise ValueError("move handled by the caller")
        initial = args[1][1].split(".")[-1] if len(args) > 1 else None
        shape = args[0][1]
        if short in ("rotateHorizontal",):
            if initial == "BLOCK_ROT_Y_180":
                shape = turn_boxes(shape, y_turn("south"))
            elif initial == "BLOCK_ROT_Y_90":
                shape = turn_boxes(shape, y_turn("east"))
            elif initial is not None:
                raise ValueError(initial)
            return ("map", {d: turn_boxes(shape, toward(d)) for d in HORIZONTAL})
        if short == "rotateAll":
            if initial == "BLOCK_ROT_X_270":
                shape = turn_boxes(shape, down_to_north)
            elif initial == "BLOCK_ROT_X_90":
                shape = turn_boxes(shape, lambda x, y, z: (x, z, -y))
            elif initial is not None:
                raise ValueError(initial)
            return ("map", {d: turn_boxes(shape, toward(d)) for d in ALL_DIRECTIONS})
        if short == "rotateAllInitialDown":
            return ("map", {d: turn_boxes(turn_boxes(shape, down_to_north), toward(d)) for d in ALL_DIRECTIONS})
        if short == "rotateAllLegacy":
            return ("map", {d: turn_boxes(shape, LEGACY[d]) for d in ALL_DIRECTIONS})
        raise ValueError(name)


def constants_of(path):
    text = re.sub(r"//[^\n]*", "", Path(path).read_text())
    out = {}
    for m in re.finditer(r"static final (?:Map<Direction, VoxelShape>|VoxelShape) (\w+) = (.*?);\s*(?=\n\s*(?:public|private|static|//|\}|$))",
                         text, re.S):
        body = m.group(2)
        # `.move(x, y, z)` after a combine
        mv = re.search(r"\)\.move\(([^)]*)\)\s*$", body)
        out[m.group(1)] = body
    return out


def move_shape(value, dx, dy, dz):
    kind, boxes = value
    return (kind, [[b[0] + dx, b[1] + dy, b[2] + dz, b[3] + dx, b[4] + dy, b[5] + dz] for b in boxes])


# ---- orientation check against the block models -------------------------------------------------------

def rotations():
    """The 24 proper rotations of the cube as point maps."""
    import itertools
    out = []
    for perm in itertools.permutations(range(3)):
        for signs in itertools.product((1, -1), repeat=3):
            m = [[0] * 3 for _ in range(3)]
            for r in range(3):
                m[r][perm[r]] = signs[r]
            det = (m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
                   + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]))
            if det == 1:
                out.append(lambda x, y, z, m=m: tuple(m[r][0] * x + m[r][1] * y + m[r][2] * z for r in range(3)))
    return out


ROTATIONS = rotations()
GRID = 24


def voxels(boxes):
    cells = set()
    for b in boxes:
        lo = [max(0, int(round(b[i] * GRID))) for i in range(3)]
        hi = [min(GRID, int(round(b[i + 3] * GRID))) for i in range(3)]
        for x in range(lo[0], hi[0]):
            for y in range(lo[1], hi[1]):
                for z in range(lo[2], hi[2]):
                    cells.add((x, y, z))
    return cells


def iou(a, b):
    if not a and not b:
        return 1.0
    return len(a & b) / max(1, len(a | b))


def clip(boxes):
    out = []
    for b in boxes:
        c = [max(0.0, min(1.0, v)) for v in b]
        if c[0] < c[3] and c[1] < c[4] and c[2] < c[5]:
            out.append(c)
    return out


MODULES = {  # namespace: (source folder, block type registry)
    "mekanism": ("main/java/mekanism/common", "main/java/mekanism/common/registries/MekanismBlockTypes.java"),
    "mekanismgenerators": ("generators/java/mekanism/generators/common",
                           "generators/java/mekanism/generators/common/registries/GeneratorsBlockTypes.java"),
    "mekanismadditions": ("additions/java/mekanism/additions/common",
                          "additions/java/mekanism/additions/common/registries/AdditionsBlockTypes.java"),
}
TIERS = ("basic", "advanced", "elite", "ultimate", "creative")
FACTORY_SHAPES = {"smelting": "SMELTING_FACTORY", "enriching": "ENRICHING_FACTORY", "crushing": "CRUSHING_FACTORY",
                  "compressing": "COMPRESSING_FACTORY", "combining": "COMBINING_FACTORY",
                  "purifying": "PURIFYING_FACTORY", "injecting": "INJECTING_FACTORY",
                  "infusing": "INFUSING_FACTORY", "sawing": "SAWING_FACTORY"}
# Block type constants whose block has another name, and blocks sharing a type's shape.
EXTRA = {"mekanism": {"laser_tractor_beam": "LASER_AMPLIFIER", "radioactive_waste_barrel": "RADIOACTIVE_WASTE_BARREL"},
         "mekanismgenerators": {"turbine_rotor": "TURBINE_ROTOR", "fission_fuel_assembly": "FUEL_ASSEMBLY",
                                "control_rod_assembly": "CONTROL_ROD_ASSEMBLY"},
         "mekanismadditions": {}}


COLORS = ("black", "dark_blue", "dark_green", "dark_aqua", "dark_red", "purple", "orange", "light_gray", "dark_gray",
          "blue", "green", "aqua", "red", "pink", "yellow", "white", "brown", "light_blue", "lime", "cyan", "magenta")


def shape_names(src, ns):
    """block name -> BlockShapes constant, from `withCustomShape(BlockShapes.X)` in the block type registry."""
    folder, registry = MODULES[ns]
    text = (src / registry).read_text()
    chunks = re.split(r"\n    public static final ", text)
    out = {}
    for chunk in chunks[1:]:
        m = re.match(r"[\w<>?, ]+? (\w+) =", chunk)
        c = re.search(r"withCustomShape\(BlockShapes\.(\w+)\)", chunk)
        if m and c and m.group(1).isupper():
            out[m.group(1).lower()] = c.group(1)
    out.update(EXTRA[ns])
    if ns == "mekanism":
        for tier in TIERS:
            out[f"{tier}_fluid_tank"] = "FLUID_TANK"
            out[f"{tier}_chemical_tank"] = "CHEMICAL_TANK"
            for kind, const in FACTORY_SHAPES.items():
                out[f"{tier}_{kind}_factory"] = const
    return out


def as_shape(value):
    kind, data = value
    if kind == "shape":
        return {"all": clip(data)}
    return {"facing": {d: clip(b) for d, b in data.items()}}


def collect(src):
    src = Path(src)
    out = {}
    for ns, (folder, _) in MODULES.items():
        consts = constants_of(src / folder / "content/blocktype/BlockShapes.java")
        ev = Eval(consts)
        names = shape_names(src, ns)
        shapes = {}
        if ns == "mekanismadditions":
            for color in COLORS:
                names[f"{color}_glow_panel"] = "GLOW_PANEL"
        for block, const in names.items():
            if const in consts:
                shapes[block] = as_shape(ev.value(const))
        out[ns] = shapes
    cube = energy_cube_boxes(src)
    for tier in ("basic", "advanced", "elite", "ultimate", "creative"):
        out["mekanism"][f"{tier}_energy_cube"] = {"all": clip(cube)}
    return out


def energy_cube_boxes(src):
    """BlockEnergyCube's frame and all six port panels (its shape when every side is enabled)."""
    text = (src / "main/java/mekanism/common/block/BlockEnergyCube.java").read_text()
    text = text[text.index("static {"):text.index("VoxelShape frameRotated")]
    text = re.sub(r"//[^\n]*", "", text)
    return [[float(v) / 16 for v in m.groups()] for m in re.finditer(
        r"box\(([-\d.]+),\s*([-\d.]+),\s*([-\d.]+),\s*([-\d.]+),\s*([-\d.]+),\s*([-\d.]+)\)", text)]


# ---- the check against block models -------------------------------------------------------------------

import json


def model_boxes(assets, model_id, seen=()):
    ns, _, path = model_id.partition(":")
    if not path:
        ns, path = "minecraft", model_id
    f = next((d / "models" / (path + ".json") for d in assets.get(ns, [])
              if (d / "models" / (path + ".json")).is_file()), None)
    if f is None:
        return []
    data = json.loads(f.read_text())
    boxes = [[v / 16 for v in e["from"] + e["to"]] for e in data.get("elements", [])]
    if not boxes and "parent" in data:
        return model_boxes(assets, data["parent"])
    return boxes


def turned(boxes, x, y):
    for _ in range((x // 90) % 4):
        boxes = turn_boxes(boxes, lambda a, b, c: (a, -c, b))
    for _ in range((y // 90) % 4):
        boxes = turn_boxes(boxes, y_turn("east"))
    return boxes


def facing_models(assets, ns, block):
    """facing -> boxes of the model the blockstate shows for it (the first variant per facing)."""
    f = next((d / "blockstates" / f"{block}.json" for d in assets[ns]
              if (d / "blockstates" / f"{block}.json").is_file()), None)
    if f is None:
        return {}
    variants = json.loads(f.read_text()).get("variants", {})
    out = {}
    for key, v in variants.items():
        v = v[0] if isinstance(v, list) else v
        facing = dict(p.split("=") for p in key.split(",") if "=" in p).get("facing")
        if facing and facing not in out:
            boxes = model_boxes(assets, v["model"])
            if boxes:
                out[facing] = turned(boxes, v.get("x", 0), v.get("y", 0))
    return out


def assets_of(root):
    root = Path(root)
    mod = {"mekanism": "main", "mekanismgenerators": "generators", "mekanismadditions": "additions"}
    return {ns: [root / "src" / m / "resources/assets" / ns, root / "src/datagen/generated" / ns / "assets" / ns]
            for ns, m in mod.items()}


def fit(shapes, assets, report):
    """Correct each facing shape's orientation to the one the block's model shows: the proper rotation of the
    Java shape that overlaps the model most.  Returns the fixed table; `report` collects what changed."""
    for ns, table in shapes.items():
        for block, shape in table.items():
            if "facing" not in shape:
                continue
            models = facing_models(assets, ns, block)
            for facing, boxes in list(shape["facing"].items()):
                if facing not in models:
                    continue
                truth = voxels(models[facing])
                if not truth:
                    continue
                mine = voxels(boxes)
                base = iou(mine, truth)
                best, best_score = None, base
                for r in ROTATIONS:
                    score = iou(voxels(turn_boxes(boxes, r)), truth)
                    if score > best_score + 1e-9:
                        best, best_score = r, score
                if best is not None:
                    shape["facing"][facing] = turn_boxes(boxes, best)
                    report.append(f"{ns}:{block} {facing}: rotated ({base:.2f} -> {best_score:.2f})")
                elif base < 0.6:
                    report.append(f"{ns}:{block} {facing}: poor overlap {base:.2f}")
    return shapes


if __name__ == "__main__":
    root = Path(sys.argv[1] if len(sys.argv) > 1 else Path(__file__).resolve().parent.parent)
    table = collect(root / "src")
    assets = assets_of(root)
    log = []
    fit(table, assets, log)
    print("\n".join(log))
    print({ns: len(t) for ns, t in table.items()})
