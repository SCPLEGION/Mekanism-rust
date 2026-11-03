#!/usr/bin/env python3
"""Mekanism's worn models (free runners, jetpacks, scuba gear) for Horizon's `layers.horizon_models`.

Mekanism draws these from its item models, placed on the wearer's model parts by `FreeRunnerArmor`,
`ChestArmor` and the head-item layer.  Here each item model is baked to quads (FaceBakery's vertex order and
UV rotation), moved by the same transforms into the part's local space (pixels, y down) and written as an
entity-model part tree: `assets/<ns>/horizon/worn_models/<name>.json`, one file per texture, with an
equipment asset `assets/<ns>/equipment/<item>.json` naming them and the texture copied to
`textures/entity/equipment/horizon_models/`.
"""
import json
import math
import shutil
from pathlib import Path

import flatten_models
import obj_models

DIRECTIONS = {  # FaceInfo vertex selectors: 0 = from, 1 = to
    "down": [[0, 0, 1], [0, 0, 0], [1, 0, 0], [1, 0, 1]],
    "up": [[0, 1, 0], [0, 1, 1], [1, 1, 1], [1, 1, 0]],
    "north": [[1, 1, 0], [1, 0, 0], [0, 0, 0], [0, 1, 0]],
    "south": [[0, 1, 1], [0, 0, 1], [1, 0, 1], [1, 1, 1]],
    "west": [[0, 1, 0], [0, 0, 0], [0, 0, 1], [0, 1, 1]],
    "east": [[1, 1, 1], [1, 0, 1], [1, 0, 0], [1, 1, 0]],
}


def default_uv(f, t, d):
    return {"down": [f[0], 16 - t[2], t[0], 16 - f[2]], "up": [f[0], f[2], t[0], t[2]],
            "north": [16 - t[0], 16 - t[1], 16 - f[0], 16 - f[1]], "south": [f[0], 16 - t[1], t[0], 16 - f[1]],
            "west": [f[2], 16 - t[1], t[2], 16 - f[1]], "east": [16 - t[2], 16 - t[1], 16 - f[2], 16 - f[1]]}[d]


def mat_mul(a, b):
    return [[sum(a[i][k] * b[k][j] for k in range(3)) for j in range(3)] for i in range(3)]


def element_matrix(rot):
    rad = math.pi / 180
    if "angle" in rot:
        s, c = math.sin(rot["angle"] * rad), math.cos(rot["angle"] * rad)
        m = {"x": [[1, 0, 0], [0, c, -s], [0, s, c]], "y": [[c, 0, s], [0, 1, 0], [-s, 0, c]],
             "z": [[c, -s, 0], [s, c, 0], [0, 0, 1]]}[rot["axis"]]
    else:
        rx, ry, rz = (rot.get(k, 0) * rad for k in "xyz")
        mx = [[1, 0, 0], [0, math.cos(rx), -math.sin(rx)], [0, math.sin(rx), math.cos(rx)]]
        my = [[math.cos(ry), 0, math.sin(ry)], [0, 1, 0], [-math.sin(ry), 0, math.cos(ry)]]
        mz = [[math.cos(rz), -math.sin(rz), 0], [math.sin(rz), math.cos(rz), 0], [0, 0, 1]]
        m = mat_mul(mz, mat_mul(my, mx))
    if rot.get("rescale"):
        for axis in range(3):
            column = [m[r][axis] for r in range(3)]
            k = 1 / max(abs(v) for v in column)
            for r in range(3):
                m[r][axis] *= k
    return m


def resolve_textures(assets, model_id, seen=()):
    """The element list and the resolved texture table of a (flattened) model."""
    data = flatten_models.flatten_file(assets, model_id)
    chain = [data]
    node = data
    while node.get("parent") and not node["parent"].startswith("minecraft:") \
            and node["parent"] not in ("block/block", "item/generated"):
        node = flatten_models.flatten_file(assets, node["parent"])
        chain.append(node)
    textures, elements = {}, None
    for node in reversed(chain):
        textures.update(node.get("textures", {}))
    for node in chain:
        if elements is None and node.get("elements") is not None:
            elements = node["elements"]
    return elements or [], textures


