//! Prüfpunkt Ortsverzeichnis (gpxart.com #39): Aus einem winzigen OSM-Ausschnitt werden die Orte herausgezogen. Die
//! Dateien des Verzeichnisses werden hier unabhängig vom Builder nach `docs/ortsverzeichnis-format.md` gelesen.

use serde_json::Value;
use wegegraph::{Orte, Ortsverzeichnis};

#[test]
fn staedte_doerfer_und_stadtteile_kommen_unter_jedem_wortanfang_ins_verzeichnis() {
    let mut orte = Orte::default();
    orte.add_node(
        1,
        48.0478,
        10.8688,
        &[
            ("place", "town"),
            ("name", "Landsberg am Lech"),
            ("population", "29.317"),
        ],
    );
    orte.add_node(
        2,
        48.0903,
        10.8573,
        &[("place", "village"), ("name", "Kaufering")],
    );
    orte.add_node(
        3,
        48.1371,
        11.5754,
        &[
            ("place", "city"),
            ("name", "München"),
            ("name:en", "Munich"),
            ("population", "1510378"),
        ],
    );
    // nicht ins Verzeichnis: falsche Art, ohne Namen, gar kein Ort
    orte.add_node(
        4,
        48.05,
        10.88,
        &[("place", "isolated_dwelling"), ("name", "Lechhof")],
    );
    orte.add_node(5, 48.06, 10.89, &[("place", "village")]);
    orte.add_node(
        6,
        48.07,
        10.90,
        &[("amenity", "townhall"), ("name", "Lechrathaus")],
    );

    let verzeichnis = Verzeichnis::lies(&orte);

    assert_eq!(verzeichnis.anzahl, 3);
    let landsberg = verzeichnis.eintrag("la", "Landsberg am Lech");
    assert_eq!(landsberg[1], "town");
    assert_eq!(
        (landsberg[2].as_f64(), landsberg[3].as_f64()),
        (Some(10.8688), Some(48.0478))
    );
    assert_eq!(landsberg[4], 29317, "Einwohnerzahl");
    assert_eq!(
        verzeichnis.eintrag("le", "Landsberg am Lech"),
        landsberg,
        "auch unter dem Wortanfang „Lech“"
    );
    assert!(verzeichnis.teil("am").is_none(), "kurze Wörter nicht");
    assert_eq!(verzeichnis.eintrag("ka", "Kaufering")[1], "village");
    assert_eq!(
        verzeichnis.eintrag("mu", "München")[7]["en"],
        "Munich",
        "mit Namen in anderen Sprachen, und unter „mu“ für beide"
    );
    assert!(
        !verzeichnis
            .alle()
            .any(|e| e[0] == "Lechhof" || e[0] == "Lechrathaus")
    );
}

#[test]
fn gleichnamige_orte_unterscheiden_sich_durch_land_und_naechstgroessere_stadt() {
    let mut orte = Orte::default();
    // Ein Staat mit zwei Ländern, alle als Rechtecke; Bayern aus zwei Wegen, die anderen aus je einem geschlossenen.
    rechteck(
        &mut orte,
        100,
        (47.0, 9.0),
        (49.0, 12.0),
        "4",
        "Bayern",
        "Bavaria",
        "DE-BY",
        true,
    );
    rechteck(
        &mut orte,
        200,
        (49.5, 8.0),
        (51.5, 10.0),
        "4",
        "Hessen",
        "Hesse",
        "DE-HE",
        false,
    );
    rechteck(
        &mut orte,
        300,
        (47.0, 8.0),
        (52.0, 12.0),
        "2",
        "Deutschland",
        "Germany",
        "DE",
        false,
    );
    for (id, lat, lon, place, name) in [
        (1, 48.0478, 10.8688, "town", "Landsberg am Lech"),
        (2, 48.0600, 10.9000, "village", "Neustadt"),
        (3, 50.8100, 8.7700, "town", "Marburg"),
        (4, 50.8000, 8.8000, "village", "Neustadt"),
        (5, 48.5734, 7.7521, "city", "Strasbourg"), // außerhalb beider Länder
    ] {
        orte.add_node(id, lat, lon, &[("place", place), ("name", name)]);
    }

    let verzeichnis = Verzeichnis::lies(&orte);

    let land = |eintrag: &Value| verzeichnis.laender[eintrag[5].as_u64().unwrap() as usize].clone();
    let neustadt = verzeichnis.teil("ne").unwrap();
    assert_eq!(neustadt.len(), 2);
    let mut zusaetze: Vec<_> = neustadt
        .iter()
        .map(|e| (land(e)[0].clone(), e[6].clone()))
        .collect();
    zusaetze.sort_by_key(|z| z.0.to_string());
    assert_eq!(
        zusaetze,
        [
            (Value::from("Bayern"), Value::from("Landsberg am Lech")),
            (Value::from("Hessen"), Value::from("Marburg")),
        ]
    );
    assert_eq!(
        land(&verzeichnis.eintrag("ma", "Marburg")),
        serde_json::json!(["Hessen", "Hesse", "DE"])
    );
    assert!(
        verzeichnis.teil("st").is_none(),
        "außerhalb der Länder kein Ort"
    );
}

