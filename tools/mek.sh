#!/usr/bin/env bash
# mek.sh: pobiera najnowsze gałęzie Mekanism-rust i Horizona, buduje je i przygotowuje uruchomienie gry.
# Można uruchamiać wielokrotnie (aktualizuje i dobudowuje tylko zmiany).
#
#   ./mek.sh            pobierz + zbuduj + przygotuj (bez uruchamiania)
#   ./mek.sh run        jak wyżej, potem uruchom grę
#   ./mek.sh play       tylko uruchom to, co już zbudowane
#   ./mek.sh status     pokaż wybrane gałęzie i stan folderów
#   ./mek.sh clean      usuń zbudowane wyniki (zostają klony i światy w game/)
#
# Opcje (zmienne środowiskowe):
#   MEK_HOME=~/mek            gdzie wszystko leży
#   MEK_BRANCH=...            wymuś gałąź Mekanism-rust (domyślnie: ostatnio zmieniona milestone/*)
#   MEK_HORIZON_BRANCH=...    wymuś gałąź Horizona (domyślnie: ta, która zawiera rewizję z compatibility.json)
#   MEK_HORIZON_HEAD=1        buduj z czubka gałęzi Horizona zamiast z przypiętej rewizji (ryzyko niezgodności)
#   MEK_RESOURCES=...         ścieżka do horizon-client/26.3 (domyślnie wykrywana)
#   MEK_SERVER_JAR=...        server.jar 26.3 (do przygotowania zasobów; domyślnie ~/Downloads/server.jar)
#   MEK_WORLD=MekTest         nazwa świata testowego
#   MEK_TEST=1                uruchom też `cargo test` w Mekanism-rust
set -euo pipefail

MEK_HOME="${MEK_HOME:-$HOME/mek}"
MEK_WORLD="${MEK_WORLD:-MekTest}"
MEK_REPO="${MEK_REPO:-https://github.com/SCPLEGION/Mekanism-rust.git}"
HZ_REPO="${HZ_REPO:-https://github.com/SCPLEGION/horizon.git}"
SRC="$MEK_HOME/src"            # klony git (źródła)
CACHE="$MEK_HOME/.cache"       # drzewo budowania (kompilacja przyrostowa)
OUT_HZ="$MEK_HOME/horizon"     # gotowa gra: horizon-client + horizon-ctl
OUT_MEK="$MEK_HOME/mekanism"   # gotowe mody .hmod
GAME="$MEK_HOME/game"          # folder gry (światy, opcje) – osobny od Twojego prawdziwego Minecrafta

log() { printf '\033[1;32m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m!!\033[0m %s\n' "$*" >&2; }
die() { printf '\033[1;31mBŁĄD:\033[0m %s\n' "$*" >&2; exit 1; }
need() { command -v "$1" >/dev/null 2>&1 || die "brak programu '$1' ($2)"; }

check_tools() {
  need git "zainstaluj git"; need cargo "zainstaluj Rust: https://rustup.rs"; need python3 "potrzebny Python 3.10+"
  python3 -c 'import sys; sys.exit(sys.version_info < (3,10))' || die "Python 3.10 lub nowszy"
}

# klonuje lub aktualizuje repo, pobiera wszystkie gałęzie
sync_repo() { # url dir
  if [ -d "$2/.git" ]; then git -C "$2" fetch --prune --tags -q origin '+refs/heads/*:refs/remotes/origin/*'
  else git clone -q --no-checkout "$1" "$2"; git -C "$2" fetch -q --tags origin '+refs/heads/*:refs/remotes/origin/*'; fi
}

newest_branch() { # dir pattern -> najświeższa gałąź (po dacie commitu)
  git -C "$1" for-each-ref --sort=-committerdate --format='%(refname:short)' "refs/remotes/origin/$2" \
    | sed 's#^origin/##' | head -1
}

