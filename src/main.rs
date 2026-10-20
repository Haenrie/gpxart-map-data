//! Kommandozeile: Wegegraph-Kacheln und Ortsverzeichnis aus `.osm.pbf` bauen.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;
use std::{env, fs};

use wegegraph::{Orte, OsmExtract, TileRange, build_tiles};

const USAGE: &str = "\
Aufruf: wegegraph <ausgabe-ordner> [<x0>-<x1> <y0>-<y1>] <eingabe.osm.pbf>...
        wegegraph orte <ausgabe-ordner> [<x0>-<x1> <y0>-<y1>]... <eingabe.osm.pbf>...

Baut den Wegegraphen (Format 2, alle Fortbewegungsarten) für alle z12-Kacheln x0..=x1, y0..=y1, ohne Bereich für
alle Kacheln mit Wegen, und schreibt sie als <ausgabe-ordner>/12/<x>/<y>.bin. Kacheln ohne Wege entfallen.
Mit „orte“ entsteht stattdessen das Ortsverzeichnis (Format 1) als <ausgabe-ordner>/index.json und
<ausgabe-ordner>/<teil>.json, mit Bereichen nur für die Orte in diesen Kacheln.
Mehrere Eingabedateien werden zusammengeführt.
Beispiele: wegegraph graph/test 2169-2173 1421-1424 oberbayern.osm.pbf schwaben.osm.pbf
           wegegraph graph/2026-10-18 dach.osm.pbf
           wegegraph orte orte/2026-10-18/orte dach.osm.pbf";

/// Kachelzeilen je Streifen: Ein Streifen von ganz DACH braucht beim Bau gut 1 GB Speicher.
const STREIFEN: u32 = 8;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let (orte, args) = match args.split_first() {
        Some((befehl, rest)) if befehl == "orte" => (true, rest),
        _ => (false, &args[..]),
    };
    let Some((out, mut inputs)) = args.split_first() else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    // Bereiche: beim Wegegraphen höchstens einer, beim Ortsverzeichnis beliebig viele
    let mut ranges = Vec::new();
    while let [x, y, rest @ ..] = inputs
        && let (Some((x0, x1)), Some((y0, y1))) = (span(x), span(y))
        && (orte || ranges.is_empty())
    {
        ranges.push(TileRange { x0, x1, y0, y1 });
        inputs = rest;
    }
    if inputs.is_empty() {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }
    let out = PathBuf::from(out);
    let ergebnis = if orte {
        run_orte(&out, &ranges, inputs)
    } else {
        run(&out, ranges.first().copied(), inputs)
    };
    match ergebnis {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Fehler: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run_orte(out: &Path, ranges: &[TileRange], inputs: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let t0 = Instant::now();
    let mut orte = Orte::default();
    for input in inputs {
        orte.read_pbf(Path::new(input))
            .map_err(|e| format!("{input}: {e}"))?;
        println!("gelesen: {input} ({:.1} s){}", t0.elapsed().as_secs_f64(), speicher());
    }
    let verzeichnis = orte.verzeichnis(ranges);
    for grenze in &verzeichnis.unvollstaendig {
        println!("unvollständig, nicht berücksichtigt: {grenze}");
    }
    fs::create_dir_all(out)?;
    let (mut bytes, mut groesster) = (0, (0, ""));
    for (name, inhalt) in &verzeichnis.dateien {
        fs::write(out.join(name), inhalt)?;
        bytes += inhalt.len();
        if inhalt.len() > groesster.0 {
            groesster = (inhalt.len(), name);
        }
    }
    println!(
        "{} Orte in {} Ländern, {} Dateien, {:.2} MB, größte {} mit {:.0} kB ({:.1} s){}",
        verzeichnis.orte,
        verzeichnis.laender,
        verzeichnis.dateien.len(),
        bytes as f64 / 1e6,
        groesster.1,
        groesster.0 as f64 / 1e3,
        t0.elapsed().as_secs_f64(),
        speicher()
    );
    Ok(())
}

fn run(
    out: &Path,
    range: Option<TileRange>,
    inputs: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    let t0 = Instant::now();
    let mut osm = OsmExtract::default();
    for input in inputs {
        osm.read_pbf(Path::new(input))
            .map_err(|e| format!("{input}: {e}"))?;
        println!("gelesen: {input} ({:.1} s)", t0.elapsed().as_secs_f64());
    }
    println!(
        "zusammen: {} Wege, {} Knoten{}",
        osm.way_count(),
        osm.node_count(),
        speicher()
    );

    let Some(range) = range.or_else(|| osm.extent()) else {
        return Err("keine Wege in der Eingabe".into());
    };
    println!(
        "Kacheln x {}–{}, y {}–{} ({:.1} s)",
        range.x0,
        range.x1,
        range.y0,
        range.y1,
        t0.elapsed().as_secs_f64()
    );

    let (mut count, mut nodes, mut edges, mut bytes) = (0, 0u64, 0u64, 0u64);
    let mut largest = (0, String::new());
    for band in range.bands(STREIFEN) {
        for ((x, y), data) in build_tiles(&osm, &band) {
            let (n, m) = (u32_le(&data, 20), u32_le(&data, 24));
            if n == 0 {
                continue;
            }
            let dir = out.join("12").join(x.to_string());
            fs::create_dir_all(&dir)?;
            fs::write(dir.join(format!("{y}.bin")), &data)?;
            if data.len() > largest.0 {
                largest = (data.len(), format!("12/{x}/{y}.bin"));
            }
            (count, nodes, edges, bytes) = (
                count + 1,
                nodes + n as u64,
                edges + m as u64,
                bytes + data.len() as u64,
            );
        }
        println!(
            "Zeilen {}–{}: bisher {count} Kacheln, {:.1} MB ({:.1} s){}",
            band.y0,
            band.y1,
            bytes as f64 / 1e6,
            t0.elapsed().as_secs_f64(),
            speicher()
        );
    }
    println!(
        "{count} Kacheln, {nodes} Knoten, {edges} Kanten, {:.2} MB, größte {} mit {:.2} MB ({:.1} s){}",
        bytes as f64 / 1e6,
        largest.1,
        largest.0 as f64 / 1e6,
        t0.elapsed().as_secs_f64(),
        speicher()
    );
    Ok(())
}

/// "2169-2173" → (2169, 2173)
fn span(s: &str) -> Option<(u32, u32)> {
    let (a, b) = s.split_once('-')?;
    let (a, b) = (a.parse().ok()?, b.parse().ok()?);
    (a <= b).then_some((a, b))
}

fn u32_le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

/// Speicher-Höchststand des Prozesses, soweit das System ihn verrät (Linux)
fn speicher() -> String {
    fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmHWM:"))
                .map(|l| format!(", Speicher höchstens {}", l[6..].trim()))
        })
        .unwrap_or_default()
}
