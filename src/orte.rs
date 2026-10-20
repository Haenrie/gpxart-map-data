//! Ortsverzeichnis für die Ortssuche im Planer von gpxart.com (#39): Städte, Orte, Dörfer und Stadtteile aus OSM, je
//! mit Art, Lage und Größe, dazu zur Unterscheidung gleichnamiger Orte das Land (Bundesland, Kanton) und die
//! nächstgrößere Stadt. Das Verzeichnis ist nach den Anfangsbuchstaben der Wörter im Namen geteilt, damit der Planer
//! beim Tippen nur einen kleinen Teil lädt. Format und Regeln: `docs/ortsverzeichnis-format.md`.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Write;
use std::path::Path;

use osmpbf::{Element, ElementReader, IndexedReader, RelMemberType};

use crate::{TileRange, tiling};

/// Arten (`place=*`) im Verzeichnis, mit dem Richtwert für die Größe, wenn ein Ort keine Einwohnerzahl nennt. Die
/// ersten beiden sind Städte: Nur sie taugen als nächstgrößere Stadt, und sie selbst bekommen keine.
const ARTEN: [(&str, u32); 6] = [
    ("city", 100_000),
    ("town", 10_000),
    ("suburb", 5_000),
    ("quarter", 2_000),
    ("village", 500),
    ("hamlet", 50),
];
const STADT: usize = 2;
/// Namen in diesen Sprachen kommen dazu, wenn sie vom Namen abweichen (München, Munich).
const SPRACHEN: [&str; 2] = ["de", "en"];
/// Staaten im Verzeichnis (ISO 3166-1): Gibt es ihre Grenzen, fallen Orte außerhalb weg.
const STAATEN: [&str; 4] = ["DE", "AT", "CH", "LI"];
/// So viele Anfangsbuchstaben eines Wortes bestimmen den Teil des Verzeichnisses.
const SCHLUESSEL: usize = 2;
/// Wörter ab dem zweiten zählen erst ab dieser Länge („Landsberg am Lech“ steht unter „la“ und „le“, nicht „am“).
const WORT_MIN: usize = 3;
/// Breite der Streifen, in die die Grenzen für die Strahlprobe geteilt sind (Grad)
const STREIFEN: f64 = 0.02;

#[derive(Clone)]
struct Ort {
    name: String,
    /// abweichende Namen je Sprache aus `SPRACHEN`
    namen: Vec<(&'static str, String)>,
    /// Index in `ARTEN`
    art: usize,
    lat: f64,
    lon: f64,
    groesse: u32,
}

/// Grenz-Relation eines Landes (`admin_level=4`) oder Staates (`admin_level=2`) in DACH
#[derive(Clone)]
struct Grenze {
    ebene: u8,
    /// ISO 3166-1 des Staates
    staat: String,
    /// Name deutsch, englisch
    name: [String; 2],
    wege: Vec<i64>,
}

/// Orte und Landesgrenzen aus OSM, gesammelt aus einer oder mehreren Eingabedateien; `verzeichnis` macht daraus das
/// Ortsverzeichnis. Was in mehreren Dateien steht, zählt einmal (nach OSM-ID).
#[derive(Default)]
pub struct Orte {
    orte: BTreeMap<i64, Ort>,
    grenzen: BTreeMap<i64, Grenze>,
    /// Knoten-IDs der Wege, aus denen die Grenzen bestehen
    wege: HashMap<i64, Vec<i64>>,
    /// Breite und Länge der Knoten dieser Wege
    knoten: HashMap<i64, (f64, f64)>,
}

/// Die Dateien des Ortsverzeichnisses (Name → Inhalt) und was der Bau dazu meldet
pub struct Ortsverzeichnis {
    pub dateien: BTreeMap<String, String>,
    pub orte: usize,
    pub laender: usize,
    /// Grenzen, denen in der Eingabe Wege oder Knoten fehlen; sie bleiben unberücksichtigt.
    pub unvollstaendig: Vec<String>,
}

impl Grenze {
    /// Eintrag in der Liste der Länder: Name deutsch, englisch, Staat
    fn land(&self) -> [&str; 3] {
        [&self.name[0], &self.name[1], &self.staat]
    }
}

enum Fund {
    Ort(i64, Ort),
    Grenze(i64, Grenze),
}

impl Orte {
    /// Nimmt den Knoten auf: als Ort, wenn er einer ist, sonst nur seine Lage (für Grenzen).
    pub fn add_node(&mut self, id: i64, lat: f64, lon: f64, tags: &[(&str, &str)]) {
        if let Some(ort) = ort(lat, lon, tags.iter().copied()) {
            self.orte.insert(id, ort);
        }
        self.knoten.insert(id, (lat, lon));
    }

