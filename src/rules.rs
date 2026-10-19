//! Regeln aus gpxart.com #11, angewandt auf OSM-Wege: je Fortbewegungsart und Richtung die Stufe und die Arten der
//! Hinweise. Die Tabelle mit den OSM-Näherungen steht in `docs/wegegraph-format.md` (Abschnitt „Inhalt“).

/// Tags eines OSM-Weges (es reichen die Schlüssel aus `KEYS`).
pub(crate) struct Tags<'a>(pub(crate) Vec<(&'a str, &'a str)>);

impl Tags<'_> {
    fn tag(&self, key: &str) -> Option<&str> {
        self.0.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
    }
}

/// Tags, die die Regeln lesen. Der PBF-Leser behält nur diese.
pub(crate) const KEYS: &[&str] = &[
    "highway",
    "route",
    "area",
    "motorroad",
    "access",
    "foot",
    "bicycle",
    "vehicle",
    "motor_vehicle",
    "motorcar",
    "oneway",
    "oneway:bicycle",
    "junction",
    "cycleway",
    "cycleway:both",
    "cycleway:left",
    "cycleway:right",
    "surface",
    "tracktype",
    "sidewalk",
    "sidewalk:both",
    "sidewalk:left",
    "sidewalk:right",
    "maxspeed",
    "source:maxspeed",
    "zone:traffic",
    "maxspeed:type",
    "toll",
    "sac_scale",
    "mtb:scale",
    "opening_hours",
];

/// Fortbewegungsarten mit Wegegraph, in der Reihenfolge der Stellen-Abschnitte.
pub(crate) const ABSCHNITTE: [&[u8; 4]; 4] = [b"FUSS", b"RENN", b"GRAV", b"AUTO"];

const HINWEIS: u32 = 1;
const GEMIEDEN: u32 = 2;
const GESPERRT: u32 = 3;

/// Arten der Hinweise; die Nummer ist das Bit im Format.
#[derive(Clone, Copy)]
enum Art {
    Privatweg = 0,
    FussgaengerVerboten = 1,
    Oeffnungszeiten = 2,
    OhneGehweg = 3,
    Faehre = 4,
    Einbahnstrasse = 5,
    Unbefestigt = 6,
    StarkBefahren = 7,
    RadfahrenVerboten = 8,
    Treppe = 9,
    Schiebestrecke = 10,
    Kopfsteinpflaster = 11,
    SchwierigerTrail = 12,
    AlpinerPfad = 13,
    Autobahn = 14,
    Mautstrasse = 15,
    Verkehrsberuhigt = 16,
}

/// Wert einer Richtung nach dem Format: Bits 0–1 Stufe, Bit 2 + i Art i.
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub(crate) struct Wert(pub(crate) u32);

impl Wert {
    fn stufe(self) -> u32 {
        self.0 & 3
    }

    /// Hebt die Stufe auf mindestens `stufe` und vermerkt die Art.
    fn mit(self, stufe: u32, art: Art) -> Wert {
        Wert(self.stufe().max(stufe) | (self.0 & !3) | (1 << (2 + art as u32)))
    }

    fn gesperrt(self) -> Wert {
        Wert(self.0 | GESPERRT)
    }
}

/// Stellen einer Kante für eine Fortbewegungsart: (in Wegrichtung, dagegen) bzw. nach dem Schreiben in
/// Kantenrichtung und Gegenrichtung.
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub(crate) struct Klasse {
    pub(crate) hin: Wert,
    pub(crate) zurueck: Wert,
}

impl Klasse {
    pub(crate) fn gedreht(self) -> Klasse {
        Klasse {
            hin: self.zurueck,
            zurueck: self.hin,
        }
    }

    fn gesperrt(self) -> bool {
        self.hin.stufe() == GESPERRT && self.zurueck.stufe() == GESPERRT
    }
}

/// Stellen je Fortbewegungsart (Reihenfolge wie `ABSCHNITTE`), bezogen auf die Richtung des OSM-Weges.
pub(crate) type Stellen = [Klasse; 4];

/// Stellen des Weges, oder `None`, wenn er kein Weg ist oder für alle Fortbewegungsarten gesperrt.
pub(crate) fn stellen(way: &Tags) -> Option<Stellen> {
    let highway = way.tag("highway").unwrap_or("");
    let faehre = way.tag("route") == Some("ferry");
    if (highway.is_empty() && !faehre) || kein_weg(highway) || way.tag("area") == Some("yes") {
        return None;
    }
    let w = Merkmale {
        way,
        highway,
        faehre,
    };
    let stellen = [zu_fuss(&w), rad(&w, false), rad(&w, true), auto(&w)];
    (!stellen.iter().all(|k| k.gesperrt())).then_some(stellen)
}

/// `highway`-Werte, die keine Wege sind (wie im Prototyp gpxart.com #15).
fn kein_weg(highway: &str) -> bool {
    matches!(
        highway,
        "construction"
            | "proposed"
            | "abandoned"
            | "razed"
            | "raceway"
            | "bus_guideway"
            | "platform"
            | "bus_stop"
            | "services"
            | "rest_area"
            | "elevator"
            | "emergency_bay"
            | "no"
    )
}