def lookup(textures, ref):
    for _ in range(16):
        if not ref.startswith("#"):
            return ref
        ref = textures.get(ref[1:], "")
    return None


def bake(assets, model_id):
    """Quads of a model: (texture id, 4 positions in blocks, 4 uv pairs in 0..1)."""
    elements, textures = resolve_textures(assets, model_id)
    quads = []
    for e in elements:
        f, t = e["from"], e["to"]
        matrix = element_matrix(e["rotation"]) if e.get("rotation") else None
        origin = [v for v in e["rotation"].get("origin", [8, 8, 8])] if matrix else None
        for d, face in e["faces"].items():
            texture = lookup(textures, face.get("texture", ""))
            if texture is None:
                continue
            uv = face.get("uv") or default_uv(f, t, d)
            shift = face.get("rotation", 0) // 90
            pts, uvs = [], []
            for i, sel in enumerate(DIRECTIONS[d]):
                p = [t[a] if sel[a] else f[a] for a in range(3)]
                if matrix:
                    q = [p[a] - origin[a] for a in range(3)]
                    p = [sum(matrix[r][c] * q[c] for c in range(3)) + origin[r] for r in range(3)]
                j = (i + shift) % 4
                uvs.append([(uv[0] if j in (0, 1) else uv[2]) / 16, (uv[1] if j in (0, 3) else uv[3]) / 16])
                pts.append([v / 16 for v in p])
            quads.append((texture, pts, uvs))
    return quads


def transform(ops):
    """Compose [("t", x, y, z) | ("s", x, y, z) | ("ry", degrees)] applied outermost first into a point map."""
    def apply(p):
        for op in reversed(ops):
            if op[0] == "t":
                p = [p[i] + op[1 + i] for i in range(3)]
            elif op[0] == "s":
                p = [p[i] * op[1 + i] for i in range(3)]
            elif op[0] == "ry":
                c, s = math.cos(math.radians(op[1])), math.sin(math.radians(op[1]))
                p = [c * p[0] + s * p[2], p[1], -s * p[0] + c * p[2]]
            elif op[0] == "rxyz":
                rx, ry, rz = (math.radians(v) for v in op[1:4])
                m = mat_mul([[math.cos(rz), -math.sin(rz), 0], [math.sin(rz), math.cos(rz), 0], [0, 0, 1]],
                            mat_mul([[math.cos(ry), 0, math.sin(ry)], [0, 1, 0], [-math.sin(ry), 0, math.cos(ry)]],
                                    [[1, 0, 0], [0, math.cos(rx), -math.sin(rx)], [0, math.sin(rx), math.cos(rx)]]))
                p = [sum(m[r][c] * p[c] for c in range(3)) for r in range(3)]
        return p
    return apply


