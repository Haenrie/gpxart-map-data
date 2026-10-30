//! OSM-Daten aus `.osm.pbf` lesen (Geofabrik-Auszüge).

use std::cell::Cell;
use std::path::Path;

use osmpbf::{Element, IndexedReader};

use crate::rules::{self, Stellen, Tags};
use crate::{Node, OsmExtract};

impl OsmExtract {
    /// Liest alle Wege mit `highway` und alle Fähren, die nach den Regeln Wege sind, und ihre Knoten aus einer
    /// `.osm.pbf`-Datei in zwei Durchläufen (erst Wege, dann nur deren Knoten). Mehrmals aufrufbar; was in mehreren
    /// Dateien steht (Wege an Regionsgrenzen), kommt nur einmal vor.
    pub fn read_pbf(&mut self, path: &Path) -> Result<(), osmpbf::Error> {
        self.index.take();
        let mut reader = IndexedReader::from_path(path)?;
        let (ways_before, nodes_before, refs_before) =
            (self.ways.len(), self.nodes.len(), self.refs.len());
        // Die Stellen rechnet schon der Filter, damit Wege ohne Stellen gar nicht erst ihre Knoten laden; der
        // Rückruf bekommt den Weg gleich danach.
        let stellen_des_weges: Cell<Option<Stellen>> = Cell::new(None);
        reader.read_ways_and_deps(
            |way| {
                let tags = Tags(
                    way.tags()
                        .filter(|(k, _)| rules::KEYS.contains(k))
                        .collect(),
                );
                stellen_des_weges.set(rules::stellen(&tags));
                stellen_des_weges.get().is_some()
            },
            |element| match element {
                Element::Way(way) => {
                    let stellen = stellen_des_weges.take().expect("Filter vor Rückruf");
                    let way = self.push_way(way.id(), way.refs(), stellen);
                    self.ways.push(way);
                }
                Element::Node(n) => self.push_node(
                    refs_before,
                    nodes_before,
                    n.id(),
                    n.decimicro_lat(),
                    n.decimicro_lon(),
                ),
                Element::DenseNode(n) => self.push_node(
                    refs_before,
                    nodes_before,
                    n.id(),
                    n.decimicro_lat(),
                    n.decimicro_lon(),
                ),
                Element::Relation(_) => {}
            },
        )?;
        self.refs.shrink_to_fit();
        sort_dedup(&mut self.ways, ways_before, |w| w.id);
        sort_dedup(&mut self.nodes, nodes_before, |n| n.id);
        self.nodes.shrink_to_fit();
        Ok(())
    }

    fn push_node(&mut self, refs_before: usize, nodes_before: usize, id: i64, lat: i32, lon: i32) {
        if self.nodes.len() == self.nodes.capacity() {
            // Höchstens so viele neue Knoten, wie die neuen Wege nennen: einmal genug Platz, statt die Liste (bei
            // DACH mehrere GB) beim Wachsen zu verdoppeln und zu kopieren.
            let neu = self.nodes.len() - nodes_before;
            let fehlen = (self.refs.len() - refs_before).saturating_sub(neu);
            self.nodes.reserve_exact(fehlen.max(1024));
        }
        self.nodes.push(Node { id, lat, lon });
    }
}

/// Sortiert die Liste nach ID und entfernt doppelte IDs, falls die neuen Einträge ab `before` das nötig machen.
/// Geofabrik-Dateien sind nach ID sortiert; bei nur einer Eingabedatei bleibt es beim Prüfen.
pub(crate) fn sort_dedup<T>(v: &mut Vec<T>, before: usize, id: impl Fn(&T) -> i64) {
    let sorted = v
        .windows(2)
        .skip(before.saturating_sub(1))
        .all(|w| id(&w[0]) < id(&w[1]));
    if !sorted {
        v.sort_by_key(&id);
        v.dedup_by_key(|t| id(t));
    }
}
