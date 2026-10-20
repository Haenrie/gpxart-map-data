# Ortsverzeichnis, Format 1

Das Ortsverzeichnis ist die Quelle der Ortssuche im Planer von gpxart.com: Man tippt einen Ort, und die Karte springt
dorthin. Der Builder baut es mit `wegegraph orte` aus denselben OSM-Daten wie den Wegegraphen. Diese Fassung ist
maßgeblich; der Planer liest das Verzeichnis danach.

Das Verzeichnis ist ein Ordner aus JSON-Dateien (UTF-8). Im Release liegt es als `<Version>/orte/` neben den Kacheln
der Version und ändert sich wie sie nie.

| Datei | Inhalt |
|---|---|
| `index.json` | Zahl der Orte, Liste der Teile, Länder |
| `<teil>.json` | alle Orte, von deren Namen ein Wort mit diesen zwei Buchstaben beginnt, z. B. `la.json` |

Der Planer lädt `index.json` beim ersten Fokus auf das Suchfeld und beim Tippen nur den Teil, der zu den ersten beiden
Buchstaben der Eingabe passt. Gesucht wird im Browser.

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

**Land.** Ein Ort liegt im Land (Bundesland, Kanton), dessen Grenz-Relation ihn umschließt (`boundary=administrative`,
`admin_level=4`, `ISO3166-2` mit `DE-`, `AT-` oder `CH-`), sonst im Staat (`admin_level=2`, `ISO3166-1` `DE`, `AT`,
`CH` oder `LI`; so Liechtenstein). Fehlen einer Grenze einzelne Wege (höchstens 5 %), schließen gerade Linien
zwischen den offenen Enden die Ringe, solange die Lücken zusammen höchstens 100 km lang sind (im DACH-Auszug fehlt
z. B. ein Stück der Grenze im Genfersee); sonst bleibt die Grenze unberücksichtigt. Gibt es mindestens eine
vollständige Staatsgrenze, fallen Orte
außerhalb aller Grenzen weg (der Geofabrik-Auszug reicht etwas über DACH hinaus). In Teilauszügen ohne Staatsgrenzen
bleiben alle Orte, dann teils ohne Land.

**Nächstgrößere Stadt.** Stadtteile, Viertel, Dörfer und Weiler nennen eine Stadt (`city` oder `town`), auf die man
sich bei ihnen bezieht: die mit der kleinsten Entfernung geteilt durch die dritte Wurzel ihrer Größe. So gilt die
nahe Kleinstadt, eine Großstadt darf aber weiter weg sein („Neustadt bei Marburg“, „Kreuzberg von Berlin“). Städte
selbst nennen keine.

Land und nächstgrößere Stadt unterscheiden gleichnamige Orte.

## Teile

Die Wörter eines Namens sind die Folgen aus `a`–`z` und `0`–`9`, nachdem der Name klein geschrieben und gefaltet ist
(Tabelle `FALTUNG` in `src/orte.rs`: `ä` → `a`, `ß` → `ss`, `é` → `e`, `ł` → `l` …); alle anderen Zeichen trennen
Wörter. Ein Ort steht im Teil der ersten beiden Buchstaben jedes Wortes, beim ersten Wort immer, bei weiteren ab drei
Buchstaben, und zwar für den Namen und die Namen in anderen Sprachen. „Landsberg am Lech“ steht also in `la.json` und
`le.json`, „München“ (englisch „Munich“) einmal in `mu.json`. Hat das erste Wort nur einen Buchstaben, heißt der Teil
nach ihm.

## `index.json`

```json
{"format":1,"orte":126417,"teile":["aa","ab",…],"laender":[["Aargau","Aargau","CH"],…,["Bayern","Bavaria","DE"],…]}
```

| Feld | Inhalt |
|---|---|
| `format` | 1 |
| `orte` | Zahl der Orte |
| `teile` | Namen der Teile (ohne `.json`), sortiert; andere gibt es nicht |
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
