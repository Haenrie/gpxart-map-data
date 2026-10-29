//! Prüfpunkt Regionen (gpxart.com #85): Zwei Regionen mit eigenen Auszügen, die sich wie bei Geofabrik nur knapp
//! überlappen, bauen ihre Kacheln jede für sich, mit den Randwegen der anderen. Zusammen ergeben sie dieselben Kacheln
//! wie ein Bau aller Wege in einem Stück, jede Kachel genau einmal.

use std::collections::BTreeMap;

use wegegraph::{OsmExtract, Regionen, TileRange, build_tiles, build_tiles_of};

/// Grenze bei 10,85° O mitten in der Kachel x 2171 (10,811–10,898° O); die Umrisse reichen je 0,01° darüber hinaus.
const LISTE: &str = "\
# Name Auszug Staaten
west  test/west  DE
ost   test/ost   AT
";

fn umriss(name: &str) -> Result<String, String> {
    let (w, o) = match name {
        "west" => (9.0, 10.86),
        "ost" => (10.84, 12.0),
        _ => return Err(format!("kein Umriss für {name}")),
    };
    Ok(format!(
        "{name}\n1\n {w} 47.5\n {o} 47.5\n {o} 48.5\n {w} 48.5\nEND\nEND\n"
    ))
}

/// Ein Weg: ID, Knoten (ID, Breite, Länge), Tags
type Weg = (i64, Vec<(i64, f64, f64)>, Vec<(&'static str, &'static str)>);

/// Straßengitter über die Grenze, alle 0,01° ein Knoten und eine Kreuzung, dazu eine Fähre mit nur zwei Knoten, die
/// weit über beide Umrisse hinaus nach Osten führt: Ihr einziges Stück kreuzt Kacheln der Region „ost“, ohne dass ein
/// Knoten darin liegt, und sie steht nur im Auszug von „west“.
fn wege() -> Vec<Weg> {
    let knoten = |i: i64, j: i64| {
        (
            1000 * i + j,
            47.95 + 0.01 * j as f64,
            10.78 + 0.01 * i as f64,
        )
    };
    let strasse = vec![("highway", "residential")];
    let mut wege: Vec<Weg> = Vec::new();
    for j in 0..15 {
        wege.push((
            1 + j,
            (0..20).map(|i| knoten(i, j)).collect(),
            strasse.clone(),
        ));
    }
    for i in 0..20 {
        wege.push((
            100 + i,
            (0..15).map(|j| knoten(i, j)).collect(),
            strasse.clone(),
        ));
    }
    wege.push((
        999,
        vec![(9_000_001, 48.003, 10.805), (9_000_002, 48.004, 13.0)],
        vec![("route", "ferry"), ("motorcar", "yes")],
    ));
    wege
}

/// Auszug wie bei Geofabrik: alle Wege mit mindestens einem Knoten im Umriss (Länge von, bis), mit allen Knoten
fn auszug(wege: &[Weg], von: f64, bis: f64) -> OsmExtract {
    let mut osm = OsmExtract::default();
    for (id, knoten, tags) in wege {
        if knoten.iter().any(|&(_, _, lon)| (von..=bis).contains(&lon)) {
            for &(k, lat, lon) in knoten {
                osm.add_node(k, lat, lon);
            }
            let ids: Vec<i64> = knoten.iter().map(|k| k.0).collect();
            osm.add_way(*id, &ids, tags);
        }
    }
    osm
}

/// Alle Kacheln mit Knoten
fn mit_wegen(kacheln: BTreeMap<(u32, u32), Vec<u8>>) -> BTreeMap<(u32, u32), Vec<u8>> {
    kacheln
        .into_iter()
        .filter(|(_, k)| u32::from_le_bytes([k[20], k[21], k[22], k[23]]) > 0)
        .collect()
}