    pub fn add_way(&mut self, id: i64, nodes: &[i64]) {
        self.wege.insert(id, nodes.to_vec());
    }

    /// Nimmt die Relation auf, wenn sie die Grenze eines Landes oder Staates in DACH ist.
    pub fn add_relation(&mut self, id: i64, ways: &[i64], tags: &[(&str, &str)]) {
        if let Some(grenze) = grenze(tags.iter().copied(), ways.to_vec()) {
            self.grenzen.insert(id, grenze);
        }
    }

    /// Liest Orte und Landesgrenzen aus einer `.osm.pbf`-Datei: erst alle Orte und Grenz-Relationen (parallel), dann
    /// die Wege der Grenzen mit ihren Knoten.
    pub fn read_pbf(&mut self, path: &Path) -> Result<(), osmpbf::Error> {
        let funde = ElementReader::from_path(path)?.par_map_reduce(
            |element| match element {
                Element::DenseNode(n) => fund_ort(n.id(), n.lat(), n.lon(), n.tags()),
                Element::Node(n) => fund_ort(n.id(), n.lat(), n.lon(), n.tags()),
                Element::Relation(r) => {
                    let wege = r
                        .members()
                        .filter(|m| m.member_type == RelMemberType::Way)
                        .map(|m| m.member_id)
                        .collect();
                    grenze(r.tags(), wege)
                        .map(|g| vec![Fund::Grenze(r.id(), g)])
                        .unwrap_or_default()
                }
                Element::Way(_) => Vec::new(),
            },
            Vec::new,
            |mut a, mut b| {
                if a.len() < b.len() {
                    std::mem::swap(&mut a, &mut b);
                }
                a.append(&mut b);
                a
            },
        )?;
        for fund in funde {
            match fund {
                Fund::Ort(id, ort) => {
                    self.orte.insert(id, ort);
                }
                Fund::Grenze(id, grenze) => {
                    self.grenzen.insert(id, grenze);
                }
            }
        }

        let fehlen: HashSet<i64> = self
            .grenzen
            .values()
            .flat_map(|g| &g.wege)
            .filter(|w| !self.wege.contains_key(w))
            .copied()
            .collect();
        if fehlen.is_empty() {
            return Ok(());
        }
        let (wege, knoten) = (&mut self.wege, &mut self.knoten);
        IndexedReader::from_path(path)?.read_ways_and_deps(
            |way| fehlen.contains(&way.id()),
            |element| match element {
                Element::Way(w) => {
                    wege.insert(w.id(), w.refs().collect());
                }
                Element::Node(n) => {
                    knoten.insert(n.id(), (n.lat(), n.lon()));
                }
                Element::DenseNode(n) => {
                    knoten.insert(n.id(), (n.lat(), n.lon()));
                }
                Element::Relation(_) => {}
            },
        )
    }