/// Zwei Regionen (gpxart.com #85) mit sich überlappenden Auszügen: Jede bringt nur die Orte ihrer Staaten mit, und aus
/// beiden Ortslisten entsteht dasselbe Verzeichnis wie aus beiden Auszügen in einem Stück.
#[test]
fn ortslisten_der_regionen_ergeben_das_verzeichnis_wie_in_einem_stueck() {
    // Auszüge, die je etwas über die Grenze am Rhein (7,8° O) reichen; Grand Est ohne den Westen, Baden-Württemberg
    // ohne den Osten, aber beide Staaten vollständig
    let land = |orte: &mut Orte| {
        rechteck(
            orte,
            100,
            (47.5, 7.8),
            (49.0, 10.0),
            "4",
            "Baden-Württemberg",
            "Baden-Württemberg",
            "DE-BW",
            false,
        );
        rechteck(
            orte,
            200,
            (47.5, 6.0),
            (49.0, 7.8),
            "4",
            "Grand Est",
            "Grand Est",
            "FR-GES",
            false,
        );
        rechteck(
            orte,
            300,
            (47.5, 7.8),
            (49.0, 10.0),
            "2",
            "Deutschland",
            "Germany",
            "DE",
            false,
        );
        rechteck(
            orte,
            400,
            (47.5, 6.0),
            (49.0, 7.8),
            "2",
            "Frankreich",
            "France",
            "FR",
            false,
        );
    };
    let ort =
        |orte: &mut Orte, id, lat, lon, place, name, namen: &[(&'static str, &'static str)]| {
            let mut tags = vec![("place", place), ("name", name)];
            tags.extend(namen);
            orte.add_node(id, lat, lon, &tags);
        };
    let strassburg = |orte: &mut Orte| {
        ort(
            orte,
            1,
            48.5734,
            7.7521,
            "city",
            "Strasbourg",
            &[("name:de", "Straßburg"), ("population", "291313")],
        )
    };
    let kehl = |orte: &mut Orte| ort(orte, 2, 48.5863, 7.8250, "suburb", "Kehl-Sundheim", &[]);
    let mut west = Orte::fuer_staaten(&["FR".to_string()]);
    let mut ost = Orte::fuer_staaten(&["DE".to_string()]);
    let mut ganz = Orte::default();
    for orte in [&mut west, &mut ost, &mut ganz] {
        land(orte);
        strassburg(orte);
        kehl(orte);
    }
    ort(
        &mut west,
        3,
        48.4,
        7.5,
        "village",
        "Dorf\tmit Tabulator \\ und Schrägstrich",
        &[],
    );
    ort(
        &mut ganz,
        3,
        48.4,
        7.5,
        "village",
        "Dorf\tmit Tabulator \\ und Schrägstrich",
        &[],
    );
    ort(
        &mut ost,
        4,
        48.4,
        8.0,
        "village",
        "Neuried",
        &[("name:en", "New Ried")],
    );
    ort(
        &mut ganz,
        4,
        48.4,
        8.0,
        "village",
        "Neuried",
        &[("name:en", "New Ried")],
    );

    let (liste_west, anzahl_west, _) = west.ortsliste();
    let (liste_ost, anzahl_ost, _) = ost.ortsliste();
    assert_eq!(
        (anzahl_west, anzahl_ost),
        (2, 2),
        "jede Region nur mit den Orten ihres Staates"
    );
    let verzeichnis = Ortsverzeichnis::aus_listen(&[liste_west, liste_ost]).unwrap();
    assert_eq!(verzeichnis.orte, 4);
    assert_eq!(verzeichnis.dateien, ganz.verzeichnis(&[]).dateien);
    // Ein Stadtteil von Kehl nennt die nahe Großstadt jenseits der Grenze
    let kehl: Value = serde_json::from_str(&verzeichnis.dateien["ke.json"]).unwrap();
    assert_eq!(kehl[0][6], "Strasbourg");
}

