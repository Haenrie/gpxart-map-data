# Ortsverzeichnis, Format 2

Das Ortsverzeichnis ist die Quelle der Ortssuche im Planer von gpxart.com: Man tippt einen Ort, und die Karte springt
dorthin. Der Builder baut es mit `wegegraph orte` aus denselben OSM-Daten wie den Wegegraphen. Diese Fassung ist
maßgeblich; der Planer liest das Verzeichnis danach.

Das Verzeichnis ist ein Ordner aus JSON-Dateien (UTF-8). Im Release liegt es als `<Version>/orte/` neben den Kacheln
der Version und ändert sich wie sie nie.

| Datei | Inhalt |
|---|---|
| `index.json` | Zahl der Orte, Liste der Teile, geteilte Teile, Länder |
| `<teil>.json` | alle Orte, von deren Namen ein Wort mit diesen zwei Buchstaben beginnt, z. B. `la.json`; ist ein Teil zu groß, mit mehr Buchstaben (`lem.json`, siehe „Teile“) |

Der Planer lädt `index.json` beim ersten Fokus auf das Suchfeld und beim Tippen nur den Teil, der zur Eingabe passt.
Gesucht wird im Browser.

Format 2 (seit gpxart.com #90) unterscheidet sich von Format 1 nur durch geteilte Teile und das Feld `geteilt`; ein
Verzeichnis in Format 1 ist eines in Format 2 ohne geteilte Teile.

## Welche Orte

Alle OSM-Knoten mit `name` und einer dieser Arten (`place=*`), mit dem Richtwert für die Größe, falls der Knoten keine
Einwohnerzahl (`population`) nennt:

| Art | Richtwert | | Art | Richtwert |
|---|---|---|---|---|
| `city` | 100 000 | | `quarter` | 2 000 |
| `town` | 10 000 | | `village` | 500 |
| `suburb` | 5 000 | | `hamlet` | 50 |

Flächen (Wege und Relationen mit `place=*`) zählen nicht. Steht ein Knoten in mehreren Eingabedateien, zählt er
einmal.

**Land.** Ein Ort liegt im Land, dessen Grenz-Relation ihn umschließt (`boundary=administrative`, `admin_level=4`,
`ISO3166-2`: Bundesland, Kanton, in Frankreich die Région wie `FR-GES` Grand Est), sonst im Staat (`admin_level=2`,
`ISO3166-1`; so Liechtenstein, Monaco, Andorra). Es zählen nur die Grenzen der Staaten, die die Region in
`regionen.txt` nennt (DACH: `DE`, `AT`, `CH`, `LI`; Frankreich: `FR`). Fehlen einer Grenze einzelne Wege (höchstens
5 %), schließen gerade Linien zwischen den offenen Enden die Ringe, solange die Lücken zusammen höchstens 100 km lang
sind (im DACH-Auszug fehlt z. B. ein Stück der Grenze im Genfersee); fehlen nur ganze Ringe (die Grenze von Frankreich
umfasst auch die Überseegebiete, die nicht im Auszug sind), zählt die Grenze mit den vorhandenen Ringen; sonst bleibt
sie unberücksichtigt. Gibt es mindestens eine vollständige Staatsgrenze, fallen Orte außerhalb aller Grenzen weg (ein
Geofabrik-Auszug reicht etwas über seine Staaten hinaus). In Teilauszügen ohne Staatsgrenzen bleiben alle Orte, dann
teils ohne Land.

**Regionen.** Jede Region schreibt ihre Orte mit Land in eine Ortsliste (`wegegraph ortsliste`); aus den Listen aller
Regionen entsteht das eine Verzeichnis (`wegegraph ortsverzeichnis`), mit der nächstgrößeren Stadt über die Grenzen
der Regionen hinweg. Steht ein Ort in mehreren Listen, zählt er einmal, mit Land, wenn eine Liste es kennt.

**Nächstgrößere Stadt.** Stadtteile, Viertel, Dörfer und Weiler nennen eine Stadt (`city` oder `town`), auf die man
sich bei ihnen bezieht: die mit der kleinsten Entfernung geteilt durch die vierte Wurzel ihrer Größe. So gilt die
nahe Kleinstadt, eine Großstadt darf aber weiter weg sein („Neustadt bei Marburg“, „Kreuzberg von Berlin“). Städte
selbst nennen keine.

Land und nächstgrößere Stadt unterscheiden gleichnamige Orte.

## Teile

Die Wörter eines Namens sind die Folgen aus `a`–`z` und `0`–`9`, nachdem der Name klein geschrieben und gefaltet ist
(Tabelle `FALTUNG` in `src/orte.rs`: `ä` → `a`, `ß` → `ss`, `é` → `e`, `ł` → `l` …). Einzelne Akzentzeichen
(U+0300–U+036F) fallen weg, so wird `İ` zu `i`; alle anderen Zeichen trennen Wörter. Ein Ort steht im Teil der ersten beiden Buchstaben jedes Wortes, beim ersten Wort aus mindestens zwei
Buchstaben immer, bei weiteren ab drei Buchstaben, und zwar für den Namen und die Namen in anderen Sprachen.
„Landsberg am Lech“ steht also in `la.json` und `le.json`, „München“ (englisch „Munich“) einmal in `mu.json`,
„L'Isle-Adam“ in `is.json` und `ad.json` (in Format 1 stand er auch in `l.json`, Teile aus einem Buchstaben gibt es
seit Format 2 nicht mehr).

