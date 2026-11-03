#!/usr/bin/env python3
"""Wavefront OBJ models made of boxes (Blockbench exports) as plain element models.

NeoForge's `neoforge:obj` loader draws a model's quads as they are; Horizon bakes cuboid elements.  Mekanism's OBJ
models (wind generator, MekaSuit, Meka-Tool) are Blockbench boxes: every `o` object has eight vertices and up to six
quads.  `elements(text, ...)` turns each box into an element: the box in its own frame, turned into place with an
Euler rotation (`rotation: {x, y, z, origin}`), each quad's texture rectangle recovered exactly the way Horizon's
`bake_face` lays out a face (rectangle and quarter turns).  Objects that are not boxes are reported, not drawn.
"""
import itertools
import math
import sys
from pathlib import Path

DIRS = ("down", "up", "north", "south", "west", "east")
NORMALS = {"down": (0, -1, 0), "up": (0, 1, 0), "north": (0, 0, -1), "south": (0, 0, 1),
           "west": (-1, 0, 0), "east": (1, 0, 0)}
# Horizon's `corners()`: the corner each of a face's four vertices sits at (0 = from, 1 = to, per axis).
SELECTORS = {
    "down": [(0, 0, 1), (0, 0, 0), (1, 0, 0), (1, 0, 1)],
    "up": [(0, 1, 0), (0, 1, 1), (1, 1, 1), (1, 1, 0)],
    "north": [(1, 1, 0), (1, 0, 0), (0, 0, 0), (0, 1, 0)],
    "south": [(0, 1, 1), (0, 0, 1), (1, 0, 1), (1, 1, 1)],
    "west": [(0, 1, 0), (0, 0, 0), (0, 0, 1), (0, 1, 1)],
    "east": [(1, 1, 1), (1, 0, 1), (1, 0, 0), (1, 1, 0)],
}


def parse_mtl(text):
    """{material: (texture variable, emissive)}: `map_Kd #var`; `Ka` other than 0 is NeoForge's full bright."""
    out, name = {}, None
    for line in (text or "").splitlines():
        parts = line.split()
        if not parts:
            continue
        if parts[0] == "newmtl":
            name = " ".join(parts[1:])
            out[name] = ["#" + name, False]
        elif parts[0] == "map_Kd" and name:
            out[name][0] = parts[1] if parts[1].startswith("#") else "#" + name
        elif parts[0] == "Ka" and name:
            out[name][1] = any(float(x) > 0 for x in parts[1:4])
    return {k: tuple(v) for k, v in out.items()}


def parse(text):
    verts, uvs, objects = [], [], []
    material = None
    for line in text.splitlines():
        parts = line.split()
        if not parts:
            continue
        if parts[0] == "usemtl":
            material = " ".join(parts[1:])
        elif parts[0] == "o":
            objects.append({"name": " ".join(parts[1:]), "faces": [], "materials": []})
        elif parts[0] == "v":
            verts.append(tuple(float(x) for x in parts[1:4]))
        elif parts[0] == "vt":
            uvs.append((float(parts[1]), float(parts[2])))
        elif parts[0] == "f":
            if not objects:
                objects.append({"name": "main", "faces": [], "materials": []})
            quad = []
            for v in parts[1:]:
                bits = v.split("/")
                quad.append((verts[int(bits[0]) - 1], uvs[int(bits[1]) - 1] if len(bits) > 1 and bits[1] else (0.0, 0.0)))
            objects[-1]["faces"].append(quad)
            objects[-1]["materials"].append(material)
    return objects


def sub(a, b):
    return tuple(x - y for x, y in zip(a, b))


def dot(a, b):
    return sum(x * y for x, y in zip(a, b))


def cross(a, b):
    return (a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0])


def unit(a):
    n = math.sqrt(dot(a, a))
    return tuple(x / n for x in a) if n > 1e-12 else None


def face_normal(quad):
    pts = [p for p, _ in quad]
    return unit(cross(sub(pts[1], pts[0]), sub(pts[2], pts[0])))


def frame_of(obj):
    """Orthonormal local axes (as world vectors, closest to x, y, z, right-handed) of a box, or None."""
    normals = [n for n in (face_normal(f) for f in obj["faces"] if len(f) == 4) if n]
    axes = []
    for n in normals:
        if not any(abs(dot(n, a)) > 0.999 for a in axes):
            axes.append(n)
    if len(axes) < 2:
        return None
    a, b = axes[0], axes[1]
    if abs(dot(a, b)) > 1e-3:
        return None
    c = unit(cross(a, b))
    cand = [a, b, c]
    if len(axes) > 2 and abs(abs(dot(axes[2], c)) - 1) > 1e-3:
        return None
    # Assign the candidates to x, y, z by closeness to the world axes.
    best = None
    for perm in itertools.permutations(range(3)):
        for signs in itertools.product((1, -1), repeat=3):
            m = [tuple(signs[i] * x for x in cand[perm[i]]) for i in range(3)]
            if dot(cross(m[0], m[1]), m[2]) < 0:
                continue
            score = sum(m[i][i] for i in range(3))
            if best is None or score > best[0] + 1e-9:
                best = (score, m)
    return best[1]


