//! Randwege (gpxart.com #85): die Wege einer Region, die Kacheln einer anderen Region berühren, mit ihren Stellen und
//! allen Knoten. Die andere Region liest sie wie eine weitere Eingabedatei und baut ihre Kacheln an der Grenze damit
//! aus allen Wegen, die sie berühren (siehe `regionen.rs`).
//!
//! Datei (Little Endian), nur zwischen den Läufen einer Version, kein dauerhaftes Format:
//!
//! | Feld | Typ |
//! |---|---|
//! | Magic `GPXR`, Version 1, reserviert 0 | `u8[4]`, `u16`, `u16` |
//! | `s` Stellen, je 4 Klassen aus hin, zurück | `u32`, `s` × `u32[8]` |
//! | `w` Wege, `r` Knoten-IDs aller Wege zusammen | `u64`, `u64` |
//! | je Weg ID, Stellen-Nummer, `len`, Knoten-IDs | `w` × (`i64`, `u32`, `u32`, `i64[len]`) |
//! | `k` Knoten, je ID, Breite, Länge in 10⁻⁷ Grad | `u64`, `k` × (`i64`, `i32`, `i32`) |

use std::collections::{BTreeMap, HashMap};

use crate::rules::{Klasse, Stellen, Wert};
use crate::tiling::{self, FEHLT};
use crate::{Node, OsmExtract, Regionen, pbf};

const MAGIC: &[u8; 4] = b"GPXR";
const VERSION: u16 = 1;

impl OsmExtract {
    /// Die Randwege für jede andere Region, die welche bekommt: Nummer der Region → Datei. `eigene` ist die Region
    /// dieses Auszugs.
    pub fn randwege(&self, regionen: &Regionen, eigene: usize) -> BTreeMap<usize, Vec<u8>> {
        let mut besitzer: HashMap<(u32, u32), usize> = HashMap::new();
        let mut wege: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        tiling::kacheln_der_wege(self, |w, x, y| {
            let b = *besitzer
                .entry((x, y))
                .or_insert_with(|| regionen.besitzer(x, y));
            if b != eigene {
                let liste = wege.entry(b).or_default();
                if liste.last() != Some(&w) {
                    liste.push(w);
                }
            }
        });
        wege.into_iter()
            .map(|(b, w)| (b, self.randwege_datei(&w)))
            .collect()
    }

    /// Datei mit den Wegen (Nummern in `ways`, aufsteigend) und ihren Knoten
    fn randwege_datei(&self, wege: &[usize]) -> Vec<u8> {
        let index = self.index();
        let mut stellen: HashMap<u32, u32> = HashMap::new();
        let mut knoten: Vec<u32> = Vec::new();
        for &w in wege {
            let way = self.ways[w];
            let next = stellen.len() as u32;
            stellen.entry(way.stellen).or_insert(next);
            knoten.extend(
                index.refs[way.start..way.start + way.len as usize]
                    .iter()
                    .filter(|&&i| i != FEHLT),
            );
        }
        knoten.sort_unstable();
        knoten.dedup();

        let mut out = Vec::new();
        out.extend(MAGIC);
        out.extend(VERSION.to_le_bytes());
        out.extend(0u16.to_le_bytes());
        let mut nach_nummer: Vec<(u32, u32)> = stellen.iter().map(|(&s, &n)| (n, s)).collect();
        nach_nummer.sort_unstable();
        out.extend((nach_nummer.len() as u32).to_le_bytes());
        for (_, s) in nach_nummer {
            for klasse in self.stellen[s as usize] {
                out.extend(klasse.hin.0.to_le_bytes());
                out.extend(klasse.zurueck.0.to_le_bytes());
            }
        }
        out.extend((wege.len() as u64).to_le_bytes());
        let refs: u64 = wege.iter().map(|&w| self.ways[w].len as u64).sum();
        out.extend(refs.to_le_bytes());
        for &w in wege {
            let way = self.ways[w];
            out.extend(way.id.to_le_bytes());
            out.extend(stellen[&way.stellen].to_le_bytes());
            out.extend(way.len.to_le_bytes());
            for id in &self.refs[way.start..way.start + way.len as usize] {
                out.extend(id.to_le_bytes());
            }
        }
        out.extend((knoten.len() as u64).to_le_bytes());
        for i in knoten {
            let n = self.nodes[i as usize];
            out.extend(n.id.to_le_bytes());
            out.extend(n.lat.to_le_bytes());
            out.extend(n.lon.to_le_bytes());
        }
        out
    }

    /// Nimmt Randwege einer anderen Region auf. Mehrmals aufrufbar und mit `read_pbf` mischbar; was schon da ist,
    /// kommt nur einmal vor.
    pub fn read_randwege(&mut self, datei: &[u8]) -> Result<(), String> {
        self.index.take();
        let mut r = Leser { b: datei, o: 0 };
        if r.bytes(4)? != MAGIC || r.u16()? != VERSION {
            return Err("keine Randwege-Datei (Version 1)".into());
        }
        r.u16()?;
        let mut stellen: Vec<Stellen> = Vec::new();
        for _ in 0..r.u32()? {
            let mut s = Stellen::default();
            for klasse in &mut s {
                *klasse = Klasse {
                    hin: Wert(r.u32()?),
                    zurueck: Wert(r.u32()?),
                };
            }
            stellen.push(s);
        }
        let (ways_before, nodes_before) = (self.ways.len(), self.nodes.len());
        let wege = r.u64()? as usize;
        self.ways.reserve_exact(wege);
        self.refs.reserve_exact(r.u64()? as usize);
        for _ in 0..wege {
            let id = r.i64()?;
            let s = *stellen
                .get(r.u32()? as usize)
                .ok_or("Stellen-Nummer zu groß")?;
            let len = r.u32()? as usize;
            let refs = (0..len).map(|_| r.i64()).collect::<Result<Vec<_>, _>>()?;
            let way = self.push_way(id, refs.into_iter(), s);
            self.ways.push(way);
        }
        let knoten = r.u64()? as usize;
        self.nodes.reserve_exact(knoten);
        for _ in 0..knoten {
            let id = r.i64()?;
            let (lat, lon) = (r.i32()?, r.i32()?);
            self.nodes.push(Node { id, lat, lon });
        }
        if r.o != datei.len() {
            return Err("Daten nach dem Ende".into());
        }
        pbf::sort_dedup(&mut self.ways, ways_before, |w| w.id);
        pbf::sort_dedup(&mut self.nodes, nodes_before, |n| n.id);
        Ok(())
    }
}

struct Leser<'a> {
    b: &'a [u8],
    o: usize,
}

impl Leser<'_> {
    fn bytes(&mut self, n: usize) -> Result<&[u8], String> {
        let teil = self
            .b
            .get(self.o..self.o + n)
            .ok_or("Randwege-Datei zu kurz")?;
        self.o += n;
        Ok(teil)
    }

    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_le_bytes(self.bytes(2)?.try_into().unwrap()))
    }

    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.bytes(4)?.try_into().unwrap()))
    }

    fn i32(&mut self) -> Result<i32, String> {
        Ok(i32::from_le_bytes(self.bytes(4)?.try_into().unwrap()))
    }

    fn u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_le_bytes(self.bytes(8)?.try_into().unwrap()))
    }

    fn i64(&mut self) -> Result<i64, String> {
        Ok(i64::from_le_bytes(self.bytes(8)?.try_into().unwrap()))
    }
}
