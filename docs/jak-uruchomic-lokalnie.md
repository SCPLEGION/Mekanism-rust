# Mekanism na Horizon: jak uruchomić u siebie

Wszystko poniżej zakłada Linuksa (Horizon jest rozwijany i testowany na Linuksie).

## 0. Najkrócej: skrypt mek.sh

`tools/mek.sh` robi kroki 2–6 za Ciebie: pobiera najnowsze gałęzie (Mekanism-rust: ostatnio zmieniona `milestone/*`;
Horizon: rewizja z `compatibility.json`), buduje grę i mody, i układa wyniki w osobnych folderach:

```sh
cd ~/mek && curl -O https://raw.githubusercontent.com/SCPLEGION/Mekanism-rust/<gałąź z PR>/tools/mek.sh && chmod +x mek.sh
./mek.sh run        # pobierz, zbuduj, uruchom   (./mek.sh = bez uruchamiania, ./mek.sh play = tylko start)
```

Wyniki: `~/mek/horizon/` (gra), `~/mek/mekanism/` (cztery `.hmod`), `~/mek/game/` (folder gry, światy), `~/mek/start.sh`.
Ponowne uruchomienie aktualizuje i dobudowuje tylko zmiany. Zmienne: `MEK_BRANCH`, `MEK_HORIZON_BRANCH`, `MEK_RESOURCES`,
`MEK_SERVER_JAR`, `MEK_WORLD`, `MEK_TEST=1` (opis w nagłówku skryptu). Zasoby Minecrafta (krok 3) skrypt wykrywa sam; gdy ich
nie ma, potrzebuje `server.jar`. Poniżej to samo ręcznie.

## 1. Co jest potrzebne

- Rust 1.90 lub nowszy (`rustup`), git, Python 3.10 lub nowszy, JDK 25.
- Na Debianie/Ubuntu: `sudo apt install libasound2-dev libudev-dev`.
- Twoja własna kopia Minecraft: Java Edition 26.3. Odpal 26.3 raz w oficjalnym launcherze, żeby powstało `~/.minecraft/versions/26.3/`.
- Jar serwera 26.3 (`server.jar`), np. z minecraft.net/download/server, zapisany w `~/Downloads/server.jar`.

## 2. Pobierz oba repozytoria obok siebie

Gałęzie to wierzchołki stosów (każdy kamień milowy zawiera wszystkie poprzednie):

```sh
mkdir ~/mek && cd ~/mek
git clone -b milestone/h17-slot-rules https://github.com/SCPLEGION/horizon.git
git clone -b milestone/g26-nucleosynthesizer https://github.com/SCPLEGION/Mekanism-rust.git
```

(Gdy pojawi się nowsza gałąź `milestone/g7-...`, weź tę najwyższą.)

## 3. Przygotuj zasoby Horizona (raz)

Horizon nie pobiera gry sam: potrzebuje plików Twojego Minecrafta 26.3. Sprawdź najpierw, co już masz:

```sh
ls -d ~/.local/share/horizon-launcher/versions/*/.local/horizon-client/26.3      # A: launcher Horizona już przygotował zasoby
ls ~/.local/share/horizon-launcher/minecraft/versions/26.3/26.3.json               # B: launcher Horizona pobrał 26.3
ls ~/.minecraft/versions/26.3/26.3.json                                            # C: oficjalny launcher
```

- **A**: nic nie przygotowuj. W kroku 6 podaj tę ścieżkę jako `--resources`.
- **B**: przygotuj z folderu launchera Horizona:
  ```sh
  cd ~/mek/horizon
  cargo run --release -p horizon-prepare -- --prepare --server-jar ~/Downloads/server.jar \
    --minecraft-dir ~/.local/share/horizon-launcher/minecraft
  ```
- **C**: `cargo run --release -p horizon-prepare -- --prepare --server-jar ~/Downloads/server.jar`
- Nic z tego: otwórz launcher Horizona i kliknij przygotowanie wersji 26.3 (znajduje też pliki Prism,
  MultiMC, Modrinth App), albo uruchom raz 26.3 w swoim launcherze; potem wróć do A, B lub C.

Dźwięki (opcjonalnie): `cargo run --release -p horizon-prepare -- sounds` (z tym samym `--minecraft-dir`).
Zasoby z B i C lądują w `horizon/.local/`. Nigdy ich nie commituj ani nie udostępniaj.

## 4. Zbuduj grę z wbudowanym Mekanismem

Część natywna (maszyny, energia, rury) jest kompilowana do gry. Najprostsza droga, bez horizon-mixins:

```sh
cd ~/mek/Mekanism-rust
cargo test                                   # szybki test logiki
STAGE=~/mek/stage && rm -rf "$STAGE" && mkdir "$STAGE"
git -C ../horizon archive "$(python3 -c 'import json;print(json.load(open("compatibility.json"))["horizon_revision"])')" | tar -x -C "$STAGE"
python3 tools/manual_stage.py "$STAGE" mixins/mekanism
(cd "$STAGE" && cargo build --profile fast -p horizon-client --bin horizon-client)
```

Gotowa gra: `~/mek/stage/target/fast/horizon-client`.

## 5. Zbuduj mody z treścią (bloki, przedmioty, receptury, rudy)

```sh
cd ~/mek/Mekanism-rust
python3 tools/carry_content.py
ls target/content/      # mekanism, mekanismgenerators, mekanismadditions, mekanismtools (.hmod)
```

## 6. Uruchom

Na osobnym, testowym folderze gry (nie na swoich prawdziwych światach):

```sh
mkdir -p ~/mek/gamedir && cd ~/mek/gamedir
C=~/mek/Mekanism-rust/target/content
~/mek/stage/target/fast/horizon-client \
  --resources ~/mek/horizon/.local/horizon-client/26.3 \
  --game-dir ~/mek/gamedir --world MekTest --creative \
  --dev-mod $C/mekanism-0.1.0.hmod --dev-mod $C/mekanismgenerators-0.1.0.hmod \
  --dev-mod $C/mekanismadditions-0.1.0.hmod --dev-mod $C/mekanismtools-0.1.0.hmod
```

W przypadku A z kroku 3 zamień ścieżkę `--resources` na tę, którą wypisała komenda A.

`--dev-mod` włącza mod na stałe. Alternatywnie wrzuć cztery pliki `.hmod` do `~/mek/gamedir/horizon-mods/` i włącz je w Options > Mods.

## 7. Co sprawdzić najpierw: podwajanie rud

1. Postaw Enrichment Chamber.
2. Obok niej (stykając się) postaw Heat Generator i włóż do niego węgiel. Może być też naładowana Energy Cube przodem do maszyny.
3. Do wejścia komory włóż rudę osmu. Po ok. 10 s na wyjściu ma być 2 pyły osmu.

Potem warto sprawdzić:
- Crusher, Energized Smelter i inne maszyny.
- Kable między generatorem a maszyną: powinny się łączyć w sześciu kierunkach.
- Ładowanie Energy Tablet w Energy Cube.
- Atomic Disassembler: shift + PPM w powietrzu zmienia tryb.
- Configurator: shift + PPM w powietrzu zmienia typ; PPM na boku maszyny ustawia stronę.

## 8. Co mi odesłać

Nic z tego nie było jeszcze odpalone w prawdziwej grze, bo tu nie mam plików Minecrafta. Kod przechodzi testy i kompiluje się z Horizonem. Jeśli coś nie działa, wklej:
- błąd z kompilacji albo ostatnie linie logu gry;
- co zrobiłeś i co się stało zamiast tego (zrzut ekranu też pomaga).