fetch_sources() {
  mkdir -p "$SRC"
  log "Pobieram Mekanism-rust i Horizon"
  sync_repo "$MEK_REPO" "$SRC/Mekanism-rust"
  sync_repo "$HZ_REPO" "$SRC/horizon"

  MEK_BR="${MEK_BRANCH:-$(newest_branch "$SRC/Mekanism-rust" 'milestone/*')}"
  [ -n "$MEK_BR" ] || die "nie znalazłem gałęzi milestone/* w Mekanism-rust (ustaw MEK_BRANCH)"
  git -C "$SRC/Mekanism-rust" checkout -q -f -B "$MEK_BR" "origin/$MEK_BR"
  MEK_SHA="$(git -C "$SRC/Mekanism-rust" rev-parse --short HEAD)"

  PIN="$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["horizon_revision"])' "$SRC/Mekanism-rust/compatibility.json")"
  git -C "$SRC/horizon" cat-file -e "$PIN^{commit}" 2>/dev/null \
    || die "rewizja Horizona $PIN z compatibility.json nie istnieje w $HZ_REPO (jest wypchnięta?)"
  if [ -n "${MEK_HORIZON_BRANCH:-}" ]; then HZ_BR="$MEK_HORIZON_BRANCH"
  else
    HZ_BR="$(git -C "$SRC/horizon" branch -r --contains "$PIN" --sort=-committerdate --format='%(refname:short)' \
      | grep -v 'HEAD' | sed 's#^origin/##' | grep -v '^claude/' | head -1 || true)"
    [ -n "$HZ_BR" ] || HZ_BR="$(newest_branch "$SRC/horizon" 'h*')"
  fi
  if [ "${MEK_HORIZON_HEAD:-0}" = 1 ]; then HZ_REV="$(git -C "$SRC/horizon" rev-parse "origin/$HZ_BR")"; else HZ_REV="$PIN"; fi
  git -C "$SRC/horizon" checkout -q -f -B "$HZ_BR" "origin/$HZ_BR" 2>/dev/null || true
  log "Mekanism-rust: $MEK_BR @ $MEK_SHA"
  log "Horizon:       $HZ_BR (buduję z ${HZ_REV:0:10}$([ "$HZ_REV" = "$PIN" ] && echo ', przypięta rewizja'))"
}

