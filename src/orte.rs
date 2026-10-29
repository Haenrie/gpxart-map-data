//! Ortsverzeichnis für die Ortssuche im Planer von gpxart.com (#39): Städte, Orte, Dörfer und Stadtteile aus OSM, je
//! mit Art, Lage und Größe, dazu zur Unterscheidung gleichnamiger Orte das Land (Bundesland, Kanton) und die
//! nächstgrößere Stadt. Das Verzeichnis ist nach den Anfangsbuchstaben der Wörter im Namen geteilt, damit der Planer
//! beim Tippen nur einen kleinen Teil lädt. Format und Regeln: `docs/ortsverzeichnis-format.md`.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Write;
use std::path::Path;

use osmpbf::{Element, ElementReader, IndexedReader, RelMemberType};

use crate::flaeche::Flaeche;
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
/// So viele Anfangsbuchstaben eines Wortes bestimmen den Teil des Verzeichnisses.
const SCHLUESSEL: usize = 2;
/// Wörter ab dem zweiten zählen erst ab dieser Länge („Landsberg am Lech“ steht unter „la“ und „le“, nicht „am“).
const WORT_MIN: usize = 3;
/// So lang dürfen die Lücken in einer Grenze zusammen sein, die gerade Linien schließen (km), …
const MAX_LUECKEN_KM: f64 = 100.0;
/// … und so groß der Anteil der fehlenden Wege (in Teilauszügen fehlt meist der größte Teil).
const MAX_ANTEIL_FEHLT: f64 = 0.05;

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

/// Grenz-Relation eines Landes (`admin_level=4`) oder Staates (`admin_level=2`)
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
/// Ortsverzeichnis, `ortsliste` die Liste einer Region dafür. Was in mehreren Dateien steht, zählt einmal (nach
/// OSM-ID).
#[derive(Default)]
pub struct Orte {
    /// Nur die Grenzen dieser Staaten (ISO 3166-1) zählen, ohne Angabe die aller.
    staaten: Option<Vec<String>>,
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
    /// Grenzen, deren Lücken geschlossen sind, und unvollständige, die unberücksichtigt bleiben
    pub meldungen: Vec<String>,
}

/// Ein Ort mit ID und Land (Name deutsch, englisch, Staat), so wie er ins Verzeichnis kommt
type OrtImLand = (i64, Ort, Option<[String; 3]>);

impl Grenze {
    /// Eintrag in der Liste der Länder: Name deutsch, englisch, Staat
    fn land(&self) -> [String; 3] {
        [
            self.name[0].clone(),
            self.name[1].clone(),
            self.staat.clone(),
        ]
    }
}

enum Fund {
    Ort(i64, Ort),
    Grenze(i64, Grenze),
}

impl Orte {
    /// Orte, bei denen nur die Grenzen dieser Staaten (ISO 3166-1) zählen: die einer Region. `Orte::default()` nimmt
    /// die Grenzen aller Staaten.
    pub fn fuer_staaten(staaten: &[String]) -> Orte {
        Orte {
            staaten: Some(staaten.to_vec()),
            ..Orte::default()
        }
    }

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

    /// Nimmt die Relation auf, wenn sie die Grenze eines Landes oder Staates ist.
    pub fn add_relation(&mut self, id: i64, ways: &[i64], tags: &[(&str, &str)]) {
        let staaten = self.staaten.as_deref();
        if let Some(grenze) = grenze(staaten, tags.iter().copied(), ways.to_vec()) {
            self.grenzen.insert(id, grenze);
        }
    }

    /// Liest Orte und Landesgrenzen aus einer `.osm.pbf`-Datei: erst alle Orte und Grenz-Relationen (parallel), dann
    /// die Wege der Grenzen mit ihren Knoten.
    pub fn read_pbf(&mut self, path: &Path) -> Result<(), osmpbf::Error> {
        let staaten = self.staaten.as_deref();
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
                    grenze(staaten, r.tags(), wege)
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
        let (orte, meldungen) = self.im_land();
        Ortsverzeichnis {
            meldungen,
            ..verzeichnis_aus(&orte, bereiche)
        }
    }