def polygon(pts, uvs):
    a = [pts[1][i] - pts[0][i] for i in range(3)]
    b = [pts[2][i] - pts[0][i] for i in range(3)]
    n = [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
    length = math.sqrt(sum(v * v for v in n)) or 1
    return ([[round(p[0] * 16, 4), round(p[1] * 16, 4), round(p[2] * 16, 4), round(u[0], 6), round(u[1], 6)]
             for p, u in zip(pts, uvs)], [round(v / length, 6) for v in n])


HEAD = [("t", 0, -0.25, 0), ("ry", 180), ("s", 0.625, -0.625, -0.625)]
FLIP = ("s", 1.02, -1.02, -1.02)
# item -> [(bone, item model, transform ops from the bone's origin)]; Mekanism's FreeRunnerArmor, ChestArmor
# and the head-item layer (with the scuba mask's own "head" display transform).
WORN = {
    "free_runners": [("left_leg", "item/left_free_runner", [("t", -0.65, 1, 0.525), FLIP]),
                     ("right_leg", "item/right_free_runner", [("t", -0.375, 1, 0.525), FLIP])],
    "free_runners_armored": [("left_leg", "item/left_free_runner_armored", [("t", -0.65, 1, 0.525), FLIP]),
                             ("right_leg", "item/right_free_runner_armored", [("t", -0.375, 1, 0.525), FLIP])],
    "jetpack": [("body", "item/jetpack", [("t", 0, 0.3, 0.145), FLIP, ("t", -0.5, -0.5, -0.5)])],
    "jetpack_armored": [("body", "item/jetpack_armored", [("t", 0, 0.3, 0.145), FLIP, ("t", -0.5, -0.5, -0.5)])],
    "scuba_tank": [("body", "item/scuba_tank", [("t", 0, 0.41, 0.28), ("s", 1, -1, -1), ("t", -0.5, -0.5, -0.5)])],
    "scuba_mask": [("head", "item/scuba_mask",
                    HEAD + [("ry", -180), ("s", 1.75, 1.75, 1.75), ("t", -0.5, -0.5, -0.5)])],
}
SLOT = {"free_runners": "feet", "free_runners_armored": "feet", "jetpack": "chest", "jetpack_armored": "chest",
        "scuba_tank": "chest", "scuba_mask": "head"}


def build(ns, item, assets, out, textures_dirs, report):
    """Write the worn models and equipment asset of `item`; returns True when it has any."""
    per_texture = {}
    for bone, model_id, ops in WORN[item]:
        fn = transform(ops)
        for texture, pts, uvs in bake(assets, f"{ns}:{model_id}"):
            poly, normal = polygon([fn(p) for p in pts], uvs)
            per_texture.setdefault(texture, {}).setdefault(bone, []).append((poly, normal))
    layers = []
    for n, (texture, bones) in enumerate(sorted(per_texture.items())):
        tns, _, tpath = texture.partition(":")
        name = f"{item}_{n}"
        children = {bone: {"pose": [0, 0, 0, 0, 0, 0, 1, 1, 1], "polygons": [p for p, _ in quads],
                           "normals": [n_ for _, n_ in quads]} for bone, quads in sorted(bones.items())}
        target = out / "assets" / ns / "horizon" / "worn_models" / f"{name}.json"
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(json.dumps({"pose": [0, 0, 0, 0, 0, 0, 1, 1, 1], "polygons": [], "children": children}))
        source = next((d / tns / "textures" / f"{tpath}.png" for d in textures_dirs
                       if (d / tns / "textures" / f"{tpath}.png").is_file()), None)
        if source is None:
            report.setdefault("worn_missing_textures", []).append(texture)
            continue
        sprite = out / "assets" / ns / "textures/entity/equipment/horizon_models" / f"{name}.png"
        sprite.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, sprite)
        layers.append({"model": f"{ns}:{name}", "texture": f"{ns}:{name}"})
    if not layers:
        return False
    target = out / "assets" / ns / "equipment" / f"{item}.json"
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(json.dumps({"layers": {"horizon_models": layers}}, indent=1) + "\n")
    report["worn_items"] = report.get("worn_items", 0) + 1
    return True


# ---- the MekaSuit: one OBJ model, parts picked per armour piece (`MekaSuitArmor`) ----------------------------
MEKASUIT = {"mekasuit_helmet": ("helmet", "head"), "mekasuit_bodyarmor": ("chest", "chest"),
            "mekasuit_pants": ("leggings", "legs"), "mekasuit_boots": ("boots", "feet")}
SLOT.update({item: slot for item, (_, slot) in MEKASUIT.items()})
# The bone each part name belongs to (`ModelPos`, in this order) and that bone's pivot in the player model, pixels.
BONES = (("head", "head", (0, 0, 0)), ("body", "body", (0, 0, 0)), ("left_arm", "left_arm", (5, 2, 0)),
         ("right_arm", "right_arm", (-5, 2, 0)), ("left_leg", "left_leg", (1.9, 12, 0)),
         ("right_leg", "right_leg", (-1.9, 12, 0)))