# --- zasoby Horizona (pliki Twojego Minecrafta 26.3) ---
find_resources() {
  [ -n "${MEK_RESOURCES:-}" ] && { RES="$MEK_RESOURCES"; return; }
  local d
  for d in "$SRC/horizon/.local/horizon-client/26.3" \
           "$HOME"/.local/share/horizon-launcher/versions/*/.local/horizon-client/26.3; do
    [ -d "$d" ] && { RES="$d"; return; }
  done
  RES=""
}

prepare_resources() {
  find_resources
  [ -n "$RES" ] && { log "Zasoby: $RES"; return; }
  local jar="${MEK_SERVER_JAR:-$HOME/Downloads/server.jar}" mc=""
  [ -f "$jar" ] || die "brak zasobów Horizona i brak $jar. Pobierz server.jar 26.3 z minecraft.net/download/server (albo ustaw MEK_SERVER_JAR), albo przygotuj wersję 26.3 w launcherze Horizona, albo ustaw MEK_RESOURCES."
  for d in "$HOME/.local/share/horizon-launcher/minecraft" "$HOME/.minecraft"; do
    [ -f "$d/versions/26.3/26.3.json" ] && { mc="$d"; break; }
  done
  [ -n "$mc" ] || die "nie znalazłem Minecrafta 26.3 (uruchom 26.3 raz w oficjalnym launcherze albo przygotuj go w launcherze Horizona)"
  log "Przygotowuję zasoby Horizona z $mc (jednorazowo, trwa)"
  (cd "$SRC/horizon" && cargo run --release -p horizon-prepare -- --prepare --server-jar "$jar" --minecraft-dir "$mc")
  find_resources; [ -n "$RES" ] || die "horizon-prepare nie utworzył zasobów"
}

# --- gra z wbudowanym Mekanismem (natywna część) ---
build_game() {
  local stage="$CACHE/stage" mark="$CACHE/stage.rev"
  mkdir -p "$CACHE"
  if [ ! -d "$stage" ] || [ "$(cat "$mark" 2>/dev/null)" != "$HZ_REV" ]; then
    log "Wypakowuję źródła Horizona ${HZ_REV:0:10} do cache"
    # nowa rewizja: nadpisujemy źródła, ale zostawiamy target/ (kompilacja przyrostowa)
    mkdir -p "$stage"
    find "$stage" -mindepth 1 -maxdepth 1 ! -name target -exec rm -rf {} +
    git -C "$SRC/horizon" archive "$HZ_REV" | tar -x -C "$stage"
    echo "$HZ_REV" > "$mark"
  fi
  log "Nakładam paczkę Mekanism (mixins/mekanism)"
  python3 "$SRC/Mekanism-rust/tools/manual_stage.py" "$stage" "$SRC/Mekanism-rust/mixins/mekanism"
  # mtime z archiwum potrafi oszukać cargo – dotknij nałożone pliki
  touch "$stage/crates/horizon-client/src/game.rs" "$stage/crates/horizon-client/src/game/native_hooks.rs"
  find "$stage/native-crates/mekanism" -name '*.rs' -exec touch {} +
  log "Buduję horizon-client (profil fast)"
  (cd "$stage" && cargo build --profile fast -p horizon-client --bin horizon-client --bin horizon-ctl)
  mkdir -p "$OUT_HZ"
  cp -f "$stage/target/fast/horizon-client" "$OUT_HZ/horizon-client"
  [ -f "$stage/target/fast/horizon-ctl" ] && cp -f "$stage/target/fast/horizon-ctl" "$OUT_HZ/horizon-ctl"
  printf 'horizon: %s (%s)\nmekanism: %s @ %s\n' "$HZ_BR" "$HZ_REV" "$MEK_BR" "$MEK_SHA" > "$OUT_HZ/VERSION.txt"
}

# --- mody z treścią ---
build_content() {
  [ "${MEK_TEST:-0}" = 1 ] && { log "cargo test"; (cd "$SRC/Mekanism-rust" && cargo test); }
  log "Buduję mody z treścią (.hmod)"
  (cd "$SRC/Mekanism-rust" && python3 tools/carry_content.py)
  rm -rf "$OUT_MEK"; mkdir -p "$OUT_MEK"
  cp -f "$SRC"/Mekanism-rust/target/content/*.hmod "$OUT_MEK/"
  ls "$OUT_MEK"/*.hmod >/dev/null 2>&1 || die "carry_content.py nie wyprodukował żadnego .hmod"
}

# --- folder gry + skrypt startowy ---
prepare_game() {
  mkdir -p "$GAME"
  {
    echo '#!/usr/bin/env bash'
    echo "# Wygenerowane przez mek.sh. Uruchamia grę z Mekanismem na osobnym folderze $GAME."
    echo 'set -e'
    echo "cd '$GAME'"
    echo "MODS=()"
    echo "for f in '$OUT_MEK'/*.hmod; do MODS+=(--dev-mod \"\$f\"); done"
    echo "exec '$OUT_HZ/horizon-client' --resources '$RES' --game-dir '$GAME' --world '$MEK_WORLD' --creative \"\${MODS[@]}\" \"\$@\""
  } > "$MEK_HOME/start.sh"
  chmod +x "$MEK_HOME/start.sh"
}

do_play() { [ -x "$MEK_HOME/start.sh" ] || die "najpierw uruchom ./mek.sh"; log "Uruchamiam grę (świat $MEK_WORLD)"; exec "$MEK_HOME/start.sh"; }

do_status() {
  echo "MEK_HOME=$MEK_HOME"
  cat "$OUT_HZ/VERSION.txt" 2>/dev/null || echo "(jeszcze nie zbudowano)"
  ls "$OUT_MEK" 2>/dev/null || true
}

cmd="${1:-build}"
case "$cmd" in
  status) do_status ;;
  play) do_play ;;
  clean) log "Usuwam $OUT_HZ $OUT_MEK $CACHE (klony i $GAME zostają)"; rm -rf "$OUT_HZ" "$OUT_MEK" "$CACHE" "$MEK_HOME/start.sh" ;;
  build|run)
    check_tools; mkdir -p "$MEK_HOME"
    fetch_sources; prepare_resources; build_game; build_content; prepare_game
    log "Gotowe. Start: $MEK_HOME/start.sh   (lub: ./mek.sh play)"
    [ "$cmd" = run ] && do_play || true ;;
  *) die "nieznana komenda '$cmd' (build|run|play|status|clean)" ;;
esac
