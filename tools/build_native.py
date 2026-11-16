#!/usr/bin/env python3
"""Check or build the Mekanism native mod (mods/mekanism) on the upstream Horizon mod API.

  tools/build_native.py check                 # cargo horizon-mod check (fetches the pinned SDK on first run)
  tools/build_native.py build [--instance N]  # horizon-launcher native-build, asks for native-code trust
  tools/build_native.py dev-install HORIZON   # local, unsigned base from a checkout of SCPLEGION/horizon

It replaces tools/manual_stage.py and tools/horizon.py, which staged the old hook bundle (mixins/mekanism) into a
fork whose native-hooks.json upstream removed. The old bundle stays in the tree until the port is complete.
"""
import argparse
import pathlib
import subprocess
import sys

MOD = pathlib.Path(__file__).resolve().parents[1] / "mods" / "mekanism"


def run(cmd, **kw):
    print("+", " ".join(map(str, cmd)), flush=True)
    return subprocess.run(cmd, check=True, **kw)


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)
    sub.add_parser("check")
    b = sub.add_parser("build")
    b.add_argument("--instance")
    d = sub.add_parser("dev-install")
    d.add_argument("horizon", type=pathlib.Path)
    a = p.parse_args()
    if a.cmd == "check":
        run(["cargo", "horizon-mod", "check"], cwd=MOD)
    elif a.cmd == "build":
        cmd = ["horizon-launcher", "native-build", str(MOD), "--trust-native-code"]
        if a.instance:
            cmd += ["--instance", a.instance]
        run(cmd)
    else:
        run(["horizon-launcher", "developer-install", str(a.horizon), "--program",
             str(a.horizon / "target/debug/horizon-client"), "--trust-unsigned"])


if __name__ == "__main__":
    sys.exit(main())
