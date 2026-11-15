#!/usr/bin/env python3
"""Flatten NeoForge `neoforge:composite` models into plain element models, which Horizon bakes.

A composite model is a list of named child models drawn together.  The result has every child's elements;
a child's texture variables are renamed `<child>__<name>` (and the references to them in its elements) so
two children can bind the same variable to different textures.  The composite's own parent (display
transforms), `textures`, `display` and `gui_light` stay.  Child `visibility` is not applied.
"""
import json
from pathlib import Path

import obj_models


def model_path(assets, model_id):
    """`assets`: the folders that hold `<namespace>/models/`; the first with the model wins."""
    ns, _, path = model_id.partition(":")
    if not path:
        ns, path = "minecraft", model_id
    for root in assets:
        f = root / ns / "models" / f"{path}.json"
        if f.is_file():
            return f
    return assets[0] / ns / "models" / f"{path}.json"


def load(assets, model_id):
    f = model_path(assets, model_id)
    return json.loads(f.read_text()) if f.is_file() else None


def obj_model(assets, data):
    """(elements, textures, problems) of a `neoforge:obj` model: the OBJ's boxes as elements."""
    ns, _, path = data["model"].partition(":")
    for root in assets:
        f = root / ns / path
        if f.is_file():
            mtl = f.with_suffix(".mtl")
            shift = (data.get("transform") or {}).get("translation", [0, 0, 0])
            elements, problems = obj_models.elements(f.read_text(), bool(data.get("flip_v")), tuple(shift),
                                                     mtl.read_text() if mtl.is_file() else "")
            return elements, dict(data.get("textures", {})), problems
    return [], dict(data.get("textures", {})), [f"{data['model']}: not found"]


def flatten_file(assets, model_id, seen=()):
    """The model `model_id` with composite loaders resolved (written back to disk when it changed)."""
    f = model_path(assets, model_id)
    data = json.loads(f.read_text())
    if data.get("loader") == "neoforge:obj":
        elements, textures, problems = obj_model(assets, data)
        if problems:
            return data
        out = {k: v for k, v in data.items() if k not in ("loader", "model", "flip_v", "textures", "transform")}
        out["textures"] = textures
        out["elements"] = elements
        return out
    if data.get("loader") != "neoforge:composite":
        return data
    if model_id in seen:
        raise ValueError(f"composite cycle at {model_id}")
    elements = list(data.get("elements", []))
    textures = dict(data.get("textures", {}))
    for name, child in data.get("children", {}).items():
        child_elements, child_textures = resolve(assets, child, seen + (model_id,))
        prefix = f"{name}__"
        for key, value in child_textures.items():
            textures[prefix + key] = "#" + prefix + value[1:] if value.startswith("#") else value
        for element in child_elements:
            element = json.loads(json.dumps(element))
            for face in element.get("faces", {}).values():
                if face.get("texture", "").startswith("#"):
                    face["texture"] = "#" + prefix + face["texture"][1:]
            element["name"] = f"{name}/{element.get('name', 'element')}"
            elements.append(element)
    out = {k: v for k, v in data.items() if k not in ("loader", "children", "textures", "elements")}
    out["textures"] = textures
    out["elements"] = elements
    return out


def resolve(assets, child, seen):
    """(elements, textures) of a composite child: its own, over those of its parent chain."""
    if child.get("loader") == "neoforge:obj":
        elements, textures, problems = obj_model(assets, child)
        if problems:
            raise ValueError("; ".join(problems[:3]))
        return elements, textures
    textures = {}
    elements = None
    chain = []
    node = child
    while node is not None:
        chain.append(node)
        parent = node.get("parent")
        if not parent or parent.startswith("minecraft:") or parent in ("block/block", "item/generated"):
            node = None
        else:
            node = flatten_file(assets, parent, seen)
    for node in chain:
        if elements is None and node.get("elements") is not None:
            elements = node["elements"]
    for node in reversed(chain):
        textures.update(node.get("textures", {}))
    return elements or [], textures


def flatten_tree(out_ns_assets, assets):
    """Rewrite the composite models of `out_ns_assets` (`.../assets/<ns>`); returns how many."""
    count = 0
    models = out_ns_assets / "models"
    for path in sorted(models.rglob("*.json")):
        try:
            data = json.loads(path.read_text())
        except ValueError:
            continue
        if data.get("loader") not in ("neoforge:composite", "neoforge:obj"):
            continue
        model_id = f"{out_ns_assets.name}:{path.relative_to(models).with_suffix('').as_posix()}"
        flat = flatten_file(assets, model_id)
        if flat.get("loader"):
            continue  # an OBJ that is not made of boxes (the transmitters): left to its own code
        path.write_text(json.dumps(flat, indent=1) + "\n")
        count += 1
    return count
