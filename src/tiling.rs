//! Wege → Kanten → z12-Kacheln.
//!
//! Koordinaten liegen in einem weltweiten Ganzzahl-Raster (Web Mercator, 2^12 Kacheln × 2^15 Schritte je Kante).
//! Kachelgrenzen sind Rasterlinien. Wo ein Weg eine Grenze kreuzt, entsteht ein Randknoten genau auf der Linie;
//! beide Nachbarkacheln speichern ihn mit derselben Rasterkoordinate. Darüber setzt der Router Kacheln zusammen.

use std::collections::{BTreeMap, HashMap};
use std::f64::consts::PI;

use crate::rules::Stellen;
use crate::{OsmExtract, TileRange, format};

pub(crate) const ZOOM: u8 = 12;
/// Rasterschritte je Kachelkante.
pub(crate) const EXTENT: i64 = 1 << 15;
/// Rasterschritte rund um die Welt.
const WORLD: f64 = ((1u64 << ZOOM) as i64 * EXTENT) as f64;

/// Ketten zusammenfassen: Eine Kette endet spätestens, wenn sie 80 m lang ist (wie im Prototyp gpxart.com #15).
/// Darin bleiben so viele Zwischenpunkte, dass die Kanten höchstens 2 m von der OSM-Geometrie abweichen
/// (Douglas-Peucker), damit Routen und GPX die Form der Wege behalten (gpxart.com #12: keine Ausdünnung).
const MAX_CHAIN_M: f64 = 80.0;
const MAX_ABWEICHUNG_M: f64 = 2.0;
/// Längere Wegstücke werden geteilt, damit jede Kantenlänge in u16-Dezimeter passt.
const MAX_SEGMENT_M: f64 = 5000.0;
const EARTH_RADIUS_M: f64 = 6_371_008.8;

/// Kennung eines Knotens beim Bau. Punkte auf einer Kachelgrenze werden über ihre Rasterkoordinate erkannt,
/// alle anderen über die OSM-ID bzw. als Teilungspunkt eines Weges.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum NodeKey {
    Border(i64, i64),
    Osm(i64),
    Split(i64, u32),
}

#[derive(Clone, Copy)]
struct Point {
    x: i64,
    y: i64,
    key: NodeKey,
    /// Muss als Knoten erhalten bleiben (Kreuzung, Wegende, Kachelgrenze, Teilungspunkt).
    keep: bool,
}

impl Point {
    fn new(x: i64, y: i64, key: NodeKey, keep: bool) -> Self {
        if x % EXTENT == 0 || y % EXTENT == 0 {
            Point {
                x,
                y,
                key: NodeKey::Border(x, y),
                keep: true,
            }
        } else {
            Point { x, y, key, keep }
        }
    }
}

/// Breite/Länge in Grad → Rasterkoordinate.
pub(crate) fn project(lat: f64, lon: f64) -> (i64, i64) {
    let x = (lon + 180.0) / 360.0 * WORLD;
    let y = (1.0 - lat.to_radians().tan().asinh() / PI) / 2.0 * WORLD;
    (x.round() as i64, y.round() as i64)
}

fn haversine_m((lat1, lon1): (f64, f64), (lat2, lon2): (f64, f64)) -> f64 {
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let dp = p2 - p1;
    let dl = (lon2 - lon1).to_radians();
    let h = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_M * h.sqrt().asin()
}

/// Für den Bau vorbereitete Verweise, einmal je OSM-Ausschnitt (`OsmExtract::index`).
pub(crate) struct Index {
    /// Je Eintrag in `OsmExtract::refs` der Index des Knotens in `OsmExtract::nodes`, `FEHLT` für einen Knoten,
    /// der nicht in der Eingabe steht (Rand der Eingabedatei).
    refs: Vec<u32>,
    /// Je Knoten: wie oft er in Wegen vorkommt, höchstens 2. Mehr als einmal heißt Kreuzung.
    uses: Vec<u8>,
    /// Je Weg sein Umriss in Kachelnummern (x0, y0, x1, y1); ohne bekannten Knoten leer (x0 > x1).
    tiles: Vec<[u32; 4]>,
}

const FEHLT: u32 = u32::MAX;