    /// Baut das Verzeichnis. Mit `bereiche` nur Orte in diesen z12-Kacheln (Testdaten); als nächstgrößere Stadt
    /// zählen trotzdem alle.
    pub fn verzeichnis(&self, bereiche: &[TileRange]) -> Ortsverzeichnis {
        // Länder und Staaten mit vollständiger Grenze; Orte bekommen das Land, sonst den Staat (Liechtenstein).
        let mut unvollstaendig = Vec::new();
        let mut flaechen: Vec<(&Grenze, Flaeche)> = Vec::new();
        for grenze in self.grenzen.values() {
            match self.flaeche(grenze) {
                Some(f) => flaechen.push((grenze, f)),
                None => unvollstaendig.push(format!("{} ({})", grenze.name[0], grenze.staat)),
            }
        }
        flaechen.sort_by_key(|(g, _)| std::cmp::Reverse(g.ebene));
        let land_von = |ort: &Ort| flaechen.iter().find(|(_, f)| f.enthaelt(ort.lat, ort.lon)).map(|(g, _)| *g);

        // Ohne Grenzen (Teilauszüge) bleiben alle Orte, sonst nur die in DACH.
        let orte: Vec<(i64, &Ort, Option<&Grenze>)> = self
            .orte
            .iter()
            .map(|(&id, ort)| (id, ort, land_von(ort)))
            .filter(|(_, _, land)| land.is_some() || flaechen.is_empty())
            .collect();
        let staedte: Vec<(i64, &Ort, f64)> = orte
            .iter()
            .filter(|(_, ort, _)| ort.art < STADT)
            .map(|&(id, ort, _)| (id, ort, (ort.groesse as f64).powf(2.0 / 3.0)))
            .collect();

        let orte: Vec<_> = orte
            .into_iter()
            .filter(|(_, ort, _)| bereiche.is_empty() || in_bereichen(ort, bereiche))
            .collect();
        let laender: BTreeMap<[&str; 3], usize> = orte
            .iter()
            .filter_map(|(_, _, land)| land.map(Grenze::land))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .enumerate()
            .map(|(nummer, land)| (land, nummer))
            .collect();

        // Jeder Ort steht in den Teilen der Anfangsbuchstaben seiner Wörter, in jedem Teil einmal; größte zuerst.
        let mut zeilen: HashMap<i64, String> = HashMap::new();
        let mut teile: BTreeMap<String, Vec<(u32, &str, i64)>> = BTreeMap::new();
        for &(id, ort, land) in &orte {
            let bei = (ort.art >= STADT)
                .then(|| naechstgroessere_stadt(id, ort, &staedte))
                .flatten();
            let land = land.map(|g| laender[&g.land()]);
            zeilen.insert(id, eintrag_json(ort, land, bei.map_or("", |s| s.name.as_str())));
            let mut schluessel = BTreeSet::new();
            for name in std::iter::once(&ort.name).chain(ort.namen.iter().map(|(_, n)| n)) {
                for (i, wort) in woerter(name).iter().enumerate() {
                    if i == 0 || wort.len() >= WORT_MIN {
                        schluessel.insert(wort[..wort.len().min(SCHLUESSEL)].to_string());
                    }
                }
            }
            for s in schluessel {
                teile.entry(s).or_default().push((ort.groesse, &ort.name, id));
            }
        }
        let mut dateien = BTreeMap::new();
        for (schluessel, eintraege) in &mut teile {
            eintraege.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(b.1)).then(a.2.cmp(&b.2)));
            let zeilen: Vec<&str> = eintraege.iter().map(|e| zeilen[&e.2].as_str()).collect();
            dateien.insert(format!("{schluessel}.json"), format!("[\n{}\n]\n", zeilen.join(",\n")));
        }

        let anzahl = orte.len();
        let mut index = format!("{{\"format\":1,\"orte\":{anzahl},\"teile\":[");
        for (i, s) in teile.keys().enumerate() {
            index.push_str(if i == 0 { "" } else { "," });
            json_string(&mut index, s);
        }
        index.push_str("],\"laender\":[");
        for (i, land) in laender.keys().enumerate() {
            index.push_str(if i == 0 { "[" } else { ",[" });
            for (j, teil) in land.iter().enumerate() {
                index.push_str(if j == 0 { "" } else { "," });
                json_string(&mut index, teil);
            }
            index.push(']');
        }
        index.push_str("]}\n");
        dateien.insert("index.json".to_string(), index);

        Ortsverzeichnis {
            dateien,
            orte: anzahl,
            laender: laender.len(),
            unvollstaendig,
        }
    }

    /// Fläche innerhalb der Grenze, oder `None`, wenn Wege oder Knoten fehlen oder die Wege keine Ringe schließen
    fn flaeche(&self, grenze: &Grenze) -> Option<Flaeche> {
        let mut kanten = Vec::new();
        let mut enden: HashMap<i64, u32> = HashMap::new();
        let wege: BTreeSet<i64> = grenze.wege.iter().copied().collect();
        for id in wege {
            let knoten = self.wege.get(&id)?;
            let (&erster, &letzter) = (knoten.first()?, knoten.last()?);
            if erster != letzter {
                *enden.entry(erster).or_default() += 1;
                *enden.entry(letzter).or_default() += 1;
            }
            let punkte: Option<Vec<(f64, f64)>> = knoten.iter().map(|k| self.knoten.get(k).copied()).collect();
            kanten.extend(punkte?.windows(2).map(|p| (p[0], p[1])));
        }
        if kanten.is_empty() || enden.values().any(|n| n % 2 == 1) {
            return None;
        }
        Some(Flaeche::neu(kanten))
    }
}

fn fund_ort<'a>(id: i64, lat: f64, lon: f64, tags: impl Iterator<Item = (&'a str, &'a str)>) -> Vec<Fund> {
    ort(lat, lon, tags).map(|o| vec![Fund::Ort(id, o)]).unwrap_or_default()
}

