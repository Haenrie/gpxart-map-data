//! Builder für den Wegegraphen von gpxart.com: OSM-Ausschnitt rein, z12-Kacheln raus.
//! Das Kachelformat steht in `docs/wegegraph-format.md` (im Repo-Wurzelordner).
//!
//! Speicher: Ganz DACH hat rund 20 Mio. Wege mit über 100 Mio. Knoten. Darum liegen Knoten und Wege in flachen,
//! nach OSM-ID sortierten Listen (statt in Maps), von jedem Weg bleibt statt der Tags nur der Verweis auf seine
//! Stellen, und die Kacheln entstehen streifenweise (`build_tiles` für einen Teilbereich).

mod format;
mod pbf;
mod rules;
mod tiling;

use std::collections::HashMap;
use std::sync::OnceLock;

use rules::{Stellen, Tags};
pub use tiling::build_tiles;

/// OSM-Ausschnitt im Speicher: Knoten-Koordinaten und Wege mit ihren Stellen.
///
/// Knoten und Wege sind nach OSM-ID sortiert und kommen je ID nur einmal vor, auch wenn sie in mehreren
/// Eingabedateien stehen (Wege an Regionsgrenzen); so ist der Bau reproduzierbar.
#[derive(Default)]
pub struct OsmExtract {
    nodes: Vec<Node>,
    ways: Vec<Way>,
    /// Knoten-IDs aller Wege hintereinander; jeder Weg verweist auf seinen Bereich.
    refs: Vec<i64>,
    /// Wörterbuch der Stellen; jeder Weg verweist auf einen Eintrag.
    stellen: Vec<Stellen>,
    stellen_nummer: HashMap<Stellen, u32>,
    /// Für den Bau vorbereitet (`tiling::Index`); jede Änderung verwirft es.
    index: OnceLock<tiling::Index>,
}

/// Knoten mit Breite und Länge in 10⁻⁷ Grad (so steht es im PBF).
#[derive(Clone, Copy)]
struct Node {
    id: i64,
    lat: i32,
    lon: i32,
}

impl Node {
    /// Breite und Länge in Grad, genau wie `osmpbf` sie aus dem PBF liest (Nanograd × 10⁻⁹).
    fn lat_lon(self) -> (f64, f64) {
        let grad = |d: i32| 1e-9 * (100 * d as i64) as f64;
        (grad(self.lat), grad(self.lon))
    }
}

/// Weg, der für mindestens eine Fortbewegungsart nutzbar ist.
#[derive(Clone, Copy)]
struct Way {
    id: i64,
    /// Bereich in `OsmExtract::refs`
    start: usize,
    len: u32,
    /// Eintrag in `OsmExtract::stellen`
    stellen: u32,
}

impl OsmExtract {
    pub fn add_node(&mut self, id: i64, lat: f64, lon: f64) {
        let dezimikro = |grad: f64| (grad * 1e7).round() as i32;
        let node = Node {
            id,
            lat: dezimikro(lat),
            lon: dezimikro(lon),
        };
        match self.nodes.binary_search_by_key(&id, |n| n.id) {
            Ok(i) => self.nodes[i] = node,
            Err(i) => self.nodes.insert(i, node),
        }
        self.index.take();
    }

    /// Nimmt den Weg auf, wenn er nach den Regeln ein Weg ist; ein Weg mit derselben ID wird ersetzt.
    pub fn add_way(&mut self, id: i64, nodes: &[i64], tags: &[(&str, &str)]) {
        let tags = Tags(tags.to_vec());
        let Some(stellen) = rules::stellen(&tags) else {
            return;
        };
        let way = self.push_way(id, nodes.iter().copied(), stellen);
        match self.ways.binary_search_by_key(&id, |w| w.id) {
            Ok(i) => self.ways[i] = way,
            Err(i) => self.ways.insert(i, way),
        }
        self.index.take();
    }

    /// Legt die Knoten-IDs des Weges ab und gibt ihn zurück, ohne ihn einzuordnen.
    fn push_way(&mut self, id: i64, nodes: impl Iterator<Item = i64>, stellen: Stellen) -> Way {
        let start = self.refs.len();
        self.refs.extend(nodes);
        let next = self.stellen.len() as u32;
        let nummer = *self.stellen_nummer.entry(stellen).or_insert(next);
        if nummer == next {
            self.stellen.push(stellen);
        }
        Way {
            id,
            start,
            len: (self.refs.len() - start) as u32,
            stellen: nummer,
        }
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn way_count(&self) -> usize {
        self.ways.len()
    }

    /// Alle z12-Kacheln, in denen Wege liegen können (Umriss aller Knoten), oder `None` ohne Knoten.
    pub fn extent(&self) -> Option<TileRange> {
        tiling::extent(self)
    }

    /// Index des Knotens in `nodes`
    fn node_index(&self, id: i64) -> Option<usize> {
        self.nodes.binary_search_by_key(&id, |n| n.id).ok()
    }
}

/// Rechteck aus z12-Kacheln (Slippy-Map-Nummern), Grenzen einschließlich.
#[derive(Clone, Copy, Debug)]
pub struct TileRange {
    pub x0: u32,
    pub x1: u32,
    pub y0: u32,
    pub y1: u32,
}

impl TileRange {
    fn contains(&self, x: u32, y: u32) -> bool {
        (self.x0..=self.x1).contains(&x) && (self.y0..=self.y1).contains(&y)
    }

    /// Teilt den Bereich in Streifen aus höchstens `rows` Kachelzeilen (von Norden nach Süden).
    pub fn bands(&self, rows: u32) -> impl Iterator<Item = TileRange> + '_ {
        (self.y0..=self.y1)
            .step_by(rows.max(1) as usize)
            .map(move |y| TileRange {
                y0: y,
                y1: (y + rows.max(1) - 1).min(self.y1),
                ..*self
            })
    }
}