def mekasuit_points(point, pivot):
    """A vertex of the OBJ (a standing player in blocks, y up, left arm at -x) in the bone's space (blocks, y down)."""
    x, y, z = point
    return [-x - pivot[0] / 16, 1.5 - y - pivot[1] / 16, z - pivot[2] / 16]


# The piece whose presence hides a piece's exclusive parts (`MekaSuitArmor(type, adjacentType)`).
ADJACENT = {"mekasuit_helmet": "mekasuit_bodyarmor", "mekasuit_bodyarmor": "mekasuit_helmet",
            "mekasuit_pants": "mekasuit_boots", "mekasuit_boots": "mekasuit_pants"}


def build_mekasuit(ns, item, assets, out, textures_dirs, report):
    """Write the worn model and equipment asset of one MekaSuit piece; exclusive (`excl_`) parts, which show only
    while the neighbouring piece is not worn, are left out; shared parts are drawn by every piece that owns them."""
    keyword, _ = MEKASUIT[item]
    root = next((d / ns / "models/entity" for d in assets if (d / ns / "models/entity/mekasuit.obj").is_file()), None)
    if root is None:
        return False
    objs = obj_models.parse((root / "mekasuit.obj").read_text())
    mtl = obj_models.parse_mtl((root / "mekasuit.mtl").read_text())
    textures = json.loads((root / "mekasuit.json").read_text())["textures"]
    per_texture = {}
    for obj in objs:
        name = obj["name"]
        exclusive = name.startswith("excl_")
        if keyword not in name:
            continue
        bone = next((b for key, b, _ in BONES if key in name), None)
        pivot = next((p for key, _, p in BONES if key in name), None)
        if bone is None:
            report.setdefault("mekasuit_unplaced", []).append(name)
            continue
        for quad, material in zip(obj["faces"], obj["materials"]):
            var = mtl.get(material, ("#" + str(material), False))[0][1:]
            texture = textures.get(var)
            if texture is None or len(quad) != 4:
                continue
            pts = [mekasuit_points(p, pivot) for p, _ in quad]
            uvs = [[uv[0], 1 - uv[1]] for _, uv in quad]
            poly, normal = polygon(pts, uvs)
            per_texture.setdefault((texture, exclusive), {}).setdefault(bone, []).append((poly, normal))
    hide = {True: [f"{ns}:{ADJACENT[item]}"], False: []}
    return write_layers(ns, item, per_texture, out, textures_dirs, report, hide)


def write_layers(ns, item, per_texture, out, textures_dirs, report, hide=None):
    layers = []
    for n, (key, bones) in enumerate(sorted(per_texture.items())):
        texture, exclusive = key
        tns, _, tpath = texture.partition(":")
        name = f"{item}_{n}"
        children = {bone: {"pose": [0, 0, 0, 0, 0, 0, 1, 1, 1], "polygons": [p for p, _ in quads],
                           "normals": [n_ for _, n_ in quads]} for bone, quads in sorted(bones.items())}
        target = out / "assets" / ns / "horizon" / "worn_models" / f"{name}.json"
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(json.dumps({"pose": [0, 0, 0, 0, 0, 0, 1, 1, 1], "polygons": [], "children": children}))
        source = next((d / tns / "textures" / f"{tpath}.png" for d in textures_dirs
                       if (d / tns / "textures" / f"{tpath}.png").is_file()), None)
        if source is None:
            report.setdefault("worn_missing_textures", []).append(texture)
            continue
        sprite = out / "assets" / ns / "textures/entity/equipment/horizon_models" / f"{name}.png"
        sprite.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, sprite)
        layer = {"model": f"{ns}:{name}", "texture": f"{ns}:{name}"}
        if exclusive and hide:
            layer["hide_with"] = hide[True]
        layers.append(layer)
    if not layers:
        return False
    target = out / "assets" / ns / "equipment" / f"{item}.json"
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(json.dumps({"layers": {"horizon_models": layers}}, indent=1) + "\n")
    report["worn_items"] = report.get("worn_items", 0) + 1
    return True


