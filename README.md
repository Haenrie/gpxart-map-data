# gpxart-map-data

Builder für den **Wegegraphen** von [gpxart.com](https://gpxart.com), dem Planer für Strava-Art: Er liest
OpenStreetMap-Daten (`.osm.pbf`) und schreibt das Wegenetz als z12-Kacheln, in denen der Planer im Browser Routen
rechnet. Dazu baut er das **Ortsverzeichnis** für die Ortssuche im Planer (Städte, Orte, Dörfer und Stadtteile). Jedes
Quartal baut ein GitHub-Actions-Lauf hier ganz DACH (Deutschland, Österreich, Schweiz) und veröffentlicht Kacheln und
Ortsverzeichnis als [Release](https://github.com/Haenrie/gpxart-map-data/releases); gpxart.com liefert sie von dort aus.

Hier liegt nur der Builder. Website, Router und Suche von gpxart.com sind nicht öffentlich.

## Lizenz

- **Daten:** © [OpenStreetMap-Mitwirkende](https://www.openstreetmap.org/copyright), verfügbar unter der
  [Open Database License (ODbL) 1.0](https://opendatacommons.org/licenses/odbl/1-0/). Wegegraph und Ortsverzeichnis
  (die Dateien in den Releases) sind aus OpenStreetMap abgeleitete Datenbanken und stehen ebenfalls unter der ODbL 1.0.
- **Code:** GNU Affero General Public License, Version 3 ([LICENSE](LICENSE)), © 2026 Henry Breuer.

Beiträge von außen (Pull Requests) nehme ich derzeit nicht an.

## Inhalt

| Pfad | Inhalt |
|---|---|
| `src/` | Builder (Rust): PBF lesen (`pbf.rs`), Regeln je Fortbewegungsart (`rules.rs`), Kacheln schneiden (`tiling.rs`), Kachel-Datei schreiben (`format.rs`), Ortsverzeichnis (`orte.rs`) |
| `tests/` | `cargo test`: winzige OSM-Ausschnitte, gebaut und nach den Format-Dokumenten gelesen |
| [`docs/wegegraph-format.md`](docs/wegegraph-format.md) | Kachelformat (Version 2), der Vertrag zwischen Builder und Router; diese Fassung ist maßgeblich |
| [`docs/ortsverzeichnis-format.md`](docs/ortsverzeichnis-format.md) | Format des Ortsverzeichnisses (Version 1) und welche Orte hineinkommen; maßgeblich |
| [`.github/workflows/dach.yml`](.github/workflows/dach.yml) | Vierteljährlicher Bau von ganz DACH als Release |
| [`.github/workflows/tests.yml`](.github/workflows/tests.yml) | `cargo test` bei jedem Push und Pull Request |

## Bauen

Rust (stable), dann:

```bash
cargo test
cargo build --release

# ganz DACH: alle Kacheln mit Wegen nach graph/<Version>/12/<x>/<y>.bin
curl -LO https://download.geofabrik.de/europe/dach-latest.osm.pbf
target/release/wegegraph graph/2026-10-18 dach-latest.osm.pbf

# nur ein Rechteck aus z12-Kacheln (x0–x1, y0–y1), mehrere Eingabedateien werden zusammengeführt
target/release/wegegraph graph/test 2169-2173 1421-1424 oberbayern-latest.osm.pbf schwaben-latest.osm.pbf

# Ortsverzeichnis von ganz DACH nach orte/<Version>/orte/ (index.json und ein Teil je Anfangsbuchstaben)
target/release/wegegraph orte orte/2026-10-18/orte dach-latest.osm.pbf

# nur die Orte in einem oder mehreren Rechtecken aus z12-Kacheln (Testdaten)
target/release/wegegraph orte orte/test 2169-2173 1421-1424 2200-2201 1342-1343 oberbayern.osm.pbf berlin.osm.pbf
```

Unter Windows mit Smart App Control kann `cargo` mit „Eine Anwendungssteuerungsrichtlinie hat diese Datei blockiert“
(os error 4551) abbrechen: Es blockiert manche frisch gebauten, unsignierten Build-Skripte. Dann die Build-Skripte mit
anderer Optimierung bauen: `CARGO_PROFILE_DEV_BUILD_OVERRIDE_OPT_LEVEL=1 cargo test` (für `--release` entsprechend
`CARGO_PROFILE_RELEASE_BUILD_OVERRIDE_OPT_LEVEL`), notfalls mit `2`.

Mit denselben Eingabedateien entstehen auf demselben Betriebssystem byte-gleiche Kacheln. Zwischen Linux und Windows
weichen einzelne Kacheln geringfügig ab, weil die Mathematik-Bibliotheken (`tan`, `asinh` …) in der letzten Stelle
anders runden; bei DACH 616 von 14 255 Kacheln, meist um einen Knoten. Eine Kachel hängt nur von den Wegen ab, die sie
berühren; der Builder baut große Gebiete darum in Streifen aus 8 Kachelzeilen, mit denselben Kacheln wie in einem Stück.

## Vierteljährlicher Lauf und Releases

[`dach.yml`](.github/workflows/dach.yml) läuft am 1. Januar, April, Juli und Oktober um 02:17 UTC und lässt sich unter Actions → „DACH
bauen“ → „Run workflow“ von Hand starten. Er lädt `dach-latest.osm.pbf` von Geofabrik (einmal im Quartal, mit
Prüfsumme), führt die Tests aus, baut alle Kacheln und das Ortsverzeichnis, prüft die Grenzen der Auslieferung und legt
ein Release an. Die drei neuesten Releases bleiben, ältere werden gelöscht.

**Version** ist das Datum der OSM-Daten plus der Commit des Builders, z. B. `2026-10-18-cf1e32f`. Gibt es das Release
schon, baut der Lauf nichts. Kacheln und Ortsverzeichnis einer Version ändern sich nie; ein Server darf sie dauerhaft
cachen.

**Ein Release enthält:**

| Datei | Inhalt |
|---|---|
| `wegegraph.tar.gz` | `aktuell.json` (`{"version": "<Version>"}`), `LIZENZ.txt` (ODbL-Vermerk, Quelle) und `<Version>/12/<x>/<y>.bin`, also genau den Ordner `graph/`, den ein Server ausliefert |
| `ortsverzeichnis.tar.gz` | `<Version>/orte/index.json` und `<Version>/orte/<teil>.json`, entpackt neben `wegegraph.tar.gz` in denselben Ordner (seit gpxart.com #39; ältere Releases haben es nicht) |
| `wegegraph.json` | Version, Quelle, Commit, Kachelzahl, Größe, größte Kachel, Orte, Speicher und Dauer des Baus |
| `SHA256SUMS` | Prüfsummen der Dateien |

gpxart.com holt jedes Quartal das neueste Release ab und veröffentlicht es auf Cloudflare Workers (Static Assets); dieses
Repo braucht dafür keine Zugangsdaten.

## Größen und Grenzen (DACH, gemessen)

Geofabrik `dach-261018.osm.pbf` (6,25 GB, OSM-Daten vom 18.10.2026):

| | gemessen | Grenze |
|---|---|---|
| Kacheln (Dateien) | 14 255, mit Ortsverzeichnis und Zeiger 14 583 | 20 000 Dateien je Worker (Cloudflare Workers Static Assets, Free) |
| Größte Kachel | 1,38 MB (`12/2200/1343`, Berlin-Mitte) | 25 MiB je Datei |
| Alle Kacheln | 1,23 GB, gepackt 658 MB | 2 GiB je Release-Datei (GitHub) |
| Wege, Knoten im PBF | 20,8 Mio. Wege, 125 Mio. Knoten | |
| Graph | 76,8 Mio. Knoten, 84,2 Mio. Kanten | |
| Bau auf GitHub Actions ([erstes Release](https://github.com/Haenrie/gpxart-map-data/releases/tag/2026-10-18-cf1e32f)) | Builder 7 min 13 s (davon Lesen 5 min 8 s), höchstens 6,4 GiB (Max RSS); ganzer Lauf 20 min, davon Download 10 min | 16 GB RAM (15 GiB nutzbar), Platte (86 GB frei, gebraucht gut 8 GB), 6 h je Job |
| Bau auf einem Windows-PC | 4½–5 min (davon Lesen 3½ min), höchstens 6,0 GiB (Working Set) | |

Den Speicher bestimmt vor allem das Lesen (5,6 GiB): Knoten-IDs, die `osmpbf` beim Lesen sammelt, dazu alle Knoten (16
Byte) und Wege. Das Schneiden in Streifen braucht danach knapp 1 GiB mehr.

Zu den Kacheln gehören auch Fährlinien aus DACH heraus (z. B. nach Skandinavien); deshalb reicht der gebaute Bereich
über DACH hinaus.

**Ortsverzeichnis** (gleiche Daten, Windows-PC): 126 417 Orte (101 Großstädte, 2 621 Städte, 7 410 Stadtteile,
1 886 Viertel, 46 410 Dörfer, 67 988 Weiler) in 52 Ländern und Kantonen; 324 Teile und `index.json`, zusammen 8,5 MB,
gepackt (gzip) 2,8 MB. Ein Abruf beim Tippen ist meist wenige kB groß, der größte Teil (`sc.json`) 319 kB, gepackt
96 kB. Bau gut 2 min (fast nur Lesen), höchstens 0,5 GiB Speicher. Im Genfersee fehlen dem Auszug fünf Wege der
Grenze von Schweiz und Waadt; der Builder schließt die Lücke (10,7 km) gerade.