#[test]
fn regionen_bauen_dieselben_kacheln_wie_ein_stueck_jede_genau_einmal() {
    let regionen = Regionen::neu(LISTE, umriss).unwrap();
    let wege = wege();
    let ganz = auszug(&wege, -180.0, 180.0);
    let bereich = ganz.extent().unwrap();
    let erwartet = mit_wegen(build_tiles(&ganz, &bereich));

    let mut auszuege = [auszug(&wege, 9.0, 10.86), auszug(&wege, 10.84, 12.0)];
    assert!(
        auszuege[1].way_count() < ganz.way_count(),
        "die Fähre fehlt im Auszug von „ost“"
    );
    let randwege: Vec<_> = auszuege
        .iter()
        .enumerate()
        .map(|(r, osm)| osm.randwege(&regionen, r))
        .collect();
    for (r, osm) in auszuege.iter_mut().enumerate() {
        for (von, dateien) in randwege.iter().enumerate() {
            if let Some(datei) = dateien.get(&r) {
                assert_ne!(von, r);
                osm.read_randwege(datei).unwrap();
            }
        }
    }

    let mut gebaut: BTreeMap<(u32, u32), Vec<u8>> = BTreeMap::new();
    for (r, osm) in auszuege.iter().enumerate() {
        let eigene = mit_wegen(build_tiles_of(osm, &bereich, |x, y| {
            regionen.besitzer(x, y) == r
        }));
        assert!(!eigene.is_empty(), "Region {r} baut Kacheln");
        for (kachel, inhalt) in eigene {
            assert!(
                gebaut.insert(kachel, inhalt).is_none(),
                "Kachel {kachel:?} doppelt"
            );
        }
    }
    assert_eq!(
        gebaut.keys().collect::<Vec<_>>(),
        erwartet.keys().collect::<Vec<_>>()
    );
    assert!(gebaut == erwartet, "Kacheln weichen ab");
    // Die Fähre reicht bis in Kacheln weit östlich beider Umrisse; die gehören der nächsten Region.
    assert!(
        gebaut.keys().any(|&(x, _)| x > 2190),
        "Fähre östlich bei 13° O"
    );
}

#[test]
fn eine_kachel_gehoert_der_ersten_region_deren_umriss_ihre_mitte_enthaelt_sonst_der_naechsten() {
    let regionen = Regionen::neu(LISTE, umriss).unwrap();
    assert_eq!(regionen.alle()[1].name, "ost");
    assert_eq!(regionen.alle()[1].staaten, ["AT"]);
    // Mitte 10,854° O: in beiden Umrissen, also der ersten
    assert_eq!(regionen.besitzer(2171, 1422), 0);
    assert_eq!(regionen.besitzer(2172, 1422), 1);
    // weit außerhalb: die nächste
    assert_eq!(regionen.besitzer(2300, 1422), 1);
    assert_eq!(regionen.besitzer(2000, 1422), 0);
    assert_eq!(regionen.besitzer(2171, 1300), 0);
    // fester Bereich, alle Kacheln vergeben
    let bereich = TileRange {
        x0: 2160,
        x1: 2190,
        y0: 1410,
        y1: 1430,
    };
    for x in bereich.x0..=bereich.x1 {
        for y in bereich.y0..=bereich.y1 {
            assert!(regionen.besitzer(x, y) < 2);
        }
    }
}

#[test]
fn die_liste_der_regionen_wird_geprueft() {
    for (liste, fehler) in [
        ("", "keine Regionen"),
        ("west\n", "Zeile 1"),
        ("west test/west\nwest test/ost\n", "doppelt"),
        ("West test/west\n", "ungültig"),
    ] {
        let e = Regionen::neu(liste, umriss).err().unwrap();
        assert!(e.contains(fehler), "{liste:?}: {e}");
    }
    assert!(
        Regionen::neu("nord test/nord\n", umriss)
            .err()
            .unwrap()
            .contains("kein Umriss")
    );
}