    /// Die Orte mit ihrem Land als Ortsliste einer Region (Text), aus der `Ortsverzeichnis::aus_listen` mit den Listen
    /// der anderen Regionen das Verzeichnis baut; dazu die Zahl der Orte und die Meldungen wie bei `verzeichnis`.
    pub fn ortsliste(&self) -> (String, usize, Vec<String>) {
        let (orte, meldungen) = self.im_land();
        let mut text = String::new();
        for (id, ort, land) in &orte {
            let land = land
                .as_ref()
                .map_or(["", "", ""], |l| [&l[0], &l[1], &l[2]]);
            let _ = write!(
                text,
                "{id}\t{}\t{}\t{}\t{}",
                ARTEN[ort.art].0, ort.lat, ort.lon, ort.groesse
            );
            let namen = ort.namen.iter().flat_map(|(s, n)| [*s, n.as_str()]);
            for feld in land.into_iter().chain([ort.name.as_str()]).chain(namen) {
                text.push('\t');
                for c in feld.chars() {
                    match c {
                        '\\' => text.push_str("\\\\"),
                        '\t' => text.push_str("\\t"),
                        '\n' => text.push_str("\\n"),
                        '\r' => text.push_str("\\r"),
                        c => text.push(c),
                    }
                }
            }
            text.push('\n');
        }
        (text, orte.len(), meldungen)
    }

    /// Orte mit ihrem Land (Land, sonst Staat wie Liechtenstein). Gibt es eine vollständige Staatsgrenze, fallen Orte
    /// ohne Land weg (der Auszug reicht über die Staaten hinaus), sonst (Teilauszüge) bleiben alle.
    fn im_land(&self) -> (Vec<OrtImLand>, Vec<String>) {
        let mut meldungen = Vec::new();
        let mut flaechen: Vec<(&Grenze, Flaeche)> = Vec::new();
        for grenze in self.grenzen.values() {
            let name = format!("{} ({})", grenze.name[0], grenze.staat);
            match self.flaeche(grenze) {
                Ok((f, luecken)) => {
                    if luecken > 0.0 {
                        meldungen.push(format!("{name}: Lücken {luecken:.1} km geschlossen"));
                    }
                    flaechen.push((grenze, f));
                }
                Err(e) => {
                    meldungen.push(format!("{name}: unvollständig, nicht berücksichtigt ({e})"))
                }
            }
        }
        flaechen.sort_by_key(|(g, _)| std::cmp::Reverse(g.ebene));
        let land_von = |ort: &Ort| {
            flaechen
                .iter()
                .find(|(_, f)| f.enthaelt(ort.lat, ort.lon))
                .map(|(g, _)| g.land())
        };
        let mit_staaten = flaechen.iter().any(|(g, _)| g.ebene == 2);
        let orte = self
            .orte
            .iter()
            .map(|(&id, ort)| (id, ort.clone(), land_von(ort)))
            .filter(|(_, _, land)| land.is_some() || !mit_staaten)
            .collect();
        (orte, meldungen)
    }

    /// Fläche innerhalb der Grenze, oder warum es keine gibt. Fehlen einzelne Wege (der DACH-Auszug schneidet etwa die
    /// Grenze im Genfersee ab), schließen gerade Linien zwischen den offenen Enden die Ringe, solange höchstens
    /// `MAX_ANTEIL_FEHLT` der Wege fehlen und die Linien zusammen höchstens `MAX_LUECKEN_KM` lang sind.
    fn flaeche(&self, grenze: &Grenze) -> Result<(Flaeche, f64), String> {
        let mut kanten = Vec::new();
        let mut enden: HashMap<i64, u32> = HashMap::new();
        let (mut ohne_weg, mut ohne_knoten) = (0, 0);
        let wege: BTreeSet<i64> = grenze.wege.iter().copied().collect();
        for id in wege {
            let Some(knoten) = self.wege.get(&id).filter(|k| !k.is_empty()) else {
                ohne_weg += 1;
                continue;
            };
            let (erster, letzter) = (knoten[0], knoten[knoten.len() - 1]);
            if erster != letzter {
                *enden.entry(erster).or_default() += 1;
                *enden.entry(letzter).or_default() += 1;
            }
            let punkte: Option<Vec<(f64, f64)>> =
                knoten.iter().map(|k| self.knoten.get(k).copied()).collect();
            match punkte {
                Some(p) => kanten.extend(p.windows(2).map(|p| (p[0], p[1]))),
                None => ohne_knoten += 1,
            }
        }
        // Offene Enden paarweise verbinden, jeweils mit dem nächsten
        let mut offen: Vec<(f64, f64)> = enden
            .iter()
            .filter(|(_, n)| *n % 2 == 1)
            .filter_map(|(k, _)| self.knoten.get(k).copied())
            .collect();
        offen.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
        let mut luecken = 0.0;
        while let Some(a) = offen.pop() {
            let Some((i, km)) = offen
                .iter()
                .map(|&b| km(a, b))
                .enumerate()
                .min_by(|x, y| x.1.total_cmp(&y.1))
            else {
                luecken = f64::INFINITY;
                break;
            };
            kanten.push((a, offen.swap_remove(i)));
            luecken += km;
        }
        // Schließen sich die vorhandenen Wege ohne Lücke zu Ringen, fehlen nur ganze Ringe (Frankreich ohne seine
        // Überseegebiete); die Grenze zählt dann mit den vorhandenen.
        let anteil = ohne_weg as f64 / grenze.wege.len().max(1) as f64;
        if ohne_knoten > 0
            || kanten.is_empty()
            || (anteil > MAX_ANTEIL_FEHLT && luecken > 0.0)
            || luecken > MAX_LUECKEN_KM
        {
            return Err(format!(
                "{ohne_weg} von {} Wegen fehlen, {ohne_knoten} Wegen fehlen Knoten, Lücken {luecken:.0} km",
                grenze.wege.len()
            ));
        }
        Ok((Flaeche::neu(kanten), luecken))
    }
}