impl Index {
    fn new(osm: &OsmExtract) -> Index {
        assert!(osm.nodes.len() < FEHLT as usize, "zu viele Knoten");
        let refs = parallel(&osm.refs, |&id| {
            osm.node_index(id).map_or(FEHLT, |i| i as u32)
        });
        let mut uses = vec![0u8; osm.nodes.len()];
        for way in &osm.ways {
            for &i in &refs[way.start..way.start + way.len as usize] {
                if i != FEHLT {
                    uses[i as usize] = (uses[i as usize] + 1).min(2);
                }
            }
        }
        let tiles = parallel(&osm.ways, |way| {
            let mut t = [u32::MAX, u32::MAX, 0, 0];
            for &i in refs[way.start..way.start + way.len as usize]
                .iter()
                .filter(|&&i| i != FEHLT)
            {
                let (lat, lon) = osm.nodes[i as usize].lat_lon();
                let (x, y) = project(lat, lon);
                let (tx, ty) = (x.div_euclid(EXTENT) as u32, y.div_euclid(EXTENT) as u32);
                t = [t[0].min(tx), t[1].min(ty), t[2].max(tx), t[3].max(ty)];
            }
            t
        });
        Index { refs, uses, tiles }
    }
}

/// `f` auf jedes Element, verteilt auf alle Rechenkerne (bei DACH über 100 Mio. Elemente).
fn parallel<T: Sync, U: Send + Clone + Default>(items: &[T], f: impl Fn(&T) -> U + Sync) -> Vec<U> {
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    let chunk = items.len().div_ceil(threads).max(1);
    let mut out = vec![U::default(); items.len()];
    std::thread::scope(|s| {
        for (src, dst) in items.chunks(chunk).zip(out.chunks_mut(chunk)) {
            let f = &f;
            s.spawn(move || {
                for (d, item) in dst.iter_mut().zip(src) {
                    *d = f(item);
                }
            });
        }
    });
    out
}

impl OsmExtract {
    fn index(&self) -> &Index {
        self.index.get_or_init(|| Index::new(self))
    }
}

/// Alle Kacheln, die Wege berühren, oder `None` ohne Wege.
pub(crate) fn extent(osm: &OsmExtract) -> Option<TileRange> {
    let t = osm.index().tiles.iter().filter(|t| t[0] <= t[2]).fold(
        [u32::MAX, u32::MAX, 0, 0],
        |a, t| {
            [
                a[0].min(t[0]),
                a[1].min(t[1]),
                a[2].max(t[2]),
                a[3].max(t[3]),
            ]
        },
    );
    (t[0] <= t[2]).then_some(TileRange {
        x0: t[0],
        y0: t[1],
        x1: t[2],
        y1: t[3],
    })
}

/// Baut alle Kacheln des Bereichs mit den Stellen aller Fortbewegungsarten, auch leere. Schlüssel ist (x, y), Wert die Kachel-Datei.
///
/// Eine Kachel hängt nur von den Wegen ab, die sie berühren; ein großes Gebiet lässt sich darum in Teilbereichen
/// bauen (`TileRange::bands`), mit denselben Kacheln wie in einem Stück.
pub fn build_tiles(osm: &OsmExtract, range: &TileRange) -> BTreeMap<(u32, u32), Vec<u8>> {
    let index = osm.index();
    let mut tiles: HashMap<(u32, u32), TileGraph> = HashMap::new();
    for (way, t) in osm.ways.iter().zip(&index.tiles) {
        if t[0] > range.x1 || t[2] < range.x0 || t[1] > range.y1 || t[3] < range.y0 {
            continue;
        }
        let stellen = osm.stellen[way.stellen as usize];
        let mut splits = 0;
        // Fehlende Knoten (Rand der Eingabedatei) teilen den Weg in Läufe.
        for run in index.refs[way.start..way.start + way.len as usize].split(|&i| i == FEHLT) {
            if run.len() < 2 {
                continue;
            }
            let (points, lengths) = expand(osm, &index.uses, way.id, &mut splits, run);
            compress(&points, &lengths, |a, b, len| {
                let tx = (a.x + b.x).div_euclid(2 * EXTENT) as u32;
                let ty = (a.y + b.y).div_euclid(2 * EXTENT) as u32;
                if range.contains(tx, ty) {
                    tiles
                        .entry((tx, ty))
                        .or_default()
                        .add_edge(tx, ty, a, b, len, stellen);
                }
            });
        }
    }

    let mut out = BTreeMap::new();
    for x in range.x0..=range.x1 {
        for y in range.y0..=range.y1 {
            let graph = tiles.remove(&(x, y)).unwrap_or_default();
            out.insert((x, y), graph.encode(x, y));
        }
    }
    out
}