#[test]
fn ein_zu_grosser_teil_wird_nach_dem_naechsten_buchstaben_des_namens_geteilt() {
    let mut orte = Orte::default();
    let mut id = 0;
    let mut ort = |name: &str| {
        id += 1;
        orte.add_node(id, 47.0, 2.0, &[("place", "village"), ("name", name)]);
    };
    // So viele Dörfer „Le …“, dass „le“ über 300 kB groß wird, wie mit Frankreich (gpxart.com #90)
    for i in 0..8000u32 {
        let b = |n: u32| char::from(b'a' + (n % 26) as u8);
        ort(&format!("Le {}{}{}ville", b(i), b(i / 26), b(i / 676)));
    }
    for name in [
        "Le Mans",
        "Lehrberg",
        "La Rochelle",
        "Les Herbiers",
        "L'Isle-Adam",
        "Landsberg",
    ] {
        ort(name);
    }
    let dateien = orte.verzeichnis(&[]).dateien;
    let index: Value = serde_json::from_str(&dateien["index.json"]).unwrap();
    assert_eq!(index["geteilt"], serde_json::json!(["le"]));
    let teile: Vec<&str> = index["teile"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t.as_str().unwrap())
        .collect();
    assert!(!teile.contains(&"le"), "kein Ort heißt nur „Le“");
    for t in &teile {
        let groesse = dateien[&format!("{t}.json")].len();
        assert!(groesse <= 300_000, "{t}.json hat {groesse} Bytes");
    }

    let verzeichnis = Verzeichnis::lies(&orte);
    // Teil nach dem Rest des Namens ab dem Wort ohne Zwischenraum: „le m…“ → „lem“
    for (teil, name) in [
        ("lem", "Le Mans"),
        ("ma", "Le Mans"),
        ("leh", "Lehrberg"),
        ("les", "Les Herbiers"),
        ("he", "Les Herbiers"),
        ("la", "La Rochelle"),
        ("la", "Landsberg"),
        ("l", "L'Isle-Adam"),
        ("is", "L'Isle-Adam"),
    ] {
        verzeichnis.eintrag(teil, name);
    }
}

/// Ein Land (`admin_level=4`) oder Staat (`2`) als Rechteck (Süd-West, Nord-Ost), als Grenz-Relation
/// `boundary=administrative`.
#[allow(clippy::too_many_arguments)]
fn rechteck(
    orte: &mut Orte,
    id: i64,
    (s, w): (f64, f64),
    (n, o): (f64, f64),
    admin_level: &str,
    de: &str,
    en: &str,
    iso: &str,
    zwei_wege: bool,
) {
    for (i, (lat, lon)) in [(s, w), (s, o), (n, o), (n, w)].into_iter().enumerate() {
        orte.add_node(id + i as i64, lat, lon, &[]);
    }
    let wege = if zwei_wege {
        orte.add_way(id, &[id, id + 1, id + 2]);
        orte.add_way(id + 1, &[id + 2, id + 3, id]);
        vec![id, id + 1]
    } else {
        orte.add_way(id, &[id, id + 1, id + 2, id + 3, id]);
        vec![id]
    };
    let tags = [
        ("type", "boundary"),
        ("boundary", "administrative"),
        ("admin_level", admin_level),
        ("name", de),
        ("name:en", en),
        (
            if admin_level == "2" {
                "ISO3166-1"
            } else {
                "ISO3166-2"
            },
            iso,
        ),
    ];
    orte.add_relation(id, &wege, &tags);
}

// ---------------------------------------------------------------- Verzeichnis lesen (nach Format-Dokument)

struct Verzeichnis {
    anzahl: u64,
    laender: Vec<Value>,
    teile: Vec<(String, Vec<Value>)>,
}

impl Verzeichnis {
    fn lies(orte: &Orte) -> Verzeichnis {
        let dateien = orte.verzeichnis(&[]).dateien;
        let index: Value = serde_json::from_str(&dateien["index.json"]).unwrap();
        assert_eq!(index["format"], 2);
        let teile = index["teile"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| {
                let t = t.as_str().unwrap().to_string();
                let eintraege: Value =
                    serde_json::from_str(&dateien[&format!("{t}.json")]).unwrap();
                (t, eintraege.as_array().unwrap().clone())
            })
            .collect();
        Verzeichnis {
            anzahl: index["orte"].as_u64().unwrap(),
            laender: index["laender"].as_array().unwrap().clone(),
            teile,
        }
    }

    fn teil(&self, schluessel: &str) -> Option<&Vec<Value>> {
        self.teile
            .iter()
            .find(|(t, _)| t == schluessel)
            .map(|(_, e)| e)
    }

    fn eintrag(&self, schluessel: &str, name: &str) -> Value {
        self.teil(schluessel)
            .and_then(|e| e.iter().find(|e| e[0] == name))
            .unwrap_or_else(|| panic!("{name} fehlt in {schluessel}.json"))
            .clone()
    }

    fn alle(&self) -> impl Iterator<Item = &Value> {
        self.teile.iter().flat_map(|(_, e)| e)
    }
}