/// Der Knoten als Ort, wenn er eine der `ARTEN` ist und einen Namen hat
fn ort<'a>(lat: f64, lon: f64, tags: impl Iterator<Item = (&'a str, &'a str)>) -> Option<Ort> {
    let (mut art, mut name, mut einwohner) = (None, None, None);
    let mut namen = Vec::new();
    for (k, v) in tags {
        match k {
            "place" => art = ARTEN.iter().position(|(a, _)| *a == v),
            "name" => name = Some(v.trim()),
            "population" => einwohner = zahl_aus(v),
            _ => {
                if let Some(sprache) = SPRACHEN.iter().find(|s| k.strip_prefix("name:") == Some(**s)) {
                    namen.push((*sprache, v.trim().to_string()));
                }
            }
        }
    }
    let (art, name) = (art?, name.filter(|n| !n.is_empty())?);
    namen.retain(|(_, n)| !n.is_empty() && n != name);
    namen.sort();
    Some(Ort {
        name: name.to_string(),
        namen,
        art,
        lat,
        lon,
        groesse: einwohner.unwrap_or(ARTEN[art].1),
    })
}

/// Die Relation als Grenze eines Landes oder Staates in DACH (nach ISO 3166)
fn grenze<'a>(tags: impl Iterator<Item = (&'a str, &'a str)>, wege: Vec<i64>) -> Option<Grenze> {
    let tags: HashMap<&str, &str> = tags.collect();
    if tags.get("boundary") != Some(&"administrative") {
        return None;
    }
    let (ebene, staat) = match tags.get("admin_level") {
        Some(&"2") => (2, tags.get("ISO3166-1").or(tags.get("ISO3166-1:alpha2"))?.to_string()),
        Some(&"4") => (4, tags.get("ISO3166-2")?.split_once('-')?.0.to_string()),
        _ => return None,
    };
    if !STAATEN.contains(&staat.as_str()) {
        return None;
    }
    let name = tags.get("name")?;
    let in_sprache = |s: &str| tags.get(format!("name:{s}").as_str()).unwrap_or(name).to_string();
    Some(Grenze {
        ebene,
        staat,
        name: [in_sprache("de"), in_sprache("en")],
        wege,
    })
}

/// Einwohnerzahl aus `population`: die erste Zahl, Tausendertrenner („29.317“, „1 234“) übergangen
fn zahl_aus(v: &str) -> Option<u32> {
    let ziffern: String = v
        .trim_start_matches(|c: char| !c.is_ascii_digit())
        .chars()
        .take_while(|c| c.is_ascii_digit() || matches!(c, '.' | ',' | ' ' | '\'' | '\u{a0}' | '\u{202f}'))
        .filter(char::is_ascii_digit)
        .collect();
    ziffern.parse().ok().filter(|&n| n > 0)
}

fn in_bereichen(ort: &Ort, bereiche: &[TileRange]) -> bool {
    let (x, y) = tiling::project(ort.lat, ort.lon);
    let (x, y) = ((x / tiling::EXTENT) as u32, (y / tiling::EXTENT) as u32);
    bereiche.iter().any(|b| b.contains(x, y))
}

/// Die Stadt, auf die man sich bei einem Ort bezieht: die nächste, wobei größere Städte weiter weg sein dürfen
/// (Entfernung geteilt durch die dritte Wurzel der Größe, kleinste gewinnt).
fn naechstgroessere_stadt<'a>(id: i64, ort: &Ort, staedte: &[(i64, &'a Ort, f64)]) -> Option<&'a Ort> {
    let breite = ort.lat.to_radians().cos();
    staedte
        .iter()
        .filter(|(sid, _, _)| *sid != id)
        .map(|&(_, stadt, gewicht)| {
            let (dy, dx) = (stadt.lat - ort.lat, (stadt.lon - ort.lon) * breite);
            ((dx * dx + dy * dy) / gewicht, stadt)
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, stadt)| stadt)
}

/// Eine Zeile des Verzeichnisses: `[Name, Art, Länge, Breite, Größe, Land, nächstgrößere Stadt, {Namen}]`
fn eintrag_json(ort: &Ort, land: Option<usize>, bei: &str) -> String {
    let mut s = String::from("[");
    json_string(&mut s, &ort.name);
    s.push(',');
    json_string(&mut s, ARTEN[ort.art].0);
    let land = land.map_or("null".to_string(), |l| l.to_string());
    let _ = write!(s, ",{},{},{},{land},", grad(ort.lon), grad(ort.lat), ort.groesse);
    json_string(&mut s, bei);
    if !ort.namen.is_empty() {
        s.push_str(",{");
        for (i, (sprache, name)) in ort.namen.iter().enumerate() {
            s.push_str(if i == 0 { "" } else { "," });
            json_string(&mut s, sprache);
            s.push(':');
            json_string(&mut s, name);
        }
        s.push('}');
    }
    s.push(']');
    s
}