/// Punktfolge eines Weglaufs (Knoten-Indizes) mit eingefügten Kachelgrenz- und Teilungspunkten, dazu die Länge
/// jedes Stücks in m. `splits` zählt die Teilungspunkte des ganzen Weges, damit ihre Kennungen über alle Läufe
/// eindeutig bleiben.
fn expand(
    osm: &OsmExtract,
    uses: &[u8],
    way_id: i64,
    splits: &mut u32,
    run: &[u32],
) -> (Vec<Point>, Vec<f64>) {
    let mut points = Vec::with_capacity(run.len());
    let mut lengths = Vec::with_capacity(run.len());
    let last = run.len() - 1;
    let mut prev: Option<((f64, f64), (i64, i64))> = None;
    for (i, &node) in run.iter().enumerate() {
        let node = node as usize;
        let ll = osm.nodes[node].lat_lon();
        let (bx, by) = project(ll.0, ll.1);
        if let Some((pll, (ax, ay))) = prev {
            let seg_m = haversine_m(pll, ll);
            let mut t_prev = 0.0;
            for (t, fx, fy) in cuts((ax, ay), (bx, by), seg_m) {
                let x = fx.unwrap_or_else(|| (ax as f64 + t * (bx - ax) as f64).round() as i64);
                let y = fy.unwrap_or_else(|| (ay as f64 + t * (by - ay) as f64).round() as i64);
                points.push(Point::new(x, y, NodeKey::Split(way_id, *splits), true));
                *splits += 1;
                lengths.push(seg_m * (t - t_prev));
                t_prev = t;
            }
            lengths.push(seg_m * (1.0 - t_prev));
        }
        let keep = i == 0 || i == last || uses[node] > 1;
        points.push(Point::new(bx, by, NodeKey::Osm(osm.nodes[node].id), keep));
        prev = Some((ll, (bx, by)));
    }
    (points, lengths)
}

/// Stellen (Anteil t ∈ (0, 1)) auf der Strecke a→b, an denen sie eine Kachelgrenze kreuzt oder für
/// `MAX_SEGMENT_M` geteilt wird. Auf einer Grenzlinie ist die Koordinate exakt (fx bzw. fy).
fn cuts(a: (i64, i64), b: (i64, i64), seg_m: f64) -> Vec<(f64, Option<i64>, Option<i64>)> {
    let mut cuts = Vec::new();
    let mut lines = |from: i64, to: i64, vertical: bool| {
        let (lo, hi) = (from.min(to), from.max(to));
        let mut c = lo.div_euclid(EXTENT) + 1;
        while c * EXTENT < hi {
            let t = (c * EXTENT - from) as f64 / (to - from) as f64;
            let line = Some(c * EXTENT);
            cuts.push(if vertical {
                (t, line, None)
            } else {
                (t, None, line)
            });
            c += 1;
        }
    };
    lines(a.0, b.0, true);
    lines(a.1, b.1, false);
    let pieces = (seg_m / MAX_SEGMENT_M).ceil() as u32;
    for k in 1..pieces {
        cuts.push((k as f64 / pieces as f64, None, None));
    }
    cuts.sort_by(|p, q| p.0.total_cmp(&q.0));
    // Fast gleiche Stellen (Ecke einer Kachel, Teilung auf der Grenze) zu einer zusammenfassen.
    let mut merged: Vec<(f64, Option<i64>, Option<i64>)> = Vec::with_capacity(cuts.len());
    for c in cuts {
        match merged.last_mut() {
            Some(m) if c.0 - m.0 < 1e-9 => {
                m.1 = m.1.or(c.1);
                m.2 = m.2.or(c.2);
            }
            _ => merged.push(c),
        }
    }
    merged
}

/// Fasst die Punktfolge zu Kanten zusammen und meldet jede Kante mit ihrer Länge in m.
fn compress(points: &[Point], lengths: &[f64], mut emit: impl FnMut(&Point, &Point, f64)) {
    let mut start = 0;
    let mut acc = 0.0;
    for i in 1..points.len() {
        acc += lengths[i - 1];
        if points[i].keep || acc >= MAX_CHAIN_M {
            emit_chain(&points[start..=i], &lengths[start..i], &mut emit);
            (start, acc) = (i, 0.0);
        }
    }
}

/// Eine Kette (Anfang und Ende werden Knoten) als Kanten: Zwischenpunkte nach Douglas-Peucker mit
/// `MAX_ABWEICHUNG_M`, Länge jeder Kante entlang des OSM-Weges.
fn emit_chain(chain: &[Point], lengths: &[f64], emit: &mut impl FnMut(&Point, &Point, f64)) {
    let last = chain.len() - 1;
    let mut node = vec![false; chain.len()];
    (node[0], node[last]) = (true, true);
    let tolerance = MAX_ABWEICHUNG_M / metres_per_step(chain[0].y);
    douglas_peucker(chain, 0, last, tolerance, &mut node);
    let (mut from, mut len) = (0, 0.0);
    for i in 1..=last {
        len += lengths[i - 1];
        if node[i] {
            emit(&chain[from], &chain[i], len);
            (from, len) = (i, 0.0);
        }
    }
}

