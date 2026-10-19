//! Prüfpunkt Builder (gpxart.com #18): Ein winziger OSM-Ausschnitt in Landsberg wird zu Kacheln gebaut,
//! stichprobenartig gegen die Regeln aus gpxart.com #11. Die Kacheln werden hier unabhängig vom Builder nach
//! `docs/wegegraph-format.md` gelesen.

use wegegraph::{OsmExtract, TileRange, build_tiles};

/// Kacheln rund um die Stadt Landsberg am Lech (z12).
const LANDSBERG: TileRange = TileRange {
    x0: 2170,
    x1: 2172,
    y0: 1422,
    y1: 1424,
};

/// Stufen und Arten nach dem Format-Dokument
const FREI: u32 = 0;
const HINWEIS: u32 = 1;
const GEMIEDEN: u32 = 2;
const GESPERRT: u32 = 3;
const ART_EINBAHNSTRASSE: u32 = 5;
const ART_TREPPE: u32 = 9;

/// Ein Weg aus zwei Knoten, genau 0,001° (111,2 m) von Süden nach Norden, an der Länge `lon`.
fn weg_nach_norden(osm: &mut OsmExtract, id: i64, lon: f64, tags: &[(&str, &str)]) {
    osm.add_node(10 * id, 48.0400, lon);
    osm.add_node(10 * id + 1, 48.0410, lon);
    osm.add_way(id, &[10 * id, 10 * id + 1], tags);
}

#[test]
fn zu_fuss_sind_autobahn_auffahrt_und_kraftfahrstrasse_gesperrt() {
    let mut osm = OsmExtract::default();
    weg_nach_norden(&mut osm, 1, 10.8700, &[("highway", "footway")]);
    weg_nach_norden(&mut osm, 2, 10.8750, &[("highway", "motorway")]);
    weg_nach_norden(&mut osm, 3, 10.8760, &[("highway", "motorway_link")]);
    weg_nach_norden(
        &mut osm,
        4,
        10.8770,
        &[("highway", "primary"), ("motorroad", "yes")],
    );

    let kanten = kanten(&osm);

    assert_eq!(kanten.len(), 4, "das Auto braucht alle vier");
    let (fussweg, gesperrt): (Vec<_>, Vec<_>) =
        kanten.iter().partition(|k| k.nach_norden(b"FUSS") == FREI);
    assert_eq!(fussweg.len(), 1);
    assert!(
        (fussweg[0].laenge_m - 111.2).abs() < 0.5,
        "Fußweg-Länge {}",
        fussweg[0].laenge_m
    );
    for kante in gesperrt {
        assert_eq!(stufe(kante.nach_norden(b"FUSS")), GESPERRT);
        assert_eq!(stufe(kante.nach_sueden(b"FUSS")), GESPERRT);
    }
}

#[test]
fn rennrad_treppe_ist_ein_hinweis() {
    let mut osm = OsmExtract::default();
    weg_nach_norden(&mut osm, 1, 10.8700, &[("highway", "steps")]);

    let kanten = kanten(&osm);

    assert_eq!(kanten.len(), 1);
    for richtung in [
        kanten[0].nach_norden(b"RENN"),
        kanten[0].nach_sueden(b"RENN"),
    ] {
        assert_eq!(stufe(richtung), HINWEIS);
        assert!(
            hat_art(richtung, ART_TREPPE),
            "Art Treppe fehlt: {richtung:#b}"
        );
    }
}

#[test]
fn auto_feldweg_ist_gesperrt_zu_fuss_frei() {
    let mut osm = OsmExtract::default();
    weg_nach_norden(&mut osm, 1, 10.8700, &[("highway", "track")]);

    let kanten = kanten(&osm);

    assert_eq!(
        kanten.len(),
        1,
        "der Feldweg bleibt für die anderen im Graphen"
    );
    let feldweg = &kanten[0];
    assert_eq!(stufe(feldweg.nach_norden(b"AUTO")), GESPERRT);
    assert_eq!(stufe(feldweg.nach_sueden(b"AUTO")), GESPERRT);
    assert_eq!(feldweg.nach_norden(b"FUSS"), FREI);
}

