//! Regionen (gpxart.com #85): Der Builder baut jede Region aus ihrem eigenen Geofabrik-Auszug, damit Speicher und
//! Platte eines Rechners bis zur ganzen Welt reichen. Jede z12-Kachel gehört genau einer Region, und nur die baut sie.
//!
//! An einer Grenze reicht kein Auszug: Geofabrik schneidet entlang eines Umrisses (`.poly`), der nur gut 1 km über die
//! Landesgrenze hinausgeht, eine Kachel ist 6–7 km breit. Darum gibt jede Region der Besitzerin jeder fremden Kachel
//! die eigenen Wege mit, die diese Kachel berühren (Randwege, `randwege.rs`). Die Besitzerin baut die Kachel aus ihrem
//! Auszug und allen Randwegen dafür, also aus allen Wegen, die die Kachel berühren: Die Kacheln sind dieselben, als
//! wären alle Auszüge zusammen in einem Stück gebaut.
//!
//! Die Liste der Regionen steht in `regionen.txt` (im Repo-Wurzelordner), die Umrisse kommen von Geofabrik.

use std::path::Path;

use crate::flaeche::Flaeche;
use crate::tiling;

/// Eine Region: Name (für Dateien und Release), Geofabrik-Auszug und die Staaten, deren Orte sie ins Ortsverzeichnis
/// bringt.
pub struct Region {
    pub name: String,
    /// Pfad bei Geofabrik, z. B. `europe/dach`
    pub auszug: String,
    /// ISO 3166-1, z. B. `DE`
    pub staaten: Vec<String>,
}

/// Die Regionen in der Reihenfolge der Liste, mit ihren Umrissen
pub struct Regionen {
    regionen: Vec<Region>,
    umrisse: Vec<Flaeche>,
}

impl Regionen {
    /// Liest die Liste und je Region den Umriss `<umrisse>/<name>.poly`.
    pub fn lies(liste: &Path, umrisse: &Path) -> Result<Regionen, String> {
        let text =
            std::fs::read_to_string(liste).map_err(|e| format!("{}: {e}", liste.display()))?;
        Regionen::neu(&text, |name| {
            let pfad = umrisse.join(format!("{name}.poly"));
            std::fs::read_to_string(&pfad).map_err(|e| format!("{}: {e}", pfad.display()))
        })
    }

    /// Nur die Liste, ohne Umrisse (für das Ortsverzeichnis)
    pub fn lies_liste(liste: &Path) -> Result<Vec<Region>, String> {
        let text =
            std::fs::read_to_string(liste).map_err(|e| format!("{}: {e}", liste.display()))?;
        liste_lesen(&text)
    }

    /// Regionen aus dem Text der Liste; `umriss` liefert zu jedem Namen den Text des Umrisses (Polygon-Format von
    /// Osmosis, wie Geofabrik es als `.poly` anbietet).
    pub fn neu(
        liste: &str,
        umriss: impl Fn(&str) -> Result<String, String>,
    ) -> Result<Regionen, String> {
        let regionen = liste_lesen(liste)?;
        let umrisse = regionen
            .iter()
            .map(|r| poly(&umriss(&r.name)?).map_err(|e| format!("Umriss {}: {e}", r.name)))
            .collect::<Result<_, _>>()?;
        Ok(Regionen { regionen, umrisse })
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
        if let Some(i) = self.umrisse.iter().position(|u| u.enthaelt(lat, lon)) {
            return i;
        }
        self.umrisse
            .iter()
            .map(|u| u.abstand(lat, lon))
            .enumerate()
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map_or(0, |(i, _)| i)
    }
}

/// Zeilen der Liste: Name, Geofabrik-Auszug, Staaten; `#` leitet einen Kommentar ein.
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
        if !gueltig(name, &[]) || !gueltig(auszug, &['/']) {
            return Err(format!("Zeile {}: ungültiger Name oder Auszug", nr + 1));
        }
        if regionen.iter().any(|r| r.name == name) {
            return Err(format!("Zeile {}: Region {name} doppelt", nr + 1));
        }
        regionen.push(Region {
            name: name.to_string(),
            auszug: auszug.to_string(),
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
