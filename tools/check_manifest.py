#!/usr/bin/env python3
"""Fail when the bundle's horizon-mixin.toml is empty, unparsable or out of step with its glue files.

Checks that the manifest parses, has the [mod], [compatibility], crate and module tables and at least one injection,
that its horizon_revision is the pin in compatibility.json, that every injection's helper file exists, and that every
helper file in the bundle directory is used by an injection (so a truncated manifest cannot pass unnoticed).

Usage: python3 tools/check_manifest.py [bundle-dir]   (default mixins/mekanism)
"""
import json
import pathlib
import sys
import tomllib

ROOT = pathlib.Path(__file__).resolve().parents[1]


def check(bundle):
    bundle = pathlib.Path(bundle)
    path = bundle / "horizon-mixin.toml"
    errors = []
    if not path.is_file() or path.stat().st_size == 0:
        return [f"{path} is missing or empty"]
    try:
        manifest = tomllib.loads(path.read_text())
    except tomllib.TOMLDecodeError as error:
        return [f"{path} does not parse: {error}"]
    for table in ("mod", "compatibility", "crate", "module", "injection"):
        if not manifest.get(table):
            errors.append(f"{path}: no [{table}] table")
    pin = json.loads((ROOT / "compatibility.json").read_text())["horizon_revision"]
    if manifest.get("compatibility", {}).get("horizon_revision") != pin:
        errors.append(f"{path}: horizon_revision differs from compatibility.json ({pin})")
    helpers = {inj.get("helper") for inj in manifest.get("injection", [])}
    for helper in sorted(h for h in helpers if h):
        if not (bundle / helper).is_file():
            errors.append(f"injection helper {helper} does not exist")
    modules = {m.get("source") for m in manifest.get("module", [])}
    for file in sorted(bundle.glob("*.rs")):
        if file.name not in helpers and file.name not in modules:
            errors.append(f"{file.name} is not used by any injection or module in the manifest")
    return errors


if __name__ == "__main__":
    problems = check(sys.argv[1] if len(sys.argv) > 1 else ROOT / "mixins/mekanism")
    for problem in problems:
        print(f"check_manifest: {problem}", file=sys.stderr)
    raise SystemExit(1 if problems else 0)