impl Ortsverzeichnis {
    /// Baut das Verzeichnis aus den Ortslisten der Regionen (`Orte::ortsliste`). Steht ein Ort in mehreren, zählt er
    /// einmal, mit Land, wenn eine Liste es kennt.
    pub fn aus_listen(listen: &[String]) -> Result<Ortsverzeichnis, String> {
        let mut orte: BTreeMap<i64, (Ort, Option<[String; 3]>)> = BTreeMap::new();
        for (nr, zeile) in listen.iter().flat_map(|l| l.lines()).enumerate() {
            let (id, ort, land) =
                ort_aus_zeile(zeile).ok_or(format!("Ortsliste, Zeile {}", nr + 1))?;
            if orte
                .get(&id)
                .is_none_or(|(_, alt)| alt.is_none() && land.is_some())
            {
                orte.insert(id, (ort, land));
            }
        }
        let orte: Vec<OrtImLand> = orte
            .into_iter()
            .map(|(id, (ort, land))| (id, ort, land))
            .collect();
        Ok(verzeichnis_aus(&orte, &[]))
    }
}

/// Eine Zeile der Ortsliste: ID, Art, Breite, Länge, Größe, Land (deutsch, englisch, Staat; ohne Land leer), Name,
/// dann je Sprache Kürzel und Name; Tabulatoren dazwischen, `\` maskiert Tabulator, Zeilenwechsel und sich selbst.
fn ort_aus_zeile(zeile: &str) -> Option<OrtImLand> {
    let felder: Vec<String> = zeile
        .split('\t')
        .map(|feld| {
            let mut s = String::new();
            let mut zeichen = feld.chars();
            while let Some(c) = zeichen.next() {
                s.push(match c {
                    '\\' => match zeichen.next() {
                        Some('t') => '\t',
                        Some('n') => '\n',
                        Some('r') => '\r',
                        _ => '\\',
                    },
                    c => c,
                });
            }
            s
        })
        .collect();
    if felder.len() < 9 || felder.len().is_multiple_of(2) {
        return None;
    }
    let mut namen = Vec::new();
    for paar in felder[9..].chunks(2) {
        let sprache = SPRACHEN.iter().find(|s| **s == paar[0])?;
        namen.push((*sprache, paar[1].clone()));
    }
    let land =
        (!felder[7].is_empty()).then(|| [felder[5].clone(), felder[6].clone(), felder[7].clone()]);
    let ort = Ort {
        name: felder[8].clone(),
        namen,
        art: ARTEN.iter().position(|(a, _)| *a == felder[1])?,
        lat: felder[2].parse().ok()?,
        lon: felder[3].parse().ok()?,
        groesse: felder[4].parse().ok()?,
    };
    Some((felder[0].parse().ok()?, ort, land))
}