/// Grad mit höchstens 4 Nachkommastellen (gut 10 m), ohne Nullen am Ende
fn grad(x: f64) -> String {
    let s = format!("{x:.4}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn json_string(s: &mut String, text: &str) {
    s.push('"');
    for c in text.chars() {
        match c {
            '"' => s.push_str("\\\""),
            '\\' => s.push_str("\\\\"),
            c if (c as u32) < 0x20 => {
                let _ = write!(s, "\\u{:04x}", c as u32);
            }
            c => s.push(c),
        }
    }
    s.push('"');
}

/// Buchstaben mit Akzent und Sonderzeichen, wie die Suche sie vergleicht (dieselbe Tabelle steht im Planer)
const FALTUNG: [(&str, &str); 22] = [
    ("àáâãäåāăą", "a"),
    ("æ", "ae"),
    ("çćĉċč", "c"),
    ("ďđ", "d"),
    ("èéêëēĕėęě", "e"),
    ("ĝğġģ", "g"),
    ("ĥħ", "h"),
    ("ìíîïĩīĭįı", "i"),
    ("ĵ", "j"),
    ("ķ", "k"),
    ("ĺļľŀł", "l"),
    ("ñńņňŉ", "n"),
    ("òóôõöøōŏő", "o"),
    ("œ", "oe"),
    ("ŕŗř", "r"),
    ("śŝşšș", "s"),
    ("ß", "ss"),
    ("ţťŧț", "t"),
    ("ùúûüũūŭůűų", "u"),
    ("ŵ", "w"),
    ("ýÿŷ", "y"),
    ("źżž", "z"),
];

/// Wörter des Namens, wie die Suche sie vergleicht: klein, ohne Akzente (ä → a, ß → ss), nur a–z und 0–9
pub(crate) fn woerter(name: &str) -> Vec<String> {
    let mut gefaltet = String::new();
    for c in name.chars().flat_map(char::to_lowercase) {
        match c {
            'a'..='z' | '0'..='9' => gefaltet.push(c),
            c => match FALTUNG.iter().find(|(von, _)| von.contains(c)) {
                Some((_, nach)) => gefaltet.push_str(nach),
                None => gefaltet.push(' '),
            },
        }
    }
    gefaltet.split(' ').filter(|w| !w.is_empty()).map(String::from).collect()
}

/// Fläche innerhalb einer Grenze für die Strahlprobe: alle Kanten der Grenze, nach Breitenstreifen sortiert
struct Flaeche {
    sued: f64,
    west: f64,
    nord: f64,
    ost: f64,
    /// je Streifen die Kanten, die ihn berühren: (Breite, Länge) von Anfang und Ende
    streifen: Vec<Vec<((f64, f64), (f64, f64))>>,
}

impl Flaeche {
    fn neu(kanten: Vec<((f64, f64), (f64, f64))>) -> Flaeche {
        let punkte = || kanten.iter().flat_map(|&(a, b)| [a, b]);
        let sued = punkte().map(|p| p.0).fold(f64::INFINITY, f64::min);
        let nord = punkte().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max);
        let west = punkte().map(|p| p.1).fold(f64::INFINITY, f64::min);
        let ost = punkte().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
        let streifen_von = |lat: f64| ((lat - sued) / STREIFEN) as usize;
        let mut streifen = vec![Vec::new(); streifen_von(nord) + 1];
        for (a, b) in kanten {
            for s in &mut streifen[streifen_von(a.0.min(b.0))..=streifen_von(a.0.max(b.0))] {
                s.push((a, b));
            }
        }
        Flaeche {
            sued,
            west,
            nord,
            ost,
            streifen,
        }
    }

    /// Strahlprobe nach Osten: Liegt der Punkt innen? Innere Ringe (Enklaven) zählen von selbst richtig.
    fn enthaelt(&self, lat: f64, lon: f64) -> bool {
        if lat < self.sued || lat > self.nord || lon < self.west || lon > self.ost {
            return false;
        }
        let mut innen = false;
        for &((y1, x1), (y2, x2)) in &self.streifen[((lat - self.sued) / STREIFEN) as usize] {
            if (y1 > lat) != (y2 > lat) && lon < (x2 - x1) * (lat - y1) / (y2 - y1) + x1 {
                innen = !innen;
            }
        }
        innen
    }
}