#[test]
fn einbahnstrasse_gegen_die_richtung_auto_gesperrt_rennrad_gemieden() {
    let mut osm = OsmExtract::default();
    weg_nach_norden(
        &mut osm,
        1,
        10.8700,
        &[("highway", "residential"), ("oneway", "yes")],
    );

    let kanten = kanten(&osm);

    assert_eq!(kanten.len(), 1);
    let strasse = &kanten[0];
    assert_eq!(strasse.nach_norden(b"AUTO"), FREI);
    assert_eq!(stufe(strasse.nach_sueden(b"AUTO")), GESPERRT);
    assert_eq!(strasse.nach_norden(b"RENN"), FREI);
    let gegen = strasse.nach_sueden(b"RENN");
    assert_eq!(stufe(gegen), GEMIEDEN);
    assert!(hat_art(gegen, ART_EINBAHNSTRASSE));
    assert_eq!(strasse.nach_sueden(b"FUSS"), FREI);
}

// ---------------------------------------------------------------- Kachel lesen (nach Format-Dokument)

/// Eine Kante mit Rasterkoordinaten ihrer Enden (Start → ziel) und den Werten je Stellen-Abschnitt.
struct Kante {
    start: (u64, u64),
    ziel: (u64, u64),
    laenge_m: f64,
    /// (Abschnittsname, Wert in Kantenrichtung, Wert in Gegenrichtung)
    stellen: Vec<([u8; 4], u32, u32)>,
}

impl Kante {
    fn werte(&self, abschnitt: &[u8; 4]) -> (u32, u32) {
        let (_, hin, zurueck) = self
            .stellen
            .iter()
            .find(|(name, _, _)| name == abschnitt)
            .unwrap_or_else(|| panic!("Abschnitt {}", String::from_utf8_lossy(abschnitt)));
        (*hin, *zurueck)
    }

    /// Wert für die Fahrt nach Norden (kleineres Raster-Y).
    fn nach_norden(&self, abschnitt: &[u8; 4]) -> u32 {
        let (hin, zurueck) = self.werte(abschnitt);
        if self.ziel.1 < self.start.1 {
            hin
        } else {
            zurueck
        }
    }

    fn nach_sueden(&self, abschnitt: &[u8; 4]) -> u32 {
        let (hin, zurueck) = self.werte(abschnitt);
        if self.ziel.1 < self.start.1 {
            zurueck
        } else {
            hin
        }
    }
}

fn stufe(wert: u32) -> u32 {
    wert & 3
}

fn hat_art(wert: u32, art: u32) -> bool {
    (wert >> 2) & (1 << art) != 0
}

fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}

fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

/// Alle Kanten aller Kacheln.
fn kanten(osm: &OsmExtract) -> Vec<Kante> {
    build_tiles(osm, &LANDSBERG)
        .values()
        .flat_map(|kachel| kanten_der_kachel(kachel))
        .collect()
}

fn kanten_der_kachel(b: &[u8]) -> Vec<Kante> {
    assert_eq!(&b[0..4], b"GPXW", "Magic");
    assert_eq!(u16_at(b, 4), 2, "Formatversion");
    let (kx, ky) = (u32_at(b, 8) as u64, u32_at(b, 12) as u64);
    let (n, m) = (u32_at(b, 20) as usize, u32_at(b, 24) as usize);
    let abschnitt = |name: &[u8; 4]| {
        (0..u16_at(b, 6) as usize)
            .map(|i| 32 + 12 * i)
            .find(|&e| &b[e..e + 4] == name)
            .map(|e| u32_at(b, e + 4) as usize)
            .unwrap_or_else(|| panic!("Abschnitt {}", String::from_utf8_lossy(name)))
    };
    let (knot, kant, laen) = (abschnitt(b"KNOT"), abschnitt(b"KANT"), abschnitt(b"LAEN"));
    let knoten = |i: usize| {
        (
            kx * 32768 + u16_at(b, knot + 2 * i) as u64,
            ky * 32768 + u16_at(b, knot + 2 * n + 2 * i) as u64,
        )
    };
    let stellen = |e: usize| {
        [b"FUSS", b"RENN", b"GRAV", b"AUTO"]
            .iter()
            .map(|name| {
                let o = abschnitt(name);
                let c = u32_at(b, o) as usize;
                let klasse = b[o + 4 + 8 * c + e] as usize;
                (
                    **name,
                    u32_at(b, o + 4 + 8 * klasse),
                    u32_at(b, o + 8 + 8 * klasse),
                )
            })
            .collect()
    };
    let mut kanten = Vec::with_capacity(m);
    let mut e = 0;
    for u in 0..n {
        for _ in 0..b[kant + 4 * m + u] {
            kanten.push(Kante {
                start: knoten(u),
                ziel: knoten(u32_at(b, kant + 4 * e) as usize),
                laenge_m: u16_at(b, laen + 2 * e) as f64 / 10.0,
                stellen: stellen(e),
            });
            e += 1;
        }
    }
    kanten
}