/// Ein OSM-Weg, wie ihn die Regeln lesen: seine Tags, dazu `highway` und ob er eine Fähre ist.
struct Merkmale<'a> {
    way: &'a Tags<'a>,
    highway: &'a str,
    faehre: bool,
}

impl Merkmale<'_> {
    fn tag(&self, key: &str) -> Option<&str> {
        self.way.tag(key)
    }

    fn ist(&self, key: &str, werte: &[&str]) -> bool {
        self.tag(key).is_some_and(|v| werte.contains(&v))
    }

    fn highway_in(&self, werte: &[&str]) -> bool {
        werte.contains(&self.highway)
    }

    /// Zugang: der erste vorhandene der Schlüssel, sonst `access`.
    fn zugang(&self, schluessel: &[&str]) -> Option<&str> {
        schluessel
            .iter()
            .chain(&["access"])
            .find_map(|k| self.tag(k))
    }

    fn autobahn(&self) -> bool {
        self.highway_in(&["motorway", "motorway_link"])
    }

    fn kraftfahrstrasse(&self) -> bool {
        self.tag("motorroad") == Some("yes")
    }

    /// Einbahnstraße: `Some(true)` in Wegrichtung, `Some(false)` dagegen.
    fn einbahn(&self) -> Option<bool> {
        match self.tag("oneway") {
            Some("yes" | "true" | "1") => Some(true),
            Some("-1" | "reverse") => Some(false),
            Some(_) => None,
            None => (self.ist("junction", &["roundabout", "circular"])
                || self.highway == "motorway")
                .then_some(true),
        }
    }

    /// Einbahnstraße gilt nicht für Rad (`oneway:bicycle=no`, Radfahren gegen die Richtung erlaubt).
    fn rad_frei_gegen_einbahn(&self) -> bool {
        self.tag("oneway:bicycle") == Some("no")
            || [
                "cycleway",
                "cycleway:both",
                "cycleway:left",
                "cycleway:right",
            ]
            .iter()
            .any(|k| self.tag(k).is_some_and(|v| v.starts_with("opposite")))
    }

    fn landstrasse_ohne_gehweg(&self) -> bool {
        const LANDSTRASSE: &[&str] = &[
            "trunk",
            "trunk_link",
            "primary",
            "primary_link",
            "secondary",
            "secondary_link",
            "tertiary",
            "tertiary_link",
        ];
        const GEHWEG: &[&str] = &["yes", "both", "left", "right", "separate"];
        let gehweg = [
            "sidewalk",
            "sidewalk:both",
            "sidewalk:left",
            "sidewalk:right",
        ]
        .iter()
        .any(|k| self.ist(k, GEHWEG));
        let innerorts = self
            .tag("maxspeed")
            .and_then(|v| v.trim().parse::<u32>().ok())
            .is_some_and(|v| v <= 50)
            || ["source:maxspeed", "zone:traffic", "maxspeed:type"]
                .iter()
                .any(|k| self.tag(k).is_some_and(|v| v.contains("urban")));
        self.highway_in(LANDSTRASSE) && !gehweg && !innerorts
    }

    fn stark_befahren(&self) -> bool {
        self.highway_in(&["trunk", "trunk_link", "primary", "primary_link"])
    }

    fn unbefestigt(&self) -> bool {
        match self.tag("surface") {
            Some(s) => matches!(
                s,
                "unpaved"
                    | "compacted"
                    | "fine_gravel"
                    | "gravel"
                    | "pebblestone"
                    | "rock"
                    | "ground"
                    | "dirt"
                    | "earth"
                    | "grass"
                    | "grass_paver"
                    | "mud"
                    | "sand"
                    | "woodchips"
            ),
            None => match self.highway {
                "track" => self.tag("tracktype") != Some("grade1"),
                "path" | "bridleway" => true,
                _ => false,
            },
        }
    }

    fn kopfsteinpflaster(&self) -> bool {
        self.ist(
            "surface",
            &[
                "sett",
                "cobblestone",
                "unhewn_cobblestone",
                "cobblestone:flattened",
            ],
        )
    }

    fn sac(&self, werte: &[&str]) -> bool {
        self.ist("sac_scale", werte)
    }

    /// `mtb:scale` als Zahl (z. B. „2+“ → 2)
    fn mtb_scale(&self) -> Option<u32> {
        self.tag("mtb:scale")?.chars().next()?.to_digit(10)
    }
}

/// Wendet die Einbahnstraße auf die Gegenrichtung an.
fn mit_einbahn(beide: Wert, einbahn: Option<bool>, gegen: impl Fn(Wert) -> Wert) -> Klasse {
    match einbahn {
        Some(true) => Klasse {
            hin: beide,
            zurueck: gegen(beide),
        },
        Some(false) => Klasse {
            hin: gegen(beide),
            zurueck: beide,
        },
        None => Klasse {
            hin: beide,
            zurueck: beide,
        },
    }
}