def build_world_model(ns, name, obj_file, texture, out, textures_dirs, report):
    """A part tree for `native_world_models`: the OBJ as it stands (pixels, y up), its texture beside it."""
    objs = obj_models.parse(obj_file.read_text())
    mtl_file = obj_file.with_suffix(".mtl")
    mtl = obj_models.parse_mtl(mtl_file.read_text() if mtl_file.is_file() else "")
    polygons, normals = [], []
    for obj in objs:
        for quad in obj["faces"]:
            if len(quad) != 4:
                continue
            poly, normal = polygon([list(p) for p, _ in quad], [[uv[0], 1 - uv[1]] for _, uv in quad])
            polygons.append(poly)
            normals.append(normal)
    tns, _, tpath = texture.partition(":")
    source = next((d / tns / "textures" / f"{tpath}.png" for d in textures_dirs
                   if (d / tns / "textures" / f"{tpath}.png").is_file()), None)
    if source is None or not polygons:
        report.setdefault("worn_missing_textures", []).append(texture)
        return False
    target = out / "assets" / ns / "horizon" / "world_models" / f"{name}.json"
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(json.dumps({"pose": [0, 0, 0, 0, 0, 0, 1, 1, 1], "polygons": polygons, "normals": normals}))
    sprite = out / "assets" / ns / "textures/entity/equipment/horizon_models" / f"{name}.png"
    sprite.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, sprite)
    report["world_models"] = report.get("world_models", 0) + 1
    return True


def build_cube_world_model(ns, name, half, texture_png, out, report):
    """A part tree for `native_world_models`: a cube of side `2 * half` blocks about its centre, the whole
    texture on every face (Mekanism's `energy_core` element at 0.4 scale)."""
    h = half
    faces = [  # counter-clockwise seen from outside
        [(-h, -h, h), (h, -h, h), (h, h, h), (-h, h, h)],       # south
        [(h, -h, -h), (-h, -h, -h), (-h, h, -h), (h, h, -h)],   # north
        [(h, -h, h), (h, -h, -h), (h, h, -h), (h, h, h)],       # east
        [(-h, -h, -h), (-h, -h, h), (-h, h, h), (-h, h, -h)],   # west
        [(-h, h, h), (h, h, h), (h, h, -h), (-h, h, -h)],       # up
        [(-h, -h, -h), (h, -h, -h), (h, -h, h), (-h, -h, h)],   # down
    ]
    uvs = [[0, 1], [1, 1], [1, 0], [0, 0]]
    polygons, normals = [], []
    for quad in faces:
        poly, normal = polygon([list(p) for p in quad], uvs)
        polygons.append(poly)
        normals.append(normal)
    target = out / "assets" / ns / "horizon" / "world_models" / f"{name}.json"
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(json.dumps({"pose": [0, 0, 0, 0, 0, 0, 1, 1, 1], "polygons": polygons, "normals": normals}))
    sprite = out / "assets" / ns / "textures/entity/equipment/horizon_models" / f"{name}.png"
    sprite.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(texture_png, sprite)
    report["world_models"] = report.get("world_models", 0) + 1


# Vanilla's FaceInfo: the four corners of each face, as (x, y, z) picks of min (0) or max (1).
FACE_CORNERS = {
    "down": [(0, 0, 1), (0, 0, 0), (1, 0, 0), (1, 0, 1)],
    "up": [(0, 1, 0), (0, 1, 1), (1, 1, 1), (1, 1, 0)],
    "north": [(1, 1, 0), (1, 0, 0), (0, 0, 0), (0, 1, 0)],
    "south": [(0, 1, 1), (0, 0, 1), (1, 0, 1), (1, 1, 1)],
    "west": [(0, 1, 0), (0, 0, 0), (0, 0, 1), (0, 1, 1)],
    "east": [(1, 1, 1), (1, 0, 1), (1, 0, 0), (1, 1, 0)],
}


