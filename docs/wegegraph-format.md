# Wegegraph-Format (Kacheln, Version 2)

Dieses Dokument ist der Vertrag zwischen dem **Builder** (Rust, dieses Repo, schreibt die Kacheln) und dem **Router**
von gpxart.com (TypeScript, liest sie). Beide richten sich nur nach diesem Dokument. Wer das Format ändert, ändert
zuerst dieses Dokument. Diese Fassung ist maßgeblich; gpxart.com hält für seine Router-Tests eine Kopie samt kleiner
Testkacheln, die mit diesem Builder gebaut sind. Verweise wie „gpxart.com #11“ nennen Tickets im (nicht öffentlichen)
Repo von gpxart.com.

## Überblick

- Der Wegegraph ist in **z12-Kacheln** geschnitten (Web Mercator, Nummerierung wie bei OSM-Kartenkacheln: `x` von
  West nach Ost, `y` von Nord nach Süd). Eine Kachel ist in DACH etwa 6–7 km breit.
- Je Kachel gibt es eine Datei `12/<x>/<y>.bin` unterhalb des Graph-Ordners (bzw. der Graph-Version in der URL).
- Der Builder schreibt nur Kacheln mit Wegen. Fehlt eine Datei, gibt es dort keine Wege: Die Kachel ist leer oder
  liegt außerhalb des gebauten Bereichs; für den Router ist das dasselbe. Eine Kachel mit `n = m = 0` bleibt gültig.
- Gebaut wird in Regionen (je ein Geofabrik-Auszug, siehe README des Builders). Für den Router ändert das nichts: Jede
  Kachel ist dieselbe wie bei einem Bau aller Regionen in einem Stück, auch an den Grenzen der Regionen.