fn zu_fuss(w: &Merkmale) -> Klasse {
    let mut v = Wert::default();
    if w.autobahn() || w.kraftfahrstrasse() {
        v = v.gesperrt();
    }
    match w.zugang(&["foot"]) {
        Some("no") => v = v.mit(HINWEIS, Art::FussgaengerVerboten),
        Some("private") => v = v.mit(HINWEIS, Art::Privatweg),
        _ => {}
    }
    if w.tag("opening_hours").is_some() {
        v = v.mit(HINWEIS, Art::Oeffnungszeiten);
    }
    if w.landstrasse_ohne_gehweg() {
        v = v.mit(GEMIEDEN, Art::OhneGehweg);
    }
    if w.faehre {
        v = v.mit(GEMIEDEN, Art::Faehre);
    }
    Klasse { hin: v, zurueck: v }
}

/// Rennrad (`gravel = false`) und Gravel/MTB (`gravel = true`): zwei Regelwerke mit gemeinsamem Kern.
fn rad(w: &Merkmale, gravel: bool) -> Klasse {
    let mut v = Wert::default();
    if w.autobahn() || w.kraftfahrstrasse() {
        v = v.gesperrt();
    }
    let rad_frei = w.ist("bicycle", &["yes", "designated", "permissive"]);
    match w.zugang(&["bicycle", "vehicle"]) {
        Some("no") => v = v.mit(HINWEIS, Art::RadfahrenVerboten),
        Some("private") => v = v.mit(HINWEIS, Art::Privatweg),
        Some("dismount") => v = v.mit(HINWEIS, Art::Schiebestrecke),
        _ => {}
    }
    if w.highway_in(&["footway", "pedestrian"]) && !rad_frei {
        v = v.mit(HINWEIS, Art::Schiebestrecke);
    }
    // Ergänzung zu gpxart.com #11 (Entscheidung im Review zu gpxart.com #18), wie die Fähre unten
    if w.highway == "bridleway" && !rad_frei {
        v = v.mit(HINWEIS, Art::RadfahrenVerboten);
    }
    if w.highway == "steps" {
        v = v.mit(HINWEIS, Art::Treppe);
    }
    if w.stark_befahren() {
        v = v.mit(GEMIEDEN, Art::StarkBefahren);
    }
    // Ergänzung zu gpxart.com #11 (Entscheidung im Review zu gpxart.com #18): Fähre auch beim Rad gemieden,
    // wie zu Fuß
    if w.faehre {
        v = v.mit(GEMIEDEN, Art::Faehre);
    }
    if gravel {
        if w.sac(&[
            "alpine_hiking",
            "demanding_alpine_hiking",
            "difficult_alpine_hiking",
        ]) || w.mtb_scale().is_some_and(|s| s >= 4)
        {
            v = v.mit(GEMIEDEN, Art::AlpinerPfad);
        } else if w.sac(&["mountain_hiking", "demanding_mountain_hiking"])
            || w.mtb_scale().is_some_and(|s| (2..=3).contains(&s))
        {
            v = v.mit(HINWEIS, Art::SchwierigerTrail);
        }
    } else {
        if w.unbefestigt() {
            v = v.mit(GEMIEDEN, Art::Unbefestigt);
        }
        if w.kopfsteinpflaster() {
            v = v.mit(HINWEIS, Art::Kopfsteinpflaster);
        }
    }
    let einbahn = if w.rad_frei_gegen_einbahn() {
        None
    } else {
        w.einbahn()
    };
    mit_einbahn(v, einbahn, |v| v.mit(GEMIEDEN, Art::Einbahnstrasse))
}

fn auto(w: &Merkmale) -> Klasse {
    const KFZ: &[&str] = &[
        "motorway",
        "motorway_link",
        "trunk",
        "trunk_link",
        "primary",
        "primary_link",
        "secondary",
        "secondary_link",
        "tertiary",
        "tertiary_link",
        "unclassified",
        "residential",
        "living_street",
        "service",
        "road",
    ];
    const FREI: &[&str] = &["yes", "permissive", "designated"];
    let mut v = Wert::default();
    let kfz_frei = w.ist("motorcar", FREI) || w.ist("motor_vehicle", FREI);
    if !(kfz_frei || (!w.faehre && w.highway_in(KFZ))) {
        v = v.gesperrt();
    }
    if w.zugang(&["motorcar", "motor_vehicle", "vehicle"])
        .is_some_and(|z| !FREI.contains(&z))
    {
        v = v.gesperrt();
    }
    if w.autobahn() {
        v = v.mit(GEMIEDEN, Art::Autobahn);
    }
    if w.tag("toll") == Some("yes") {
        v = v.mit(GEMIEDEN, Art::Mautstrasse);
    }
    if w.faehre {
        v = v.mit(GEMIEDEN, Art::Faehre);
    }
    if w.highway == "living_street" {
        v = v.mit(HINWEIS, Art::Verkehrsberuhigt);
    }
    mit_einbahn(v, w.einbahn(), |v| v.mit(GESPERRT, Art::Einbahnstrasse))
}