/// Markiert in `node` die Punkte zwischen a und b, die bleiben müssen, damit keiner weiter als `tolerance`
/// (Rasterschritte) von seiner Kante abliegt.
fn douglas_peucker(p: &[Point], a: usize, b: usize, tolerance: f64, node: &mut [bool]) {
    let (mut worst, mut at) = (tolerance, None);
    for i in a + 1..b {
        let d = distance_to_segment(&p[i], &p[a], &p[b]);
        if d > worst {
            (worst, at) = (d, Some(i));
        }
    }
    if let Some(i) = at {
        node[i] = true;
        douglas_peucker(p, a, i, tolerance, node);
        douglas_peucker(p, i, b, tolerance, node);
    }
}

/// Abstand des Punkts p von der Strecke a–b in Rasterschritten.
fn distance_to_segment(p: &Point, a: &Point, b: &Point) -> f64 {
    let (dx, dy) = ((b.x - a.x) as f64, (b.y - a.y) as f64);
    let (px, py) = ((p.x - a.x) as f64, (p.y - a.y) as f64);
    let l2 = dx * dx + dy * dy;
    let t = if l2 > 0.0 {
        ((px * dx + py * dy) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (px - t * dx).hypot(py - t * dy)
}

/// Meter je Rasterschritt auf der Höhe der Rasterzeile y (Web Mercator, Erdradius wie bei den Kantenlängen).
fn metres_per_step(y: i64) -> f64 {
    let lat = (PI * (1.0 - 2.0 * y as f64 / WORLD)).sinh().atan();
    2.0 * PI * EARTH_RADIUS_M / WORLD * lat.cos()
}

/// Knoten und Kanten einer Kachel während des Baus.
#[derive(Default)]
struct TileGraph {
    index: HashMap<NodeKey, u32>,
    nodes: Vec<(NodeKey, u16, u16)>,
    /// (kleinerer, größerer Knotenindex, Stellen in dieser Richtung) → Länge in m; von doppelten Kanten mit gleichen
    /// Stellen gewinnt die kürzeste.
    edges: HashMap<(u32, u32, Stellen), f64>,
}

/// Stellen in Gegenrichtung
fn gedreht(stellen: Stellen) -> Stellen {
    stellen.map(|k| k.gedreht())
}

impl TileGraph {
    fn node(&mut self, tx: u32, ty: u32, p: &Point) -> u32 {
        let next = self.nodes.len() as u32;
        let i = *self.index.entry(p.key).or_insert(next);
        if i == next {
            let qx = p.x - tx as i64 * EXTENT;
            let qy = p.y - ty as i64 * EXTENT;
            debug_assert!((0..=EXTENT).contains(&qx) && (0..=EXTENT).contains(&qy));
            self.nodes.push((p.key, qx as u16, qy as u16));
        }
        i
    }

    /// Kante a → b; `stellen` gelten in dieser Richtung.
    fn add_edge(&mut self, tx: u32, ty: u32, a: &Point, b: &Point, len: f64, stellen: Stellen) {
        let (u, v) = (self.node(tx, ty, a), self.node(tx, ty, b));
        if u == v {
            return;
        }
        let key = if u < v {
            (u, v, stellen)
        } else {
            (v, u, gedreht(stellen))
        };
        let e = self.edges.entry(key).or_insert(len);
        *e = e.min(len);
    }

    /// Knoten zeilenweise sortieren (reproduzierbar), Kanten nach Startknoten, dann schreiben.
    fn encode(self, x: u32, y: u32) -> Vec<u8> {
        let mut order: Vec<u32> = (0..self.nodes.len() as u32).collect();
        order.sort_by_key(|&i| {
            let (key, qx, qy) = self.nodes[i as usize];
            (qy, qx, key)
        });
        let mut rank = vec![0u32; order.len()];
        for (r, &i) in order.iter().enumerate() {
            rank[i as usize] = r as u32;
        }
        let nodes: Vec<(u16, u16)> = order
            .iter()
            .map(|&i| (self.nodes[i as usize].1, self.nodes[i as usize].2))
            .collect();
        let mut edges: Vec<(u32, u32, Stellen, u16)> = self
            .edges
            .into_iter()
            .map(|((u, v, stellen), len)| {
                let (a, b) = (rank[u as usize], rank[v as usize]);
                let dm = (len * 10.0).round();
                debug_assert!(dm <= u16::MAX as f64);
                if a < b {
                    (a, b, stellen, dm as u16)
                } else {
                    (b, a, gedreht(stellen), dm as u16)
                }
            })
            .collect();
        edges.sort_unstable();
        format::encode(ZOOM, x, y, &nodes, &edges)
    }
}
