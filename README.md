# gpxart-map-data

Builder für den **Wegegraphen** von [gpxart.com](https://gpxart.com), dem Planer für Strava-Art: Er liest
OpenStreetMap-Daten (`.osm.pbf`) und schreibt das Wegenetz als z12-Kacheln, in denen der Planer im Browser Routen
rechnet. Dazu baut er das **Ortsverzeichnis** für die Ortssuche im Planer (Städte, Orte, Dörfer und Stadtteile). Jedes
Quartal baut ein GitHub-Actions-Lauf hier alle [Regionen](#regionen) (DACH, Frankreich, Monaco, Andorra), jede aus
ihrem eigenen Geofabrik-Auszug, und veröffentlicht Kacheln und Ortsverzeichnis als
[Release](https://github.com/Haenrie/gpxart-map-data/releases); gpxart.com liefert sie von dort aus.

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
| [`regionen.txt`](regionen.txt) | Die Regionen: Name, Geofabrik-Auszug, Staaten fürs Ortsverzeichnis |
| `src/` | Builder (Rust): PBF lesen (`pbf.rs`), Regeln je Fortbewegungsart (`rules.rs`), Kacheln schneiden (`tiling.rs`), Kachel-Datei schreiben (`format.rs`), Ortsverzeichnis (`orte.rs`), Regionen und ihre Umrisse (`regionen.rs`, `flaeche.rs`), Randwege zwischen Regionen (`randwege.rs`) |
| `tests/` | `cargo test`: winzige OSM-Ausschnitte, gebaut und nach den Format-Dokumenten gelesen |
| [`docs/wegegraph-format.md`](docs/wegegraph-format.md) | Kachelformat (Version 2), der Vertrag zwischen Builder und Router; diese Fassung ist maßgeblich |
| [`docs/ortsverzeichnis-format.md`](docs/ortsverzeichnis-format.md) | Format des Ortsverzeichnisses (Version 1) und welche Orte hineinkommen; maßgeblich |
| [`.github/workflows/bauen.yml`](.github/workflows/bauen.yml) | Vierteljährlicher Bau aller Regionen als Release |
| [`.github/workflows/tests.yml`](.github/workflows/tests.yml) | `cargo test` bei jedem Push und Pull Request |

## Bauen

Rust (stable), dann:

```bash
cargo test
cargo build --release

# in einem Stück, z. B. ganz DACH: alle Kacheln mit Wegen nach graph/<Version>/12/<x>/<y>.bin
curl -LO https://download.geofabrik.de/europe/dach-latest.osm.pbf
target/release/wegegraph graph/2026-10-18 dach-latest.osm.pbf

# nur ein Rechteck aus z12-Kacheln (x0–x1, y0–y1), mehrere Eingabedateien werden zusammengeführt
target/release/wegegraph graph/test 2169-2173 1421-1424 oberbayern-latest.osm.pbf schwaben-latest.osm.pbf

# Ortsverzeichnis von ganz DACH nach orte/<Version>/orte/ (index.json und ein Teil je Anfangsbuchstaben)
target/release/wegegraph orte orte/2026-10-18/orte dach-latest.osm.pbf

# nur die Orte in einem oder mehreren Rechtecken aus z12-Kacheln (Testdaten)
target/release/wegegraph orte orte/test 2169-2173 1421-1424 2200-2201 1342-1343 oberbayern.osm.pbf berlin.osm.pbf

# je Region (wie der vierteljährliche Lauf, siehe „Regionen“); Umrisse von Geofabrik nach umrisse/<Auszug>.poly
curl --create-dirs -o umrisse/europe/dach.poly https://download.geofabrik.de/europe/dach.poly  # ebenso france, monaco, andorra
target/release/wegegraph regionen regionen.txt umrisse                               # prüft Liste und Umrisse
target/release/wegegraph randwege regionen.txt umrisse dach rand dach.osm.pbf        # schreibt rand/dach-france.rand
target/release/wegegraph randwege regionen.txt umrisse france rand france.osm.pbf monaco.osm.pbf andorra.osm.pbf
target/release/wegegraph region regionen.txt umrisse dach graph/2026-10-18 dach.osm.pbf rand/*-dach.rand
target/release/wegegraph region regionen.txt umrisse france graph/2026-10-18 france.osm.pbf monaco.osm.pbf andorra.osm.pbf rand/*-france.rand
target/release/wegegraph ortsliste regionen.txt dach orte-dach.tsv dach.osm.pbf
target/release/wegegraph ortsliste regionen.txt france orte-france.tsv france.osm.pbf monaco.osm.pbf andorra.osm.pbf
target/release/wegegraph ortsverzeichnis orte/2026-10-18/orte orte-dach.tsv orte-france.tsv
```

Unter Windows mit Smart App Control kann `cargo` mit „Eine Anwendungssteuerungsrichtlinie hat diese Datei blockiert“
(os error 4551) abbrechen: Es blockiert manche frisch gebauten, unsignierten Build-Skripte. Dann die Build-Skripte mit
anderer Optimierung bauen: `CARGO_PROFILE_DEV_BUILD_OVERRIDE_OPT_LEVEL=1 cargo test` (für `--release` entsprechend
`CARGO_PROFILE_RELEASE_BUILD_OVERRIDE_OPT_LEVEL`), notfalls mit `2`.

Mit denselben Eingabedateien entstehen auf demselben Betriebssystem byte-gleiche Kacheln. Zwischen Linux und Windows
weichen einzelne Kacheln geringfügig ab, weil die Mathematik-Bibliotheken (`tan`, `asinh` …) in der letzten Stelle
anders runden; bei DACH 616 von 14 255 Kacheln, meist um einen Knoten. Eine Kachel hängt nur von den Wegen ab, die
darin Stücke haben; der Builder baut große Gebiete darum in Streifen aus 8 Kachelzeilen, mit denselben Kacheln wie in
einem Stück, und die Welt in Regionen (nächster Abschnitt).

## Regionen

Ein Rechner hat zu wenig Speicher und Platte für Europa oder die Welt in einem Stück (der Builder hält einen Auszug
ganz im Speicher, für DACH 6,4 GiB; der öffentliche Runner hat 16 GB RAM und ≈ 14 GB Platte, freiräumbar auf ≈ 31 GB).
Darum baut jeder Lauf **jede Region für sich** aus ihren eigenen Geofabrik-Auszügen, als eigener Job
(gpxart.com ADR 0003). [`regionen.txt`](regionen.txt) listet sie:

```
dach     europe/dach                                   DE AT CH LI
france   europe/france,europe/monaco,europe/andorra    FR MC AD
```

Je Zeile Name (für Dateien und Release), die Pfade der Auszüge bei Geofabrik (mit Komma getrennt) und die Staaten
(ISO 3166-1), deren Orte die Region ins Ortsverzeichnis bringt. **Für eine weitere Region genügt eine Zeile**; Umrisse
und Auszüge holt der Lauf selbst. Eine Region ist ein Land, ein Sammelauszug von Geofabrik (`dach`) oder eine Gruppe
kleiner Nachbarländer (z. B. das Baltikum), so groß, dass ihre Auszüge zusammen in den Speicher passen (bis etwa
10 GB PBF; große Länder in den Teilauszügen, die Geofabrik anbietet). Kleine Nachbarn zusammen sparen Jobs und
Randwege. Der Umriss einer Region ist die Vereinigung der Umrisse ihrer Auszüge.

Regionen überlappen sich **nur am Rand**. `wegegraph regionen` (im Lauf vor allem anderen) bricht ab, wenn eine Region
zu mehr als der Hälfte in anderen liegt: Ihre Wege würden doppelt gelesen. Das wäre nicht falsch (doppelte Wege zählen
einmal), aber doppelte Arbeit. Gemessen an allen Ländern Europas (Oktober 2026): Länder höchstens 7 %, Kleinstaaten
14–26 % (Andorra, Liechtenstein, Guernsey und Jersey); über der Grenze Isle of Man 62 %, Azoren 97 % (im Umriss von
Portugal), Monaco 100 % (im Umriss von Frankreich) und `alps` neben `dach` 100 %. Solche Auszüge gehören in die Zeile
ihres Nachbarn.

**Wie Geofabrik an Grenzen schneidet** (geprüft an `alsace`, `freiburg-regbez`, `dach` und `france`, Oktober 2026): Ein
Auszug enthält jeden Weg, der mindestens einen Knoten im Umriss (`<Auszug>.poly`) hat, mit allen Knoten (kein Weg hat
fehlende Knoten); solche Wege reichen bis 18 km über den Umriss hinaus. Dazu kommen vereinzelt Wege ganz außerhalb
(Teile von Multipolygonen). Der Umriss selbst folgt der Landesgrenze mit einem Saum: Die Umrisse von `dach` und
`france` überlappen sich im Median um 0,8–1 km, selten bis 6 km. Eine z12-Kachel ist 6–7 km breit, an einer Grenze
enthält also kein Auszug alle Wege einer Kachel.

**Grenzverfahren.** Jede z12-Kachel gehört genau einer Region: der ersten der Liste, deren Umriss die Mitte der Kachel
enthält, sonst (Meer, Fährlinien) der mit dem nächsten Umriss. Nur sie baut die Kachel. Der Lauf hat zwei Schritte je
Region:

1. **Randwege** (`wegegraph randwege`): Die Region schreibt für jede andere Region die Wege ihrer Auszüge, die eine
   Kachel der anderen berühren, mit Stellen und allen Knoten (`<von>-<an>.rand`, Format in `src/randwege.rs`, nur
   innerhalb eines Laufs). Dazu die Ortsliste.
2. **Kacheln** (`wegegraph region`): Die Region liest ihre Auszüge und die Randwege der anderen für sie und baut ihre
   Kacheln. Was doppelt vorkommt (ein Weg über die Grenze steht in beiden Auszügen), zählt einmal.

So baut jede Region ihre Kacheln aus **allen** Wegen, die sie berühren, und die Kacheln sind **byte-gleich mit einem
Bau aller Auszüge in einem Stück**; keine Kante fehlt, keine steht doppelt, eine Route geht über die Grenze, als gäbe es
sie nicht. Das hängt weder davon ab, wie weit ein Auszug über die Grenze reicht, noch wie die Umrisse genau verlaufen
(sie verteilen nur die Kacheln); auch eine Fähre, deren einziges Stück eine fremde Kachel ohne Knoten darin kreuzt,
kommt hinein. Geprüft: `cargo test` (`tests/regionen.rs`); die Testkacheln Kehl/Straßburg von gpxart.com
(`freiburg-regbez` und `alsace`, beide vom 28.10.2026); ganz DACH und `alsace` (als Frankreich) entlang der ganzen
Grenze im Elsass, 530 Kacheln gleich bis auf drei, in denen sich Wege zwischen den Datenständen der beiden Auszüge
(18. und 28.10.) geändert hatten (in einem Stück gewinnt der zuerst gelesene Auszug; liest man ihn andersherum,
stimmen die drei, und zwei andere weichen ab). Der Lauf nimmt darum Auszüge desselben Tages. Damit das gilt, liegt
jede Kante in der Kachel ihrer Kette, auch wenn sie nach Douglas-Peucker genau auf der Kachelgrenze liegt (seit
gpxart.com #85; vorher hing so eine Kante in seltenen Fällen von Wegen der Nachbarkachel ab).

Die Randwege sind klein: `dach` gibt `france` 5,7 MB, `france` gibt `dach` 15,1 MB (Grenze am Rhein, im Jura und am
Genfersee), Monaco und Andorra geben Frankreich je ½ MB. Speicher und Platte je Job hängen nur von der eigenen Region
ab; der Schritt Randwege kostet je Region ein zweites Lesen ihres Auszugs (DACH gut 5 min).

Monaco und Andorra sind eigene Auszüge, nicht Teil von `france`: Monaco liegt zwar im Umriss von Frankreich, Andorra
nur mit seinem Rand. Beide stehen in der Zeile von `france` (bis gpxart.com #85 waren sie eigene Regionen; Monaco
besaß keine Kachel und gab nur Randwege).

Ein **Release ist immer ganz**: alle Regionen der Liste, Auszüge desselben Tages, eine Version. Kommt eine Region dazu,
ändern sich auch die Grenzkacheln ihrer Nachbarn, die dann mit demselben Datenstand neu gebaut werden müssen. Ein
Release nur für die neue Region gibt es darum nicht. Wo die Abdeckung endet und keine Nachbarregion existiert,
fehlen einer Randkachel die Wege jenseits der Grenze; sie wird trotzdem veröffentlicht (Fähren laufen durch Kacheln
außerhalb aller Umrisse) und ist vollständig, sobald der Nachbar dazukommt.

## Vierteljährlicher Lauf und Releases

[`bauen.yml`](.github/workflows/bauen.yml) läuft am 1. Januar, April, Juli und Oktober um 02:17 UTC und lässt sich unter
Actions → „Wegegraph bauen“ → „Run workflow“ von Hand starten. Jobs:

| Job | je | macht |
|---|---|---|
| `plan` | Lauf | Version, Datei je Auszug, Umrisse, `cargo test`, Builder, prüft die Regionen; legt das Release als Entwurf an |
| `randwege` | Region | lädt die Auszüge von Geofabrik (mit Prüfsumme), schreibt Randwege und Ortsliste |
| `kacheln` | Region | baut die Kacheln aus Auszügen und Randwegen der Nachbarn, prüft je Kachel 25 MiB, packt sie (je Archiv 2 GiB) und lädt das Archiv ins Release |
| `veroeffentlichen` | Lauf | Ortsverzeichnis aus allen Ortslisten, Zeiger, `wegegraph.json`, Prüfsummen; macht das Release sichtbar und löscht ältere |

Alle Regionen nehmen die Auszüge desselben Tages: das älteste Datum der neuesten Auszüge (Geofabrik aktualisiert sie
nacheinander und hält ältere Tage eine Woche vor). Der Auszug geht als Artefakt vom Job `randwege` zum Job `kacheln`
derselben Region, Geofabrik liefert ihn nur einmal. Großen Auszügen (über 4 GB) räumt der Job vorher Platz frei. Die
drei neuesten Releases bleiben, ältere werden gelöscht. Ein **Probelauf** baut und misst alles, legt aber kein Release
an: von Hand („Probelauf“ ankreuzen) oder mit einem Push auf einen Branch `probelauf/…`.

**Version** ist das Datum der OSM-Daten plus der Commit des Builders, z. B. `2026-10-18-cf1e32f`, für alle Regionen
gleich. Gibt es das Release schon, baut der Lauf nichts; einen Entwurf aus einem abgebrochenen Lauf ersetzt er. Kacheln
und Ortsverzeichnis einer Version ändern sich nie; ein Server darf sie dauerhaft cachen.

### Release

Die Auslieferung braucht keine Sonderfälle: **Alle Archive (`*.tar.gz`) gehören entpackt in denselben Ordner** (den
Ordner `graph/` eines Servers), keine Datei steht in zwei Archiven, und zusammen ergeben sie genau die Dateien, die
ein Server ausliefert:

```
aktuell.json                    {"version": "<Version>"}, Zeiger auf die neueste Version
LIZENZ.txt                      ODbL-Vermerk und Quellen
<Version>/12/<x>/<y>.bin        Kacheln aller Regionen (docs/wegegraph-format.md)
<Version>/orte/index.json       Ortsverzeichnis (docs/ortsverzeichnis-format.md)
<Version>/orte/<teil>.json
```

| Datei | Inhalt |
|---|---|
| `wegegraph-<Region>.tar.gz` | `<Version>/12/<x>/<y>.bin`: die Kacheln der Region, eines je Zeile in `regionen.txt` (auch leer, wenn eine Region keine Kachel besitzt) |
| `ortsverzeichnis.tar.gz` | `<Version>/orte/`: das Ortsverzeichnis aller Regionen |
| `zeiger.tar.gz` | `aktuell.json` und `LIZENZ.txt` |
| `wegegraph.json` | Version, OSM-Datum, Commit, `archive` (die Liste aller Archive oben), Summen, je Region Quelle, Kacheln, Größe, größte Kachel, Speicher und Dauer von Randwegen und Bau, Ortsverzeichnis |
| `SHA256SUMS` | Prüfsummen aller Dateien außer sich selbst |

Ausliefern heißt also: `SHA256SUMS` prüfen, jedes Archiv aus `wegegraph.json` → `archive` (oder einfach jedes `*.tar.gz`)
in einen Ordner entpacken, `<Version>/` hochladen und zuletzt `aktuell.json` umstellen. Mit einer weiteren Region
kommt nur ein Archiv dazu. Jedes Archiv bleibt unter 2 GiB (eine Region mit ≈ 10 GB PBF hat ≈ 1 GB gepackt), ein
Release unter den 1 000 Dateien von GitHub (die Welt in Geofabrik-Ländern sind ≈ 200 Regionen). Die frühere Grenze von
20 000 Dateien gilt nicht mehr (gpxart.com liefert aus R2 aus, gpxart.com #84). Releases bis `2026-10-18-…` hatten
nur `wegegraph.tar.gz` (DACH, mit Zeiger und Lizenz) und `ortsverzeichnis.tar.gz`.

gpxart.com holt jedes Quartal das neueste Release ab und liefert es aus; dieses Repo braucht dafür keine Zugangsdaten.

## Größen und Grenzen (gemessen)

**Ganz Europa** ohne Russland, Probelauf auf GitHub Actions (Branch `probelauf/europa`, Lauf 41047593278, Auszüge vom
28.10.2026) mit 45 Auszügen in 13 Regionen: alle Jobs grün, **44 min für alles**, kein Job über 6,6 GiB Speicher.

| Region | Auszüge | Kacheln | Größe, gepackt | größte Kachel | Randwege (Schritt 1) | Kacheln (Schritt 2) |
|---|---|---|---|---|---|---|
| `dach` | 6,26 GB | 12 853 | 1 231 MB, 657 MB | 1,38 MB | 5:42, 6,6 GiB | 8:27, 6,6 GiB |
| `france` (+ Monaco, Andorra) | 5,11 GB | 14 022 | 967 MB, 525 MB | 1,83 MB | 4:01, 5,2 GiB | 4:19, 5,2 GiB |
| `norden` (NO, SE, DK, FI, IS, FO) | 3,66 GB | 65 478 | 751 MB, 417 MB | 1,01 MB | 2:31, 3,1 GiB | 3:24, 3,2 GiB |
| `britain` (GB, IE, IM, GG, JE) | 2,64 GB | 12 587 | 560 MB, 311 MB | 1,28 MB | 2:04, 3,1 GiB | 2:13, 3,1 GiB |
| `italia` (+ Malta) | 2,26 GB | 8 659 | 605 MB, 324 MB | 1,44 MB | 1:54, 3,3 GiB | 2:47, 3,3 GiB |
| `benelux` | 2,15 GB | 2 030 | 209 MB, 115 MB | 1,10 MB | 1:40, 1,1 GiB | 1:41, 1,4 GiB |
| `polska` | 2,11 GB | 9 013 | 393 MB, 215 MB | 1,59 MB | 1:48, 2,1 GiB | 2:23, 2,1 GiB |
| `iberia` (ES, PT, Azoren) | 1,94 GB | 13 719 | 799 MB, 423 MB | 1,86 MB | 1:30, 4,1 GiB | 2:17, 4,1 GiB |
| `mitte` (CZ, SK, HU, SI) | 1,94 GB | 5 658 | 340 MB, 183 MB | 1,16 MB | 1:06, 1,4 GiB | 1:42, 1,9 GiB |
| `suedosten` (11 Länder) | 1,71 GB | 18 304 | 632 MB, 343 MB | 1,11 MB | 0:53, 2,6 GiB | 2:03, 2,7 GiB |
| `osten` (UA, BY) | 1,23 GB | 21 346 | 323 MB, 179 MB | 0,91 MB | 0:54, 1,3 GiB | 1:15, 1,4 GiB |
| `tuerkei` (+ Georgien, Zypern) | 0,79 GB | 16 674 | 425 MB, 232 MB | 0,83 MB | 0:28, 1,8 GiB | 0:43, 2,0 GiB |
| `baltikum` | 0,49 GB | 6 838 | 107 MB, 60 MB | 0,76 MB | 0:20, 0,4 GiB | 0:27, 0,5 GiB |
| zusammen | 32,3 GB | 207 181 | 7,34 GB, 3,99 GB | | | |

Der längste Weg sind `france` und `dach`: Download 13 bzw. 11 min, Randwege, Übergabe, Kacheln. Randwege je Region
höchstens 33 MB (`mitte`). Platte: 88 GB frei, nach dem Aufräumen 110 GB. Ortsverzeichnis Europa: 1 094 088 Orte in
521 Ländern, 1:34 min und 1 GiB im Job `veroeffentlichen`; mit Format 2 (gpxart.com #90) 3 922 Dateien, 96 MB, größter
Teil 300 kB.

**Grenzen einmal lokal geprüft** (Windows-PC, Auszüge vom 28.10.2026): `netherlands`, `belgium`, `luxembourg`,
`denmark` und `sweden` als fünf Regionen gegen alle fünf in einem Stück: 27 735 Kacheln, alle byte-gleich, keine fehlt,
keine doppelt; darin die Enklaven Baarle-Hertog/-Nassau, das Dreiländereck, der Øresund und Fähren über das Kattegat.

Probelauf mit `dach`, `france`, `monaco` und `andorra` als vier Regionen (Auszüge vom 28.10.2026, Lauf 40879450270,
40 min für alles; die Regionen laufen nebeneinander):

| Region | Auszug | Kacheln | Größe, gepackt | größte Kachel | Randwege (Schritt 1) | Kacheln (Schritt 2) |
|---|---|---|---|---|---|---|
| `dach` | 6,26 GB | 14 180 | 1,23 GB, 659 MB | 1,38 MB (`12/2200/1343`, Berlin-Mitte) | 5 min 9 s, 6,5 GiB | 7 min 33 s, 6,6 GiB |
| `france` | 5,11 GB | 16 129 | 967 MB, 525 MB | 1,83 MB (`12/2074/1409`, Paris) | 3 min 57 s, 5,2 GiB | 5 min 9 s, 5,2 GiB |
| `andorra` | 3,5 MB | 133 | 1,1 MB, 0,6 MB | 0,28 MB | 0,2 s | 0,3 s |
| `monaco` | 0,7 MB | 0 (gehören `france`) | – | – | 0,02 s | 0,02 s |
| zusammen | | 30 442 | 2,20 GB, 1,18 GB | | | |

Zeiten und Speicher (Max RSS) gelten für den Builder; dazu kommen je Region der Download von Geofabrik (DACH 11 min,
Frankreich 13 min), die Ortsliste (je gut 2½ min, 0,3 GiB) und die Übergabe des Auszugs als Artefakt (1–7 min). Grenzen:
16 GB RAM (15 GiB nutzbar), 6 h je Job, je Kachel 25 MiB, je Release-Datei 2 GiB; Platte hatte der Runner im Probelauf
86 GB frei, gebraucht werden je Job gut 8 GB. Der Speicher wächst mit dem Auszug (gut 1 GiB je GB PBF); bis ≈ 10 GB PBF
je Region reicht der Runner, die Welt also in Ländern und bei großen Ländern in Geofabriks Teilauszügen.

Frühere Messung, ganz DACH in einem Stück (`dach-261018`, erstes Release): 14 255 Kacheln, 20,8 Mio. Wege und 125 Mio.
Knoten im PBF, Graph 76,8 Mio. Knoten und 84,2 Mio. Kanten; Builder 7 min 13 s, 6,4 GiB; auf einem Windows-PC
4½–5 min, 6,0 GiB.

Den Speicher bestimmt vor allem das Lesen (5,6 GiB): Knoten-IDs, die `osmpbf` beim Lesen sammelt, dazu alle Knoten (16
Byte) und Wege. Das Schneiden in Streifen braucht danach knapp 1 GiB mehr.

Zu den Kacheln gehören auch Fährlinien aus DACH heraus (z. B. nach Skandinavien); deshalb reicht der gebaute Bereich
über DACH hinaus.

**Ortsverzeichnis** aller Regionen (Probelauf): 401 263 Orte in 68 Ländern (Bundesländer, Kantone, Régions, dazu
Liechtenstein, Monaco, Andorra), 423 Dateien, 39,8 MB, Bau aus den Ortslisten 8 s. Französische Namen beginnen oft mit
Le/La/Les: `le.json` hat 74 493 Einträge, 4,6 MB (gzip 1,1 MB), `la.json` 4,3 MB; die übrigen Teile höchstens 1,3 MB.
DACH allein (`dach-261018`, Windows-PC): 126 417 Orte (101 Großstädte, 2 621 Städte, 7 410 Stadtteile,
1 886 Viertel, 46 410 Dörfer, 67 988 Weiler) in 52 Ländern und Kantonen; 324 Teile und `index.json`, zusammen 8,6 MB,
gepackt (gzip) 2,8 MB. Ein Abruf beim Tippen ist meist wenige kB groß, der größte Teil (`sc.json`) 319 kB, gepackt
96 kB. Bau gut 2 min (fast nur Lesen), höchstens 0,5 GiB Speicher. Im Genfersee fehlen dem Auszug fünf Wege der
Grenze von Schweiz und Waadt; der Builder schließt die Lücke (10,7 km) gerade.
