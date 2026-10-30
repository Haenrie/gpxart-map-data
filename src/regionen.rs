//! Regionen (gpxart.com #85): Der Builder baut jede Region aus ihren eigenen Geofabrik-Auszügen, damit Speicher und
//! Platte eines Rechners bis zur ganzen Welt reichen. Jede z12-Kachel gehört genau einer Region, und nur die baut sie.
//!
//! An einer Grenze reicht kein Auszug: Geofabrik schneidet entlang eines Umrisses (`.poly`), der nur gut 1 km über die
//! Landesgrenze hinausgeht, eine Kachel ist 6–7 km breit. Darum gibt jede Region der Besitzerin jeder fremden Kachel
//! die eigenen Wege mit, die diese Kachel berühren (Randwege, `randwege.rs`). Die Besitzerin baut die Kachel aus ihrem
//! Auszug und allen Randwegen dafür, also aus allen Wegen, die die Kachel berühren: Die Kacheln sind dieselben, als
//! wären alle Auszüge zusammen in einem Stück gebaut.
//!
//! Die Liste der Regionen steht in `regionen.txt` (im Repo-Wurzelordner), die Umrisse kommen von Geofabrik. Eine
//! Region besteht aus einem oder mehreren Auszügen (ein Land, ein Sammelauszug wie `dach` oder kleine Nachbarländer
//! zusammen), ihr Umriss ist die Vereinigung der Umrisse ihrer Auszüge (gpxart.com ADR 0003).

use std::path::Path;

use crate::flaeche::Flaeche;
use crate::tiling;

/// Eine Region: Name (für Dateien und Release), Geofabrik-Auszüge und die Staaten, deren Orte sie ins
/// Ortsverzeichnis bringt.
pub struct Region {
    pub name: String,
    /// Pfade bei Geofabrik, z. B. `europe/dach`
    pub auszuege: Vec<String>,
    /// ISO 3166-1, z. B. `DE`
    pub staaten: Vec<String>,
}

/// Die Regionen in der Reihenfolge der Liste, mit den Umrissen ihrer Auszüge
pub struct Regionen {
    regionen: Vec<Region>,
    umrisse: Vec<Vec<Flaeche>>,
}

/// Höchstens dieser Anteil einer Region darf in anderen liegen: Geofabriks Umrisse überlappen sich nur in einem Saum
/// von gut 1 km. Gemessen an allen Ländern Europas (Oktober 2026) sind das höchstens 7 %, bei Kleinstaaten 14–26 %
/// (Andorra, Liechtenstein, Guernsey und Jersey). Mehr heißt, eine Region steckt in anderen und ihre Wege würden doppelt
/// gelesen: Isle of Man 62 %, Azoren 97 % (im Umriss von Portugal), Monaco 100 %, `alps` neben `dach` 100 %.
const UEBERLAPPUNG: f64 = 0.5;

/// Stichproben je Richtung über den Rahmen einer Region für die Prüfung der Überlappung
const PROBEN: usize = 100;

impl Regionen {
    /// Liest die Liste und je Auszug den Umriss `<umrisse>/<Auszug>.poly`, z. B. `umrisse/europe/dach.poly`.
    pub fn lies(liste: &Path, umrisse: &Path) -> Result<Regionen, String> {
        let text =
            std::fs::read_to_string(liste).map_err(|e| format!("{}: {e}", liste.display()))?;
        Regionen::neu(&text, |auszug| {
            let pfad = umrisse.join(format!("{auszug}.poly"));
            std::fs::read_to_string(&pfad).map_err(|e| format!("{}: {e}", pfad.display()))
        })
    }

    /// Nur die Liste, ohne Umrisse (für das Ortsverzeichnis)
    pub fn lies_liste(liste: &Path) -> Result<Vec<Region>, String> {
        let text =
            std::fs::read_to_string(liste).map_err(|e| format!("{}: {e}", liste.display()))?;
        liste_lesen(&text)
    }

    /// Regionen aus dem Text der Liste; `umriss` liefert zu jedem Auszug den Text des Umrisses (Polygon-Format von
    /// Osmosis, wie Geofabrik es als `.poly` anbietet). Steckt eine Region zu mehr als der Hälfte in anderen, ist das
    /// ein Fehler.
    pub fn neu(
        liste: &str,
        umriss: impl Fn(&str) -> Result<String, String>,
    ) -> Result<Regionen, String> {
        let regionen = liste_lesen(liste)?;
        let umrisse = regionen
            .iter()
            .map(|r| {
                r.auszuege
                    .iter()
                    .map(|a| poly(&umriss(a)?).map_err(|e| format!("Umriss {a}: {e}")))
                    .collect::<Result<_, _>>()
            })
            .collect::<Result<_, _>>()?;
        let regionen = Regionen { regionen, umrisse };
        regionen.ueberlappung_pruefen()?;
        Ok(regionen)
    }

    /// Fehler, wenn eine Region zu mehr als `UEBERLAPPUNG` in anderen liegt
    fn ueberlappung_pruefen(&self) -> Result<(), String> {
        for (i, region) in self.regionen.iter().enumerate() {
            let (anteil, andere) = self.ueberlappung(i);
            if anteil > UEBERLAPPUNG {
                return Err(format!(
                    "Region {} liegt zu {:.0} % in {}; Regionen dürfen sich nur am Rand überlappen",
                    region.name,
                    100.0 * anteil,
                    andere.join(", ")
                ));
            }
        }
        Ok(())
    }

