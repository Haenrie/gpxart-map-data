//! Fläche innerhalb eines Umrisses aus Kanten (Breite, Länge) für die Strahlprobe: Grenzen von Ländern im
//! Ortsverzeichnis (`orte`) und Umrisse der Regionen (`regionen`).

/// Breite der Streifen, in die die Kanten für die Strahlprobe geteilt sind (Grad)
const STREIFEN: f64 = 0.02;

/// Kante eines Umrisses: (Breite, Länge) von Anfang und Ende
pub(crate) type Kante = ((f64, f64), (f64, f64));

/// Fläche innerhalb eines Umrisses für die Strahlprobe: alle Kanten des Umrisses, nach Breitenstreifen sortiert
pub(crate) struct Flaeche {
    sued: f64,
    west: f64,
    nord: f64,
    ost: f64,
    /// je Streifen die Kanten, die ihn berühren: (Breite, Länge) von Anfang und Ende
    streifen: Vec<Vec<Kante>>,
}

impl Flaeche {
    pub(crate) fn neu(kanten: Vec<Kante>) -> Flaeche {
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
    pub(crate) fn enthaelt(&self, lat: f64, lon: f64) -> bool {
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

    /// Abstand des Punkts vom Umriss in Grad (Länge mit dem Kosinus der Breite gestaucht), 0 innen
    pub(crate) fn abstand(&self, lat: f64, lon: f64) -> f64 {
        if self.enthaelt(lat, lon) {
            return 0.0;
        }
        let k = lat.to_radians().cos();
        let mut abstand = f64::INFINITY;
        for &((y1, x1), (y2, x2)) in self.streifen.iter().flatten() {
            let (dx, dy) = ((x2 - x1) * k, y2 - y1);
            let (px, py) = ((lon - x1) * k, lat - y1);
            let l2 = dx * dx + dy * dy;
            let t = if l2 > 0.0 {
                ((px * dx + py * dy) / l2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            abstand = abstand.min((px - t * dx).hypot(py - t * dy));
        }
        abstand
    }
}
