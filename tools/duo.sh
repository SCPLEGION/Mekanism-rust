#!/usr/bin/env bash
# duo.sh: builds Horizon (the SCPLEGION fork, main) with Mekanism and JEI as native mods, ready to play together.
# Replaces mek.sh for the upstream mod API (see docs/horizon-api-migration.md). Rerun any time; it only rebuilds changes.
# Not run end to end by its author: the steps are the documented ones (horizon docs/modding .../07-local-developer-builds.md).
#
#   tools/duo.sh            fetch + build + install, no launch
#   tools/duo.sh run        the same, then start the instance
#   tools/duo.sh status
#
# Env: DUO_HOME (default ~/duo), DUO_INSTANCE (default "Mekanism+JEI"), DUO_PROFILE (debug|release, default debug),
#      DUO_HORIZON_BRANCH (default main), DUO_MEK_BRANCH / DUO_JEI_BRANCH (default 26.3).
# Needs: git, cargo, python3, horizon-launcher (the official one), Minecraft 26.3 prepared in the launcher.
set -euo pipefail
HOME_DIR="${DUO_HOME:-$HOME/duo}"; INSTANCE="${DUO_INSTANCE:-Mekanism+JEI}"; PROFILE="${DUO_PROFILE:-debug}"
SRC="$HOME_DIR/src"; SDK="$HOME_DIR/sdk"
log() { printf '\033[1;32m==>\033[0m %s\n' "$*"; }
die() { printf '\033[1;31mBŁĄD:\033[0m %s\n' "$*" >&2; exit 1; }
need() { command -v "$1" >/dev/null || die "brak programu '$1'"; }
sync() { # url dir branch
  if [ -d "$2/.git" ]; then git -C "$2" fetch -q origin "$3"; else git clone -q "$1" "$2"; fi
  git -C "$2" checkout -q -f -B "$3" "origin/$3"
}
[ "${1:-build}" = status ] && { cat "$HOME_DIR/VERSION.txt" 2>/dev/null || echo "(not built)"; exit 0; }
need git; need cargo; need python3; need horizon-launcher
mkdir -p "$SRC"
log "Fetching horizon, Mekanism-rust, JustEnoughItems-rust"
sync https://github.com/SCPLEGION/horizon.git "$SRC/horizon" "${DUO_HORIZON_BRANCH:-main}"
sync https://github.com/SCPLEGION/Mekanism-rust.git "$SRC/Mekanism-rust" "${DUO_MEK_BRANCH:-26.3}"
sync https://github.com/SCPLEGION/JustEnoughItems-rust.git "$SRC/JustEnoughItems-rust" "${DUO_JEI_BRANCH:-26.3}"
REV="$(git -C "$SRC/horizon" rev-parse HEAD)"
# A native mod pins the exact game build it is compiled against: the checkout's HEAD.
for t in "$SRC/Mekanism-rust/mods/mekanism/horizon-mod.toml" "$SRC/JustEnoughItems-rust/mods/jei/horizon-mod.toml"; do
  sed -i -E "s/^horizon = \"[0-9a-f]{40}\"/horizon = \"$REV\"/" "$t"
done
FLAG=""; DIR=debug; [ "$PROFILE" = release ] && { FLAG="--release"; DIR=release; }
H="$SRC/horizon"; T="$H/target/$DIR"
log "Building the game and its companion programs ($PROFILE)"
( cd "$H"
  cargo build $FLAG -p horizon-client
  cargo build $FLAG -p horizon-client-lib --bin horizon-ctl
  cargo build $FLAG -p horizon-prepare
  cargo build $FLAG -p horizon-server
  cargo run --locked $FLAG -p cargo-horizon-mod -- wasm-std "$T/horizon-wasm-std.wasm"
  "$T/horizon-prepare" compile-exporters --root .
  mkdir -p exporters && cp -r .local/exporters/* exporters/ 2>/dev/null || true )
[ -f "$H/exporters/manifest.json" ] || die "exporters/manifest.json missing: copy the output of 'horizon-prepare compile-exporters' into $H/exporters/ (server, client, manifest.json)"
log "Installing the unsigned developer base"
( cd "$H" && git -c user.name=duo -c user.email=duo@localhost status --short | grep -q . && die "the horizon checkout has local changes" || true )
TAG="$(horizon-launcher developer-install "$H" --program "$T/horizon-client" --trust-unsigned | grep -o 'developer-[A-Za-z0-9._-]*' | tail -1)"
[ -n "$TAG" ] || die "developer-install returned no tag"
log "Base: $TAG"
if [ ! -d "$SDK" ] || [ "$(cat "$SDK/.rev" 2>/dev/null)" != "$REV" ]; then
  log "Building the SDK of this commit (heavy, once per horizon revision)"
  ( cd "$H" && python3 tools/prebuilt_game.py build "$SDK" ) && echo "$REV" > "$SDK/.rev"
fi
log "Content of Mekanism (blocks, items, recipes, models)"
( cd "$SRC/Mekanism-rust" && python3 tools/carry_content.py )
C="$SRC/Mekanism-rust/target/content"
# One folder of mods, built together into one game: the native mod "mekanism" carries the main module's
# assets and data itself (a native mod and a content mod cannot share the id), JEI is built as it is, and
# Mekanism's other three modules stay content mods.
MODS="$HOME_DIR/mods"; rm -rf "$MODS"; mkdir -p "$MODS"
cp -r "$SRC/Mekanism-rust/mods/mekanism" "$MODS/mekanism"; rm -rf "$MODS/mekanism/.horizon" "$MODS/mekanism/target"
sed -i "s#\.\./\.\./mixins/mekanism/core#$SRC/Mekanism-rust/mixins/mekanism/core#" "$MODS/mekanism/Cargo.toml"
[ -d "$C/mekanism/assets" ] && cp -r "$C/mekanism/assets" "$MODS/mekanism/assets"
[ -d "$C/mekanism/data" ] && cp -r "$C/mekanism/data" "$MODS/mekanism/data"
cp -r "$SRC/JustEnoughItems-rust/mods/jei" "$MODS/jei"; rm -rf "$MODS/jei/.horizon" "$MODS/jei/target"
for ns in generators additions tools; do cp "$C"/mekanism$ns-*.hmod "$MODS/" 2>/dev/null || true; done
if ! horizon-launcher instance list 2>/dev/null | grep -q "$INSTANCE"; then horizon-launcher instance create "$INSTANCE"; fi
horizon-launcher instance version "$INSTANCE" "$TAG"
export HORIZON_LAUNCHER_DEVELOPER_SDK="$SDK"
log "Building Mekanism and JEI together"
horizon-launcher native-build "$MODS" --version "$TAG" --instance "$INSTANCE" --trust-native-code
printf 'horizon %s (%s)\nmekanism %s\njei %s\n' "$REV" "$TAG" \
  "$(git -C "$SRC/Mekanism-rust" rev-parse --short HEAD)" "$(git -C "$SRC/JustEnoughItems-rust" rev-parse --short HEAD)" > "$HOME_DIR/VERSION.txt"
log "Ready: instance '$INSTANCE' on $TAG. Prepare the 26.3 resources for it in the launcher once, then play."
[ "${1:-}" = run ] && exec horizon-launcher play --instance "$INSTANCE" || true