def euler_of(axes):
    """Degrees (x, y, z) such that Horizon's R = Rz(z) Ry(y) Rx(x) has `axes` as its columns."""
    r = [[axes[j][i] for j in range(3)] for i in range(3)]  # r[row][column]
    sy = max(-1.0, min(1.0, -r[2][0]))
    y = math.asin(sy)
    if abs(sy) < 0.999999:
        x = math.atan2(r[2][1], r[2][2])
        z = math.atan2(r[1][0], r[0][0])
    else:
        x = 0.0
        z = math.atan2(-r[0][1], r[1][1])
    return [math.degrees(x), math.degrees(y), math.degrees(z)]


def predict(direction, rect, rotation):
    """Horizon's UV of the four vertices of a face (bake_face): vertex i gets corner (i + rotation) % 4."""
    out = []
    for i in range(4):
        corner = (i + rotation) % 4
        out.append((rect[0 if corner < 2 else 2], rect[1 if corner in (0, 3) else 3]))
    return out


def face_of(direction, local_quad, lo, hi, flip_v):
    """({texture rect, rotation}) for one quad whose vertices are in the box frame, or None."""
    observed = {}
    for (p, (u, v)) in local_quad:
        sel = []
        for i in range(3):
            if abs(p[i] - lo[i]) < 1e-4:
                sel.append(0)
            elif abs(p[i] - hi[i]) < 1e-4:
                sel.append(1)
            else:
                return None
        observed[tuple(sel)] = (u * 16, (1 - v if flip_v else v) * 16)
    if set(observed) != set(SELECTORS[direction]):
        return None
    want = [observed[s] for s in SELECTORS[direction]]
    us = sorted({round(w[0], 6) for w in want})
    vs = sorted({round(w[1], 6) for w in want})
    if len(us) > 2 or len(vs) > 2:
        return None
    us = us * 2 if len(us) == 1 else us
    vs = vs * 2 if len(vs) == 1 else vs
    for u0, u1 in itertools.permutations(us):
        for v0, v1 in itertools.permutations(vs):
            for rot in range(4):
                got = predict(direction, (u0, v0, u1, v1), rot)
                if all(abs(g[0] - w[0]) < 1e-3 and abs(g[1] - w[1]) < 1e-3 for g, w in zip(got, want)):
                    return {"uv": [u0, v0, u1, v1], "rotation": rot * 90}
    return None


def element(obj, flip_v, shift, mtl, scale=16.0):
    """One element for a box object, or (None, reason)."""
    frame = frame_of(obj)
    if frame is None:
        return None, "no frame"
    pts = [p for f in obj["faces"] for p, _ in f]
    centre = tuple(sum(p[i] for p in pts) / len(pts) for i in range(3))
    # An object with faces only on some sides: the centre must still be the box's.
    local = [tuple(dot(sub(p, centre), frame[i]) for i in range(3)) for p in pts]
    lo = [min(q[i] for q in local) for i in range(3)]
    hi = [max(q[i] for q in local) for i in range(3)]
    mid = [(lo[i] + hi[i]) / 2 for i in range(3)]
    centre = tuple(centre[i] + sum(mid[j] * frame[j][i] for j in range(3)) for i in range(3))
    local = [tuple(dot(sub(p, centre), frame[i]) for i in range(3)) for p in pts]
    lo = [min(q[i] for q in local) for i in range(3)]
    hi = [max(q[i] for q in local) for i in range(3)]
    faces = {}
    emissive = []
    for quad, material in zip(obj["faces"], obj["materials"]):
        if len(quad) != 4:
            return None, "not a quad"
        n = face_normal(quad)
        if n is None:
            continue
        ln = tuple(dot(n, frame[i]) for i in range(3))
        d = max(DIRS, key=lambda name: dot(ln, NORMALS[name]))
        if dot(ln, NORMALS[d]) < 0.999:
            return None, "not a box"
        lq = [(tuple(dot(sub(p, centre), frame[i]) for i in range(3)), uv) for p, uv in quad]
        face = face_of(d, lq, lo, hi, flip_v)
        if face is None:
            return None, f"face {d}"
        var, glow = mtl.get(material, ("#" + str(material), False))
        face["texture"] = var
        emissive.append(glow)
        if face["rotation"] == 0:
            del face["rotation"]
        faces[d] = face
    c = [(centre[i] + shift[i]) * scale for i in range(3)]
    el = {"name": obj["name"],
          "from": [round(c[i] + lo[i] * scale, 5) for i in range(3)],
          "to": [round(c[i] + hi[i] * scale, 5) for i in range(3)],
          "faces": faces}
    if emissive and all(emissive):
        el["light_emission"] = 15
    x, y, z = (round(a, 5) for a in euler_of(frame))
    if abs(x) > 1e-4 or abs(y) > 1e-4 or abs(z) > 1e-4:
        el["rotation"] = {"x": x, "y": y, "z": z, "origin": [round(v, 5) for v in c]}
    return el, None


def elements(text, flip_v=True, shift=(0.0, 0.0, 0.0), mtl_text=""):
    """(elements, problems) for the OBJ `text`; `shift` in blocks is added to every vertex."""
    out, problems = [], []
    mtl = parse_mtl(mtl_text)
    for obj in parse(text):
        el, why = element(obj, flip_v, shift, mtl)
        if el is None:
            problems.append(f"{obj['name']}: {why}")
        else:
            out.append(el)
    return out, problems


if __name__ == "__main__":
    for f in sys.argv[1:]:
        els, problems = elements(Path(f).read_text())
        print(f, "elements", len(els), "problems", problems[:6], len(problems))