**Geteilte Teile.** Wird ein Teil größer als 300 000 Bytes, wird er nach dem nächsten Buchstaben weiter geteilt, und
zwar nach dem **Rest des Namens ab dem Wort, ohne Zwischenraum**: „Le Mans“ hat zum Wort „le“ den Rest „lemans“ und
steht darum in `lem.json` statt `le.json` (Frankreich: viele Namen beginnen mit Le, La, Les, L'). Ein geteilter Teil
behält nur die Orte, deren Rest nicht länger ist als sein Name (ein Ort, der nur „Le“ heißt); hat er keine, gibt es
die Datei nicht. Ein Teil wird so oft geteilt, wie es nötig ist (`le` → `les` → `lesh`), höchstens bis zu sechs
Buchstaben. Den Teil zu einer Eingabe findet man genauso: das erste
Wort der Eingabe mit mindestens zwei Buchstaben, davon zwei Buchstaben, und solange der Teil in `geteilt` steht und die
Eingabe ab diesem Wort ohne Zwischenraum länger ist, einen Buchstaben mehr davon („Le M“ → `lem`, „Les Herb“ → `lesh`,
„L'Isle“ → `is`).

## `index.json`

```json
{"format":2,"orte":401263,"teile":["aa","ab",…,"lea","leb",…],"geteilt":["la","le"],"laender":[["Aargau","Aargau","CH"],…]}
```

| Feld | Inhalt |
|---|---|
| `format` | 2 |
| `orte` | Zahl der Orte |
| `teile` | Namen der Teile (ohne `.json`), sortiert; andere gibt es nicht |
| `geteilt` | geteilte Teile, sortiert (siehe „Teile“); fehlt in Format 1 |
| `laender` | Länder als `[Name deutsch, Name englisch, Staat]`, Staat nach ISO 3166-1; die Orte verweisen mit der Nummer darauf (ab 0) |

## `<teil>.json`

Eine Liste von Orten, jeder als Liste, die größten zuerst (bei gleicher Größe nach Name):

```json
[
["München","city",11.5754,48.1371,1505036,6,"",{"en":"Munich"}],
["Kaufering","village",10.8573,48.0903,500,6,"Landsberg am Lech"]
]
```

| Stelle | Inhalt |
|---|---|
| 0 | Name (`name`) |
| 1 | Art (`place`) |
| 2, 3 | Länge, Breite in Grad, auf 4 Nachkommastellen gerundet (gut 10 m) |
| 4 | Größe: Einwohnerzahl (`population`, Tausendertrenner übergangen), sonst der Richtwert der Art |
| 5 | Land: Nummer in `laender`, `null` ohne Land |
| 6 | nächstgrößere Stadt (ihr `name`), `""` bei Städten |
| 7 | nur wenn vorhanden: abweichende Namen je Sprache (`name:de`, `name:en`), z. B. `{"de":"Genf","en":"Geneva"}` |

Aus denselben Eingabedateien schreibt der Builder byte-gleiche Dateien.