/// Das Verzeichnis aus den Orten mit Land; mit `bereiche` nur die Orte in diesen z12-Kacheln, als nächstgrößere Stadt
/// zählen trotzdem alle.
fn verzeichnis_aus(orte: &[OrtImLand], bereiche: &[TileRange]) -> Ortsverzeichnis {
    let staedte: Vec<(i64, &Ort, f64)> = orte
        .iter()
        .filter(|(_, ort, _)| ort.art < STADT)
        .map(|(id, ort, _)| (*id, ort, (ort.groesse as f64).sqrt()))
        .collect();

    let orte: Vec<&OrtImLand> = orte
        .iter()
        .filter(|(_, ort, _)| bereiche.is_empty() || in_bereichen(ort, bereiche))
        .collect();
    let laender: BTreeMap<&[String; 3], usize> = orte
        .iter()
        .filter_map(|(_, _, land)| land.as_ref())
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
            .then(|| naechstgroessere_stadt(*id, ort, &staedte))
            .flatten();
        let land = land.as_ref().map(|l| laender[l]);
        zeilen.insert(
            *id,
            eintrag_json(ort, land, bei.map_or("", |s| s.name.as_str())),
        );
        let mut schluessel = BTreeSet::new();
        for name in std::iter::once(&ort.name).chain(ort.namen.iter().map(|(_, n)| n)) {
            for (i, wort) in woerter(name).iter().enumerate() {
                if i == 0 || wort.len() >= WORT_MIN {
                    schluessel.insert(wort[..wort.len().min(SCHLUESSEL)].to_string());
                }
            }
        }
        for s in schluessel {
            teile
                .entry(s)
                .or_default()
                .push((ort.groesse, &ort.name, *id));
        }
    }
    let mut dateien = BTreeMap::new();
    for (schluessel, eintraege) in &mut teile {
        eintraege.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(b.1)).then(a.2.cmp(&b.2)));
        let zeilen: Vec<&str> = eintraege.iter().map(|e| zeilen[&e.2].as_str()).collect();
        dateien.insert(
            format!("{schluessel}.json"),
            format!("[\n{}\n]\n", zeilen.join(",\n")),
        );
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
        meldungen: Vec::new(),
    }
}

fn fund_ort<'a>(
    id: i64,
    lat: f64,
    lon: f64,
    tags: impl Iterator<Item = (&'a str, &'a str)>,
) -> Vec<Fund> {
    ort(lat, lon, tags)
        .map(|o| vec![Fund::Ort(id, o)])
        .unwrap_or_default()
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
                if let Some(sprache) = SPRACHEN
                    .iter()
                    .find(|s| k.strip_prefix("name:") == Some(**s))
                {
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

/// Die Relation als Grenze eines Landes oder Staates (nach ISO 3166), mit `staaten` nur eines dieser Staaten
fn grenze<'a>(
    staaten: Option<&[String]>,
    tags: impl Iterator<Item = (&'a str, &'a str)>,
    wege: Vec<i64>,
) -> Option<Grenze> {
    let tags: HashMap<&str, &str> = tags.collect();
    if tags.get("boundary") != Some(&"administrative") {
        return None;
    }
    let (ebene, staat) = match tags.get("admin_level") {
        Some(&"2") => (
            2,
            tags.get("ISO3166-1")
                .or(tags.get("ISO3166-1:alpha2"))?
                .to_string(),
        ),
        Some(&"4") => (4, tags.get("ISO3166-2")?.split_once('-')?.0.to_string()),
        _ => return None,
    };
    if staaten.is_some_and(|s| !s.contains(&staat)) {
        return None;
    }
    let name = tags.get("name")?;
    let in_sprache = |s: &str| {
        tags.get(format!("name:{s}").as_str())
            .unwrap_or(name)
            .to_string()
    };
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
        .take_while(|c| {
            c.is_ascii_digit() || matches!(c, '.' | ',' | ' ' | '\'' | '\u{a0}' | '\u{202f}')
        })
        .filter(char::is_ascii_digit)
        .collect();
    ziffern.parse().ok().filter(|&n| n > 0)
}

/// Entfernung zweier Punkte (Breite, Länge) in km, für Strecken bis einige 100 km genau genug
fn km((lat1, lon1): (f64, f64), (lat2, lon2): (f64, f64)) -> f64 {
    let x = (lon2 - lon1) * ((lat1 + lat2) / 2.0).to_radians().cos() * 111.32;
    let y = (lat2 - lat1) * 110.57;
    x.hypot(y)
}

fn in_bereichen(ort: &Ort, bereiche: &[TileRange]) -> bool {
    let (x, y) = tiling::project(ort.lat, ort.lon);
    let (x, y) = ((x / tiling::EXTENT) as u32, (y / tiling::EXTENT) as u32);
    bereiche.iter().any(|b| b.contains(x, y))
}

/// Die Stadt, auf die man sich bei einem Ort bezieht: die nächste, wobei größere Städte weiter weg sein dürfen
/// (Entfernung geteilt durch die vierte Wurzel der Größe, kleinste gewinnt).
fn naechstgroessere_stadt<'a>(
    id: i64,
    ort: &Ort,
    staedte: &[(i64, &'a Ort, f64)],
) -> Option<&'a Ort> {
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
    let _ = write!(
        s,
        ",{},{},{},{land},",
        grad(ort.lon),
        grad(ort.lat),
        ort.groesse
    );
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
    gefaltet
        .split(' ')
        .filter(|w| !w.is_empty())
        .map(String::from)
        .collect()
}