def default_uv(direction, lo, hi):
    if direction in ("north", "south"):
        return [lo[0], 16 - hi[1], hi[0], 16 - lo[1]]
    if direction in ("east", "west"):
        return [lo[2], 16 - hi[1], hi[2], 16 - lo[1]]
    if direction == "up":
        return [lo[0], lo[2], hi[0], hi[2]]
    return [lo[0], 16 - hi[2], hi[0], 16 - lo[2]]


def rotate_point(p, rotation):
    """`p` (pixels) turned about the element's rotation axis (degrees) around its origin."""
    axis = next((a for a in "xyz" if a in rotation), None)
    if axis is None:
        return p
    angle = math.radians(rotation[axis])
    o = rotation.get("origin", [8, 8, 8])
    q = [p[i] - o[i] for i in range(3)]
    c, s = math.cos(angle), math.sin(angle)
    if axis == "x":
        q = [q[0], q[1] * c - q[2] * s, q[1] * s + q[2] * c]
    elif axis == "y":
        q = [q[0] * c + q[2] * s, q[1], -q[0] * s + q[2] * c]
    else:
        q = [q[0] * c - q[1] * s, q[0] * s + q[1] * c, q[2]]
    return [q[i] + o[i] for i in range(3)]


def build_element_world_model(ns, name, model, texture_png, out, report, center=(8, 8, 8)):
    """A part tree for `native_world_models` from an element model with one texture, centred on the block's
    middle (pixels, y up), so the game can turn it about the block's centre."""
    polygons, normals = [], []
    for el in model["elements"]:
        lo, hi = el["from"], el["to"]
        for direction, face in el["faces"].items():
            u0, v0, u1, v1 = face.get("uv") or default_uv(direction, lo, hi)
            turn = int(face.get("rotation", 0)) // 90 % 4
            pts, uvs = [], []
            for i, pick in enumerate(FACE_CORNERS[direction]):
                p = [hi[k] if pick[k] else lo[k] for k in range(3)]
                p = rotate_point(p, el.get("rotation", {}))
                pts.append([(p[k] - center[k]) / 16 for k in range(3)])
                corner = (i + turn) % 4
                uvs.append([(u0 if corner < 2 else u1) / 16, (v0 if corner in (0, 3) else v1) / 16])
            poly, normal = polygon(pts, uvs)
            polygons.append(poly)
            normals.append(normal)
    target = out / "assets" / ns / "horizon" / "world_models" / f"{name}.json"
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(json.dumps({"pose": [0, 0, 0, 0, 0, 0, 1, 1, 1], "polygons": polygons, "normals": normals}))
    sprite = out / "assets" / ns / "textures/entity/equipment/horizon_models" / f"{name}.png"
    sprite.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(texture_png, sprite)
    report["world_models"] = report.get("world_models", 0) + 1