- Version 2 enthält einen gemeinsamen Graphen für die Fortbewegungsarten **Zu Fuß, Rennrad, Gravel/MTB und Auto**:
  Knoten, Kanten, Kantenlängen und je Fortbewegungsart die **Stellen** (gpxart.com #11), also für jede Kante und
  Richtung die Stufe (frei, Hinweis, gemieden, gesperrt) und die Arten der Hinweise. Eine Kante, die für alle vier
  gesperrt ist, fehlt. „Ohne Wege“ braucht keinen Graphen. Steigung fehlt (kein Höhenmodell im MVP), hat aber Platz
  (siehe [Neue Attribute](#neue-attribute-und-versionen)).
- Version 1 (nur Zu Fuß, ohne Stellen) ist abgelöst; ein Router für Version 2 lehnt sie ab.

## Koordinaten

Alle Positionen liegen auf einem weltweiten Ganzzahl-Raster in Web Mercator mit **32 768 (2^15) Schritten je
Kachelkante**, also 2^27 Schritten rund um die Welt. Ein Schritt sind 0,30 m × cos(Breite), in DACH etwa 0,2 m.

Ein Knoten speichert seine Position relativ zur Kachel: `qx`, `qy` ∈ [0, 32768] (beide Grenzen eingeschlossen).

```
X = x · 32768 + qx                 (globale Rasterkoordinate)
Y = y · 32768 + qy
Länge = X / 2^27 · 360 − 180
Breite = atan(sinh(π · (1 − 2 · Y / 2^27))) · 180 / π
```

Für Abstände im Router genügt eine lokale Näherung mit demselben Erdradius wie die Kantenlängen:
Meter = Rasterschritte × 2π · 6 371 008,8 / 2^27 × cos(Breite).

## Datei

Alles ist **Little Endian**. Jeder Abschnitt beginnt an einer durch 4 teilbaren Stelle (Füllbytes 0), damit der
Router typisierte Arrays direkt auf den Puffer legen kann (`new Uint16Array(buffer, offset, n)`).

### Kopf (32 Byte)

| Offset | Typ | Feld |
|---|---|---|
| 0 | `u8[4]` | Magic `GPXW` (ASCII) |
| 4 | `u16` | Formatversion, hier `2` |
| 6 | `u16` | `k`: Zahl der Abschnitte |
| 8 | `u32` | Kachel `x` |
| 12 | `u32` | Kachel `y` |
| 16 | `u8` | Zoomstufe `z`, hier `12` |
| 17 | `u8[3]` | reserviert, `0` |
| 20 | `u32` | `n`: Zahl der Knoten |
| 24 | `u32` | `m`: Zahl der Kanten |
| 28 | `u32` | reserviert, `0` |

Reservierte Felder schreibt der Builder als 0; der Router ignoriert sie.

### Abschnittsverzeichnis (ab Offset 32, `k` × 12 Byte)

| Offset im Eintrag | Typ | Feld |
|---|---|---|
| 0 | `u8[4]` | Name (4 ASCII-Zeichen) |
| 4 | `u32` | Offset des Abschnitts ab Dateianfang |
| 8 | `u32` | Länge des Abschnitts in Byte (ohne Füllbytes) |

Der Router sucht Abschnitte über den Namen, nicht über ihre Reihenfolge.

### Abschnitte in Version 2

| Name | Inhalt | Länge |
|---|---|---|
| `KNOT` | `qx: u16[n]`, danach `qy: u16[n]` | 4n |
| `KANT` | `ziel: u32[m]`, danach `grad: u8[n]` | 4m + n |
| `LAEN` | `laenge: u16[m]`, Kantenlänge in Dezimetern | 2m |
| `FUSS` | Stellen für Zu Fuß (siehe [Stellen](#stellen-je-fortbewegungsart)) | 4 + 8c + m |
| `RENN` | Stellen für Rennrad | 4 + 8c + m |
| `GRAV` | Stellen für Gravel/MTB | 4 + 8c + m |
| `AUTO` | Stellen für Auto | 4 + 8c + m |

**Kanten** stehen nur einmal in der Datei, vom kleineren zum größeren Knotenindex; diese Richtung (Start → `ziel`)
heißt **Kantenrichtung**. Befahren werden darf eine Kante in beiden Richtungen, soweit ihre Stellen das nicht sperren
(Einbahnstraße). Sie sind nach
Startknoten sortiert: Die ersten `grad[0]` Kanten beginnen bei Knoten 0, die nächsten `grad[1]` bei Knoten 1 usw.
Kante `e` verbindet also ihren Startknoten mit `ziel[e]`, und es gilt `ziel[e] > Startknoten`. Die Summe aller `grad`
ist `m`. Der Router baut daraus die Nachbarlisten für beide Richtungen.

```ts
let e = 0;
for (let u = 0; u < n; u++)
  for (let j = 0; j < grad[u]; j++, e++) addEdge(u, ziel[e], laenge[e] / 10);
```

Kanten-Abschnitte (`LAEN` und künftige) haben genau `m` Einträge in dieser Kantenreihenfolge, Knoten-Abschnitte genau
`n` Einträge in Knotenreihenfolge. Die Knoten sind nach (`qy`, `qx`) sortiert; das dient nur einer reproduzierbaren
Ausgabe und hat keine weitere Bedeutung.

### Stellen je Fortbewegungsart

Die Abschnitte `FUSS`, `RENN`, `GRAV` und `AUTO` sind gleich aufgebaut: ein kleines Wörterbuch der **Klassen**, die in
der Kachel vorkommen, und je Kante der Index ihrer Klasse.

| Offset im Abschnitt | Typ | Feld |
|---|---|---|
| 0 | `u32` | `c`: Zahl der Klassen, höchstens 256 |
| 4 | `u32[2c]` | je Klasse zwei Werte: erst **in Kantenrichtung**, dann **in Gegenrichtung** (`ziel` → Start) |
| 4 + 8c | `u8[m]` | Klasse jeder Kante, in Kantenreihenfolge |

Ein Wert (`u32`) beschreibt, was gilt, wenn man die Kante in dieser Richtung nutzt:

- Bits 0–1: **Stufe**: `0` frei, `1` Hinweis, `2` gemieden, `3` gesperrt. Gesperrt heißt, die Kante existiert in dieser
  Richtung für die Fortbewegungsart nicht; gemieden heißt, sie kostet mehr und trägt immer mindestens eine Art.
- Bit 2 + i: **Art** i trifft zu (siehe Tabelle). Mehrere Arten sind möglich. Eine Stelle mit Stufe Hinweis oder
  gemieden hat mindestens eine Art; bei gesperrt sind die Arten bedeutungslos.

```ts
const c = dv.getUint32(o, true);
const klasse = new Uint32Array(buf, o + 4, 2 * c);
const index = new Uint8Array(buf, o + 4 + 8 * c, m);
const hin = klasse[2 * index[e]];       // Start → ziel
const zurueck = klasse[2 * index[e] + 1]; // ziel → Start
const stufe = hin & 3, arten = hin >>> 2; // Art i ⇔ Bit i von arten
```

| i | Art | Bedeutung (OSM-Näherung siehe [Inhalt](#inhalt)) |
|---|---|---|
| 0 | Privatweg | `access=private` |
| 1 | Fußgänger verboten | `foot=no` |
| 2 | Weg mit Öffnungszeiten | `opening_hours` am Weg (Park, Friedhof) |
| 3 | Landstraße ohne Gehweg | Bundes-, Landes-, Kreisstraße ohne `sidewalk`, nicht innerorts |
| 4 | Fähre | `route=ferry` |
| 5 | Einbahnstraße gegen die Richtung | nur in der gesperrten bzw. gemiedenen Richtung |
| 6 | Unbefestigt | Schotter, Feld- und Waldweg |
| 7 | Stark befahren | Bundesstraße, über die Straßenklasse |
| 8 | Radfahren verboten | `bicycle=no` |
| 9 | Treppe | `highway=steps` |
| 10 | Schiebestrecke | Fußgängerzone, Gehweg, `bicycle=dismount` |
| 11 | Kopfsteinpflaster | `surface=sett`, `cobblestone` … |
| 12 | Schwieriger Trail | `sac_scale` Bergwandern, `mtb:scale` 2–3 |
| 13 | Alpiner Pfad | `sac_scale` alpin, `mtb:scale` ab 4 |
| 14 | Autobahn | `highway=motorway`, `motorway_link` |
| 15 | Mautstraße | `toll=yes` |
| 16 | Verkehrsberuhigter Bereich | `highway=living_street` |

Neue Arten bekommen die nächste freie Nummer (bis 29); ein Router, der eine Art nicht kennt, behandelt die Stelle nach
ihrer Stufe und zeigt einen allgemeinen Hinweis. Hinweise, die erst aus der Route entstehen („Wenden nötig“ beim
Auto, Gesamthinweis „Ohne Wege“), stehen nicht im Graphen.

## Inhalt

- **Wege:** alle OSM-Wege mit `highway` und alle Fähren (`route=ferry`), außer:
  - Werte, die keine Wege sind: `construction`, `proposed`, `abandoned`, `razed`, `raceway`,
    `bus_guideway`, `platform`, `bus_stop`, `services`, `rest_area`, `elevator`, `emergency_bay`, `no`;
  - Flächen (`area=yes`);
  - Wege, die nach den Regeln unten für alle vier Fortbewegungsarten in beiden Richtungen gesperrt sind.
- **Stellen** nach der Regeltabelle aus gpxart.com #11, ohne Steigung. Was OSM nicht direkt hergibt, nähert der
  Builder an:

  | | Gesperrt | Gemieden | Hinweis |
  |---|---|---|---|
  | **Zu Fuß** | Autobahn (`motorway`, `motorway_link`), Kraftfahrstraße (`motorroad=yes`) | Landstraße ohne Gehweg, Fähre | Privatweg, Fußgänger verboten, Weg mit Öffnungszeiten |
  | **Rennrad** | Autobahn, Kraftfahrstraße | Einbahnstraße gegen die Richtung, unbefestigt, stark befahren, Fähre | Privatweg, Radfahren verboten, Treppe, Schiebestrecke, Kopfsteinpflaster |
  | **Gravel/MTB** | Autobahn, Kraftfahrstraße | Einbahnstraße gegen die Richtung, stark befahren, alpiner Pfad, Fähre | Privatweg, Radfahren verboten, Treppe, Schiebestrecke, schwieriger Trail |
  | **Auto** | ohne Kfz-Freigabe, Privatweg, Anlieger frei, Einbahnstraße gegen die Richtung | Autobahn, Mautstraße, Fähre | verkehrsberuhigter Bereich |

  - **Zugang:** Der erste vorhandene Schlüssel zählt, sonst `access`: zu Fuß `foot`; Rad `bicycle`, `vehicle`; Auto
    `motorcar`, `motor_vehicle`, `vehicle`. Zu Fuß und Rad: `no` → verboten, `private` → Privatweg. Auto: alles außer
    `yes`, `permissive`, `designated` ist gesperrt (`private`, `destination` = Anlieger frei, `agricultural` …).
  - **Kfz-Freigabe** haben `motorway`, `trunk`, `primary`, `secondary`, `tertiary` (samt `_link`), `unclassified`,
    `residential`, `living_street`, `service` und `road`; andere Wege (Feld- und Waldweg `track`, Fußgängerzone …) und
    Fähren nur mit `motorcar` bzw. `motor_vehicle` = `yes`, `permissive` oder `designated`.
  - **Einbahnstraße:** `oneway=yes|true|1` in Wegrichtung, `-1|reverse` dagegen; ohne `oneway`-Tag auch Kreisverkehre
    (`junction=roundabout|circular`) und Autobahnen. Für Rad nicht bei `oneway:bicycle=no` oder `cycleway…=opposite…`;
    zu Fuß nie.
  - **Landstraße ohne Gehweg:** `trunk`, `primary`, `secondary`, `tertiary` (samt `_link`) ohne Gehweg
    (`sidewalk`, `sidewalk:both|left|right` nicht `yes|both|left|right|separate`) und nicht innerorts (`maxspeed` ≤ 50
    oder `urban` in `source:maxspeed`, `zone:traffic`, `maxspeed:type`).
  - **Stark befahren:** `trunk`, `primary` (samt `_link`), also Bundesstraßen; Verkehrsdaten gibt es nicht.
  - **Unbefestigt:** `surface` unbefestigt (`unpaved`, `gravel`, `compacted`, `dirt`, `ground`, `grass` …); ohne
    `surface` auch `track` (außer `tracktype=grade1`), `path` und `bridleway`.
  - **Schiebestrecke:** `footway` und `pedestrian` ohne `bicycle=yes|designated|permissive`, oder `bicycle=dismount`.
  - **Bewusste Ergänzungen zu gpxart.com #11** (Henrys Entscheidung im Review zu gpxart.com #18): Beim Rad ist die
    **Fähre gemieden** (wie zu Fuß), und ein **Reitweg** (`bridleway`) ohne Rad-Freigabe bekommt den Hinweis
    **„Radfahren verboten“**.
  - Bewusst nicht: Steigung (kein Höhenmodell), unbeleuchtete Wege (gpxart.com #11).
- **Knoten:** Kreuzungen (OSM-Knoten in mehr als einem Weg oder mehrfach im selben Weg), Wegenden und Randknoten
  (siehe unten). Dazwischen fasst der Builder Ketten zusammen: Eine Kette endet spätestens an dem Zwischenpunkt, an
  dem sie ab dem letzten Knoten 80 m erreicht. Innerhalb einer Kette bleiben die Zwischenpunkte, die nötig sind, damit
  keiner der OSM-Punkte weiter als **2 m** von seiner Kante abliegt (Douglas-Peucker mit 2 m). So behalten Routen und
  GPX die Form der Wege (gpxart.com #12: keine Ausdünnung, die man sieht). Wegstücke über 5 km teilt der Builder,
  damit jede Länge in `u16` passt.
- **Kanten:** Die Geometrie einer Kante ist die gerade Linie zwischen ihren Knoten; sie weicht höchstens 2 m vom
  OSM-Weg ab. Sie steht in der Kachel, in der ihr Stück des OSM-Weges liegt; liegt sie genau auf einer Kachelgrenze,
  kann das jede der beiden Kacheln sein. Ihre **Länge** ist die echte Länge entlang des OSM-Weges (Großkreis, Erdradius 6 371 008,8 m). Sie ist
  nie kürzer als die Luftlinie zwischen den Knoten, bis auf Rundung (höchstens ≈ 0,3 m).
  Verbinden mehrere Kanten dieselben zwei Knoten und haben sie in allen vier Stellen-Abschnitten dieselbe Klasse, bleibt
  nur die kürzeste. Kanten mit verschiedenen Stellen bleiben nebeneinander stehen (Mehrfachkanten).
- Alle Zusammenhangskomponenten bleiben erhalten. Welche der Router nutzt (z. B. nur die größte im Gebiet),
  entscheidet er selbst, weil das vom geladenen Gebiet abhängt.

## Kacheln zusammensetzen

Jede Kante liegt vollständig in einer Kachel. Wo ein Weg eine Kachelgrenze kreuzt, setzt der Builder einen
**Randknoten** genau auf die Grenzlinie. Beide Nachbarkacheln speichern ihn mit derselben globalen Rasterkoordinate.

- Ein Knoten ist ein Randknoten, wenn `qx` oder `qy` gleich `0` oder `32768` ist.
- **Randknoten verschiedener Kacheln mit gleichem (X, Y) sind derselbe Knoten.** An einer Kachelecke kann ein
  Randknoten in bis zu vier Kacheln stehen. Innere Knoten werden nie zusammengelegt.
- Innerhalb einer Kachel ist jeder Randknoten pro Koordinate eindeutig; ein Router kann also einfach über einen
  Schlüssel aus (X, Y) vereinen.
- Ein Randknoten ohne Partner ist erlaubt (z. B. ein OSM-Knoten, der genau auf der Grenze liegt und nur Kanten in eine
  Kachel hat, oder der Rand des gebauten Bereichs).
- Grenze der Methode: Kreuzen zwei verschiedene Wege eine Kachelgrenze an derselben Stelle (auf ≈ 0,2 m genau), werden
  sie dort verbunden. Das ist praktisch bedeutungslos und wird hingenommen.

## Neue Attribute und Versionen

- **Neue Attribute** kommen als **neuer Abschnitt** mit neuem Namen dazu, z. B. je Kante (`m` Einträge) oder je Knoten
  (`n` Einträge). Ein Router überspringt Abschnitte, die er nicht kennt. Bestehende Abschnitte ändern sich dabei nicht,
  die Formatversion bleibt.
- **Die Formatversion steigt**, wenn bestehende Daten ihre Bedeutung ändern oder ein Router, der einen neuen Abschnitt
  nicht kennt, falsche Routen rechnen würde. Ein Router lehnt Kacheln mit unbekannter Version ab.
- Version 2 kam, weil der Graph seither Kanten enthält, die zu Fuß gesperrt sind (z. B. Autobahn für Auto); ein
  Router für Version 1 hätte sie genutzt.
- Vorgesehen (noch nicht gebaut): **Steigung** (2–4 Byte je Kante), z. B. Abschnitt `STEI` mit `i16[m]` in Promille,
  gemessen in Kantenrichtung, in Gegenrichtung mit umgekehrtem Vorzeichen. Die Steigungs-Hinweise aus gpxart.com #11
  (Rennrad ab 8 %, zu Fuß und Gravel/MTB ab 12 %) leitet der Router dann daraus ab. Das ist ein neuer Abschnitt, die
  Version bleibt 2.

## Größen (gemessen, Version 2)

Je Knoten 5 Byte, je Kante 10 Byte (davon 4 für die Stellen), dazu Kopf, Verzeichnis und die Wörterbücher der Klassen
(in ganz DACH höchstens 67 Klassen je Abschnitt, erlaubt sind 256; höchstens 12 Kanten ab einem Knoten, erlaubt sind
255). Version 1 brauchte je Kante 6 Byte.

| Gebiet | Kacheln | Knoten | Kanten | Größe | gzip |
|---|---|---|---|---|---|
| Ganz DACH (Geofabrik `dach-261018`) | 14 255 | 76 776 917 | 84 198 653 | 1 232 MB | 658 MB |
| Landsberg am Lech (Testkacheln von gpxart.com) | 20 | 145 773 | 158 250 | 2,32 MB | 1,27 MB |
| Berlin: Mitte, Friedrichshain, Kreuzberg, Neukölln (ebenso) | 4 | 221 393 | 285 982 | 3,97 MB | 1,98 MB |

In DACH hat eine Kachel im Median 0,07 MB, 95 % haben höchstens 0,22 MB, die größte 1,38 MB (`12/2200/1343`,
Berlin-Mitte). Mit der Form auf 2 m genau (statt Ketten bis 25° Knick) wuchs Landsberg um 13 % (viele kurvige Feld-
und Waldwege); Berlin schrumpfte um 1,5 %, weil dort viele leichte Knicke unter 2 m Abweichung liegen.
