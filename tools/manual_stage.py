#!/usr/bin/env python3
"""Type-check the bundle's glue without horizon-mixins: a hand-rolled stand-in for `horizon-mixins apply`.

Usage: git -C ../horizon archive <pinned rev> | tar -x -C STAGE
       python3 tools/manual_stage.py STAGE mixins/mekanism
       (cd STAGE && cargo check -p horizon-client --bin horizon-client --no-default-features)

It does what the manifest asks in the simplest way: stages the `core` crate as a workspace member and a
dependency of horizon-client, appends runtime.rs as `game::mekanism`, and calls each helper at the HEAD
of its named hook (only the hooks in HOOKS below). The real build is `tools/horizon.py` (horizon-mixins)
or the launcher; use this when those are not at hand. Run it again after an edit: it replaces what it
added before.
"""
import sys, shutil, pathlib, re
stage, bundle = map(pathlib.Path, sys.argv[1:3])
dst = stage / 'native-crates/mekanism/core'
shutil.rmtree(dst, ignore_errors=True); shutil.copytree(bundle / 'core', dst, ignore=shutil.ignore_patterns('target'))
c = (stage / 'Cargo.toml').read_text()
if 'native-crates/mekanism/core' not in c:
    c = c.replace('members = [\n', 'members = [\n    "native-crates/mekanism/core",\n', 1)
    (stage / 'Cargo.toml').write_text(c)
cc = stage / 'crates/horizon-client/Cargo.toml'; t = cc.read_text()
if 'mekanism-core' not in t:
    t = t.replace('[dependencies]\n', '[dependencies]\nmekanism-core = { path = "../../native-crates/mekanism/core" }\n', 1); cc.write_text(t)
g = stage / 'crates/horizon-client/src/game.rs'; t = g.read_text()
t = re.sub(r'\n// BEGIN mekanism module.*', '', t, flags=re.S)
t += '\n// BEGIN mekanism module\npub(crate) mod mekanism {\n' + (bundle / 'runtime.rs').read_text() + '}\n'
g.write_text(t)
import tomllib
m = tomllib.loads((bundle / 'horizon-mixin.toml').read_text())
import json
HOOKS = {h['name']: h['target'].rsplit('::', 1)[1] for h in json.loads((stage / 'native-hooks.json').read_text())['hooks']}
hooks = HOOKS
n = stage / 'crates/horizon-client/src/game/native_hooks.rs'; t = n.read_text()
t = re.sub(r'\n// BEGIN mekanism helper.*', '', t, flags=re.S)
t = re.sub(r'        mekanism_hook_[a-z_]+\(.*?\);\n', '', t)
for inj in m.get('injection', []):
    fn = hooks[inj['hook']]
    helper = (bundle / inj['helper']).read_text()
    name = re.search(r'fn (\w+)', helper).group(1)
    new = f'mekanism_hook_{name}'
    t += f'\n// BEGIN mekanism helper\n' + helper.replace(f'fn {name}', f'fn {new}', 1)
    args = ', '.join(b['value'] for b in inj['bindings'])
    # An empty hook body (`fn native_tick(&mut self) {}`) opens up to take the call.
    t = re.sub(r'(pub\(super\) fn ' + fn + r'\([^{]*)\{\}\n', r'\1{\n    }\n', t, count=1)
    t = re.sub(r'(pub\(super\) fn ' + fn + r'\([^{]*\{\n)', lambda mm: mm.group(1) + f'        {new}({args});\n', t, count=1)
n.write_text(t)
print('staged')