def _entity_cube(origin, size, tex, tex_size=(16, 16)):
    """The polygons of a vanilla `ModelPart.Cube` (px, y up as the model is drawn unflipped): four points and
    their uv (0..1, v down) per face, in `Cube`'s own vertex order."""
    (x0, y0, z0), (w, h, d) = origin, size
    x1, y1, z1 = x0 + w, y0 + h, z0 + d
    v = [(x0, y0, z0), (x1, y0, z0), (x1, y1, z0), (x0, y1, z0),
         (x0, y0, z1), (x1, y0, z1), (x1, y1, z1), (x0, y1, z1)]
    u, vv = tex
    f1, f2, f3, f4 = u + d, u + d + w, u + 2 * d + w, u + 2 * d + 2 * w
    f5, f6, f7 = vv, vv + d, vv + d + h
    # (vertex indices, u0, v0, u1, v1) of each face; Polygon puts (u1, v0), (u0, v0), (u0, v1), (u1, v1) on 0..3.
    faces = [((5, 4, 0, 1), f1, f5, f2, f6), ((2, 3, 7, 6), f2, f6, f3, f5),
             ((0, 4, 7, 3), u, f6, f1, f7), ((1, 0, 3, 2), f1, f6, f2, f7),
             ((5, 1, 2, 6), f2, f6, f3, f7), ((4, 5, 6, 7), f3, f6, f4, f7)]
    out = []
    for idx, a, b, c, e in faces:
        uv = [(c, b), (a, b), (a, e), (c, e)]
        out.append(([v[i] for i in idx], [(p / tex_size[0], q / tex_size[1]) for p, q in uv]))
    return out


# `ModelTurbine`: (arm rotation axis, sign) and its boxes: (origin, size, texOffs, is the blade).
_TURBINE_ARMS = [
    ("z", 1, [((-1, 0, -4), (2, 1, 3), (0, 9), False), ((-1.5, 0, -8), (3, 1, 4), (0, 0), True)]),
    ("x", -1, [((1, 0, -1), (3, 1, 2), (0, 13), False), ((4, 0, -1.5), (4, 1, 3), (0, 5), True)]),
    ("z", -1, [((-1, 0, 1), (2, 1, 3), (0, 9), False), ((-1.5, 0, 4), (3, 1, 4), (0, 0), True)]),
    ("x", 1, [((-4, 0, -1), (3, 1, 2), (0, 13), False), ((-8, 0, -1.5), (4, 1, 3), (0, 5), True)]),
]


def build_turbine_blade(ns, index, texture_png, out, report):
    """One turbine blade pair's arms (`ModelTurbine.setupAnim` for blade `index`) about the shaft, as a world model
    whose origin is the arms' pivot."""
    bend = 0.418879
    scale = index * 0.5
    adjusted = scale / 16
    # west, east: x grows by `scale`, z by `adjusted`, pushed out by 2 px per index; north, south the other way.
    blade_of = {0: (adjusted, scale, 0.0, -2.0), 1: (scale, adjusted, 2.0, 0.0),
                2: (adjusted, scale, 0.0, 2.0), 3: (scale, adjusted, -2.0, 0.0)}
    polygons, normals = [], []
    turn = math.radians(5 * index)
    for arm_index, (axis, sign, boxes) in enumerate(_TURBINE_ARMS):
        sx, sz, tx, tz = blade_of[arm_index]
        angle = bend * sign
        c, s = math.cos(angle), math.sin(angle)
        for origin, size, tex, is_blade in boxes:
            for pts, uvs in _entity_cube(origin, size, tex):
                final = []
                for p in pts:
                    x, y, z = p
                    if is_blade:
                        x, z = x * (1 + sx) - tx * index, z * (1 + sz) - tz * index
                    if axis == "z":
                        x, y = x * c - y * s, x * s + y * c
                    else:
                        y, z = y * c - z * s, y * s + z * c
                    ct, st = math.cos(turn), math.sin(turn)
                    x, z = x * ct + z * st, -x * st + z * ct
                    final.append([x / 16, y / 16, z / 16])
                poly, normal = polygon(final, uvs)
                polygons.append(poly)
                normals.append(normal)
    target = out / "assets" / ns / "horizon" / "world_models" / f"turbine_blade_{index}.json"
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(json.dumps({"pose": [0, 0, 0, 0, 0, 0, 1, 1, 1], "polygons": polygons, "normals": normals}))
    sprite = out / "assets" / ns / "textures/entity/equipment/horizon_models" / f"turbine_blade_{index}.png"
    sprite.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(texture_png, sprite)
    report["world_models"] = report.get("world_models", 0) + 1
