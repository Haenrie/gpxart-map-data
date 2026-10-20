//! Prüfpunkt Ortsverzeichnis (gpxart.com #39): Aus einem winzigen OSM-Ausschnitt werden die Orte herausgezogen. Die
//! Dateien des Verzeichnisses werden hier unabhängig vom Builder nach `docs/ortsverzeichnis-format.md` gelesen.

use serde_json::Value;
use wegegraph::Orte;

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
        assert_eq!(index["format"], 1);
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