    /// Welcher Anteil der Region Nummer `i` in anderen Regionen liegt, und in welchen (Stichproben auf einem Gitter
    /// über ihren Rahmen)
    pub fn ueberlappung(&self, i: usize) -> (f64, Vec<&str>) {
        let umrisse = &self.umrisse[i];
        let (sued, west, nord, ost) = umrisse.iter().map(Flaeche::rahmen).fold(
            (
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            ),
            |a, r| (a.0.min(r.0), a.1.min(r.1), a.2.max(r.2), a.3.max(r.3)),
        );
        let (mut innen, mut fremd) = (0, 0);
        let mut andere = vec![false; self.umrisse.len()];
        for s in 0..PROBEN {
            for w in 0..PROBEN {
                let lat = sued + (nord - sued) * (s as f64 + 0.5) / PROBEN as f64;
                let lon = west + (ost - west) * (w as f64 + 0.5) / PROBEN as f64;
                if !umrisse.iter().any(|u| u.enthaelt(lat, lon)) {
                    continue;
                }
                innen += 1;
                let mut drin = false;
                for (j, u) in self.umrisse.iter().enumerate() {
                    if j != i && u.iter().any(|u| u.enthaelt(lat, lon)) {
                        (drin, andere[j]) = (true, true);
                    }
                }
                fremd += drin as usize;
            }
        }
        let namen = (0..andere.len())
            .filter(|&j| andere[j])
            .map(|j| self.regionen[j].name.as_str())
            .collect();
        (fremd as f64 / innen.max(1) as f64, namen)
    }

    pub fn alle(&self) -> &[Region] {
        &self.regionen
    }

    /// Nummer der Region mit diesem Namen
    pub fn nummer(&self, name: &str) -> Option<usize> {
        self.regionen.iter().position(|r| r.name == name)
    }

    /// Die Region, der die z12-Kachel gehört: die erste der Liste, deren Umriss die Mitte der Kachel enthält (die
    /// Umrisse überlappen sich an den Grenzen um gut 1 km), sonst die mit dem nächsten Umriss (Meer, Fährlinien).
    pub fn besitzer(&self, x: u32, y: u32) -> usize {
        let (lat, lon) = tiling::mitte(x, y);
        if let Some(i) = self
            .umrisse
            .iter()
            .position(|u| u.iter().any(|u| u.enthaelt(lat, lon)))
        {
            return i;
        }
        self.umrisse
            .iter()
            .map(|u| {
                u.iter()
                    .map(|u| u.abstand(lat, lon))
                    .fold(f64::INFINITY, f64::min)
            })
            .enumerate()
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map_or(0, |(i, _)| i)
    }
}

/// Zeilen der Liste: Name, Geofabrik-Auszüge (mit Komma getrennt), Staaten; `#` leitet einen Kommentar ein.
fn liste_lesen(text: &str) -> Result<Vec<Region>, String> {
    let mut regionen: Vec<Region> = Vec::new();
    for (nr, zeile) in text.lines().enumerate() {
        let zeile = zeile.split('#').next().unwrap_or("");
        let mut felder = zeile.split_whitespace();
        let (Some(name), Some(auszug)) = (felder.next(), felder.next()) else {
            if !zeile.trim().is_empty() {
                return Err(format!("Zeile {}: Name und Auszug fehlen", nr + 1));
            }
            continue;
        };
        let gueltig = |s: &str, extra: &[char]| {
            !s.is_empty()
                && s.chars().all(|c| {
                    c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || extra.contains(&c)
                })
        };
        let auszuege: Vec<String> = auszug.split(',').map(String::from).collect();
        if !gueltig(name, &[]) || !auszuege.iter().all(|a| gueltig(a, &['/'])) {
            return Err(format!("Zeile {}: ungültiger Name oder Auszug", nr + 1));
        }
        if regionen.iter().any(|r| r.name == name) {
            return Err(format!("Zeile {}: Region {name} doppelt", nr + 1));
        }
        for (i, a) in auszuege.iter().enumerate() {
            if auszuege[..i].contains(a) || regionen.iter().any(|r| r.auszuege.contains(a)) {
                return Err(format!("Zeile {}: Auszug {a} doppelt", nr + 1));
            }
        }
        regionen.push(Region {
            name: name.to_string(),
            auszuege,
            staaten: felder.map(String::from).collect(),
        });
    }
    if regionen.is_empty() {
        return Err("keine Regionen".into());
    }
    Ok(regionen)
}

/// Umriss im Polygon-Format von Osmosis: erste Zeile Name, dann Ringe (Name, Zeilen „Länge Breite“, `END`; Name mit
/// `!` für ein Loch), am Schluss `END`. Löcher zählen bei der Strahlprobe von selbst.
fn poly(text: &str) -> Result<Flaeche, String> {
    let mut kanten = Vec::new();
    let mut ring: Option<Vec<(f64, f64)>> = None;
    for zeile in text.lines().skip(1) {
        let zeile = zeile.trim();
        if zeile.is_empty() {
            continue;
        }
        if zeile == "END" {
            if let Some(punkte) = ring.take() {
                for (i, &a) in punkte.iter().enumerate() {
                    kanten.push((a, punkte[(i + 1) % punkte.len()]));
                }
            }
            continue;
        }
        match &mut ring {
            None => ring = Some(Vec::new()),
            Some(punkte) => {
                let mut zahlen = zeile.split_whitespace().map(str::parse::<f64>);
                let (Some(Ok(lon)), Some(Ok(lat)), None) =
                    (zahlen.next(), zahlen.next(), zahlen.next())
                else {
                    return Err(format!("unerwartete Zeile „{zeile}“"));
                };
                punkte.push((lat, lon));
            }
        }
    }
    if kanten.is_empty() {
        return Err("keine Ringe".into());
    }
    Ok(Flaeche::neu(kanten))
}
