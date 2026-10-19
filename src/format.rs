//! Kachel-Datei schreiben, Format 2 nach `docs/wegegraph-format.md`.

use std::collections::BTreeMap;

use crate::rules::{ABSCHNITTE, Klasse, Stellen};

const MAGIC: &[u8; 4] = b"GPXW";
const VERSION: u16 = 2;
const HEADER_LEN: usize = 32;
const DIR_ENTRY_LEN: usize = 12;

/// `nodes`: (qx, qy) je Knoten. `edges`: (a, b, Stellen in Kantenrichtung a → b, Länge in dm) mit a < b,
/// sortiert nach a.
pub(crate) fn encode(
    z: u8,
    x: u32,
    y: u32,
    nodes: &[(u16, u16)],
    edges: &[(u32, u32, Stellen, u16)],
) -> Vec<u8> {
    let (n, m) = (nodes.len(), edges.len());

    let mut knot = Vec::with_capacity(4 * n);
    nodes
        .iter()
        .for_each(|(qx, _)| knot.extend(qx.to_le_bytes()));
    nodes
        .iter()
        .for_each(|(_, qy)| knot.extend(qy.to_le_bytes()));

    let mut grad = vec![0u8; n];
    let mut kant = Vec::with_capacity(4 * m + n);
    for &(a, b, _, _) in edges {
        kant.extend(b.to_le_bytes());
        let g = &mut grad[a as usize];
        *g = g
            .checked_add(1)
            .expect("mehr als 255 Kanten ab einem Knoten");
    }
    kant.extend(&grad);

    let mut laen = Vec::with_capacity(2 * m);
    edges
        .iter()
        .for_each(|(_, _, _, len)| laen.extend(len.to_le_bytes()));

    let mut sections: Vec<(&[u8; 4], Vec<u8>)> =
        vec![(b"KNOT", knot), (b"KANT", kant), (b"LAEN", laen)];
    for (i, name) in ABSCHNITTE.iter().enumerate() {
        sections.push((name, stellen(edges.iter().map(|e| e.2[i]))));
    }

    let mut out = Vec::new();
    out.extend(MAGIC);
    out.extend(VERSION.to_le_bytes());
    out.extend((sections.len() as u16).to_le_bytes());
    out.extend(x.to_le_bytes());
    out.extend(y.to_le_bytes());
    out.extend([z, 0, 0, 0]);
    out.extend((n as u32).to_le_bytes());
    out.extend((m as u32).to_le_bytes());
    out.extend(0u32.to_le_bytes());
    debug_assert_eq!(out.len(), HEADER_LEN);

    let mut offset = align4(HEADER_LEN + DIR_ENTRY_LEN * sections.len());
    for (name, data) in &sections {
        out.extend(*name);
        out.extend((offset as u32).to_le_bytes());
        out.extend((data.len() as u32).to_le_bytes());
        offset = align4(offset + data.len());
    }
    for (_, data) in &sections {
        out.resize(align4(out.len()), 0);
        out.extend(data);
    }
    out
}

/// Stellen-Abschnitt einer Fortbewegungsart: Wörterbuch der Klassen (sortiert), dann je Kante der Index.
fn stellen(klassen: impl Iterator<Item = Klasse> + Clone) -> Vec<u8> {
    let mut woerterbuch: BTreeMap<Klasse, u8> = klassen.clone().map(|k| (k, 0)).collect();
    assert!(
        woerterbuch.len() <= 256,
        "mehr als 256 Klassen in einer Kachel"
    );
    let mut out = Vec::new();
    out.extend((woerterbuch.len() as u32).to_le_bytes());
    for (i, (klasse, index)) in woerterbuch.iter_mut().enumerate() {
        *index = i as u8;
        out.extend(klasse.hin.0.to_le_bytes());
        out.extend(klasse.zurueck.0.to_le_bytes());
    }
    out.extend(klassen.map(|k| woerterbuch[&k]));
    out
}

fn align4(v: usize) -> usize {
    (v + 3) & !3
}
