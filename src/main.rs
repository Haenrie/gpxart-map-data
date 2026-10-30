//! Kommandozeile: Wegegraph-Kacheln und Ortsverzeichnis aus `.osm.pbf` bauen, in einem Stück oder je Region.

use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;
use std::{env, fmt, fs};

use wegegraph::{
    Orte, Ortsverzeichnis, OsmExtract, Regionen, TileRange, build_tiles, build_tiles_of,
};

const USAGE: &str = "\
Aufruf: wegegraph <ausgabe-ordner> [<x0>-<x1> <y0>-<y1>] <eingabe.osm.pbf>...
        wegegraph orte <ausgabe-ordner> [<x0>-<x1> <y0>-<y1>]... <eingabe.osm.pbf>...
        wegegraph regionen <regionen.txt> <umrisse>
        wegegraph randwege <regionen.txt> <umrisse> <region> <ausgabe-ordner> <eingabe.osm.pbf>...
        wegegraph region <regionen.txt> <umrisse> <region> <ausgabe-ordner> <eingabe.osm.pbf>... [<randwege.rand>...]
        wegegraph ortsliste <regionen.txt> <region> <ausgabe.tsv> <eingabe.osm.pbf>...
        wegegraph ortsverzeichnis <ausgabe-ordner> <ortsliste.tsv>...

In einem Stück: Baut den Wegegraphen (Format 2, alle Fortbewegungsarten) für alle z12-Kacheln x0..=x1, y0..=y1,
ohne Bereich für alle Kacheln mit Wegen, und schreibt sie als <ausgabe-ordner>/12/<x>/<y>.bin. Kacheln ohne Wege
entfallen. Mit „orte“ entsteht stattdessen das Ortsverzeichnis (Format 1) als <ausgabe-ordner>/index.json und
<ausgabe-ordner>/<teil>.json, mit Bereichen nur für die Orte in diesen Kacheln. Mehrere Eingabedateien werden
zusammengeführt.

Je Region (Liste regionen.txt, Umrisse <umrisse>/<Auszug>.poly): „regionen“ prüft die Liste und die Umrisse.
„randwege“ schreibt aus den Auszügen der Region für jede andere Region die Wege, die eine ihrer Kacheln berühren, als
<ausgabe-ordner>/<region>-<andere>.rand. „region“ baut die Kacheln der Region aus ihren Auszügen und den Randwegen der
anderen für sie (Dateien auf .rand), genau so, als wären alle Auszüge in einem Stück gebaut. „ortsliste“ schreibt die
Orte der Region mit ihrem Land, „ortsverzeichnis“ baut aus den Listen aller Regionen das Ortsverzeichnis.

Beispiele: wegegraph graph/test 2169-2173 1421-1424 oberbayern.osm.pbf schwaben.osm.pbf
           wegegraph orte orte/test 2169-2173 1421-1424 oberbayern.osm.pbf schwaben.osm.pbf
           wegegraph regionen regionen.txt umrisse
           wegegraph randwege regionen.txt umrisse dach rand dach.osm.pbf
           wegegraph region regionen.txt umrisse dach graph/2026-10-18 dach.osm.pbf rand/france-dach.rand
           wegegraph ortsliste regionen.txt france orte-france.tsv france.osm.pbf monaco.osm.pbf andorra.osm.pbf
           wegegraph ortsverzeichnis orte/2026-10-18/orte orte-dach.tsv orte-france.tsv";

/// Kachelzeilen je Streifen: Ein Streifen von ganz DACH braucht beim Bau gut 1 GB Speicher.
const STREIFEN: u32 = 8;

type Ergebnis = Result<(), Box<dyn Error>>;

/// Ausgabe-Ordner, Bereiche und Eingabedateien eines Aufrufs
type Aufgabe<'a> = (PathBuf, Vec<TileRange>, &'a [String]);

/// Falscher Aufruf: Die Hilfe erscheint.
#[derive(Debug)]
struct Aufruf;

impl fmt::Display for Aufruf {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(USAGE)
    }
}

impl Error for Aufruf {}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let ergebnis = match args.split_first() {
        Some((befehl, rest)) if befehl == "orte" => run_orte(rest),
        Some((befehl, rest)) if befehl == "regionen" => run_regionen(rest),
        Some((befehl, rest)) if befehl == "randwege" => run_randwege(rest),
        Some((befehl, rest)) if befehl == "region" => run_region(rest),
        Some((befehl, rest)) if befehl == "ortsliste" => run_ortsliste(rest),
        Some((befehl, rest)) if befehl == "ortsverzeichnis" => run_ortsverzeichnis(rest),
        _ => run(&args),
    };
    match ergebnis {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) if e.is::<Aufruf>() => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
        Err(e) => {
            eprintln!("Fehler: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Ausgabe-Ordner, Bereiche aus z12-Kacheln (mit `mehrere` beliebig viele, sonst höchstens einer) und Eingabedateien
fn bereiche_und_eingaben(args: &[String], mehrere: bool) -> Result<Aufgabe<'_>, Box<dyn Error>> {
    let Some((out, mut inputs)) = args.split_first() else {
        return Err(Aufruf.into());
    };
    let mut ranges = Vec::new();
    while let [x, y, rest @ ..] = inputs
        && let (Some((x0, x1)), Some((y0, y1))) = (span(x), span(y))
        && (mehrere || ranges.is_empty())
    {
        ranges.push(TileRange { x0, x1, y0, y1 });
        inputs = rest;
    }
    if inputs.is_empty() {
        return Err(Aufruf.into());
    }
    Ok((PathBuf::from(out), ranges, inputs))
}

/// Regionen aus Liste und Umrissen, die Nummer der genannten Region und die übrigen Argumente
fn region(args: &[String]) -> Result<(Regionen, usize, &[String]), Box<dyn Error>> {
    let [liste, umrisse, name, rest @ ..] = args else {
        return Err(Aufruf.into());
    };
    let regionen = Regionen::lies(Path::new(liste), Path::new(umrisse))?;
    let nummer = regionen
        .nummer(name)
        .ok_or_else(|| format!("Region {name} steht nicht in {liste}"))?;
    Ok((regionen, nummer, rest))
}

fn run_orte(args: &[String]) -> Ergebnis {
    let (out, ranges, inputs) = bereiche_und_eingaben(args, true)?;
    let t0 = Instant::now();
    let orte = orte_lesen(Orte::default(), inputs, t0)?;
    schreibe_verzeichnis(&out, &orte.verzeichnis(&ranges), t0)
}

fn run_regionen(args: &[String]) -> Ergebnis {
    let [liste, umrisse] = args else {
        return Err(Aufruf.into());
    };
    let regionen = Regionen::lies(Path::new(liste), Path::new(umrisse))?;
    for (i, r) in regionen.alle().iter().enumerate() {
        let (anteil, andere) = regionen.ueberlappung(i);
        println!(
            "{:<16} {:<3} % in anderen ({}); {}; {}",
            r.name,
            (100.0 * anteil).round(),
            andere.join(", "),
            r.auszuege.join(","),
            r.staaten.join(" ")
        );
    }
    println!(
        "{} Regionen mit {} Auszügen, Umrisse überlappen sich nur am Rand",
        regionen.alle().len(),
        regionen
            .alle()
            .iter()
            .map(|r| r.auszuege.len())
            .sum::<usize>()
    );
    Ok(())
}

fn run_ortsliste(args: &[String]) -> Ergebnis {
    let [liste, name, out, inputs @ ..] = args else {
        return Err(Aufruf.into());
    };
    if inputs.is_empty() {
        return Err(Aufruf.into());
    }
    let t0 = Instant::now();
    let regionen = Regionen::lies_liste(Path::new(liste))?;
    let region = regionen
        .iter()
        .find(|r| &r.name == name)
        .ok_or_else(|| format!("Region {name} steht nicht in {liste}"))?;
    let orte = orte_lesen(Orte::fuer_staaten(&region.staaten), inputs, t0)?;
    let (text, anzahl, meldungen) = orte.ortsliste();
    for meldung in &meldungen {
        println!("{meldung}");
    }
    fs::write(out, text)?;
    println!(
        "{anzahl} Orte in {} ({:.1} s){}",
        region.staaten.join(", "),
        t0.elapsed().as_secs_f64(),
        speicher()
    );
    Ok(())
}

fn run_ortsverzeichnis(args: &[String]) -> Ergebnis {
    let Some((out, listen)) = args.split_first() else {
        return Err(Aufruf.into());
    };
    let t0 = Instant::now();
    let listen = listen
        .iter()
        .map(|l| fs::read_to_string(l).map_err(|e| format!("{l}: {e}")))
        .collect::<Result<Vec<_>, _>>()?;
    let verzeichnis = Ortsverzeichnis::aus_listen(&listen)?;
    schreibe_verzeichnis(Path::new(out), &verzeichnis, t0)
}

fn orte_lesen(mut orte: Orte, inputs: &[String], t0: Instant) -> Result<Orte, Box<dyn Error>> {
    for input in inputs {
        orte.read_pbf(Path::new(input))
            .map_err(|e| format!("{input}: {e}"))?;
        println!(
            "gelesen: {input} ({:.1} s){}",
            t0.elapsed().as_secs_f64(),
            speicher()
        );
    }
    Ok(orte)
}

fn schreibe_verzeichnis(out: &Path, verzeichnis: &Ortsverzeichnis, t0: Instant) -> Ergebnis {
    for meldung in &verzeichnis.meldungen {
        println!("{meldung}");
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

fn run(args: &[String]) -> Ergebnis {
    let (out, ranges, inputs) = bereiche_und_eingaben(args, false)?;
    let t0 = Instant::now();
    let osm = lesen(inputs, &[], t0)?;
    bauen(&osm, &out, ranges.first().copied(), None, t0)
}

fn run_randwege(args: &[String]) -> Ergebnis {
    let (regionen, eigene, [out, inputs @ ..]) = region(args)? else {
        return Err(Aufruf.into());
    };
    if inputs.is_empty() {
        return Err(Aufruf.into());
    }
    let t0 = Instant::now();
    let osm = lesen(inputs, &[], t0)?;
    let dateien = osm.randwege(&regionen, eigene);
    let out = Path::new(out);
    fs::create_dir_all(out)?;
    let von = &regionen.alle()[eigene].name;
    for (an, datei) in &dateien {
        let an = &regionen.alle()[*an].name;
        fs::write(out.join(format!("{von}-{an}.rand")), datei)?;
        println!("Randwege für {an}: {:.1} MB", datei.len() as f64 / 1e6);
    }
    println!(
        "{} Dateien mit Randwegen ({:.1} s){}",
        dateien.len(),
        t0.elapsed().as_secs_f64(),
        speicher()
    );
    Ok(())
}

fn run_region(args: &[String]) -> Ergebnis {
    let (regionen, eigene, [out, dateien @ ..]) = region(args)? else {
        return Err(Aufruf.into());
    };
    // Auszüge zuerst, dann die Randwege (.rand)
    let n = dateien.partition_point(|d| !d.ends_with(".rand"));
    let (inputs, randwege) = dateien.split_at(n);
    if inputs.is_empty() || randwege.iter().any(|d| !d.ends_with(".rand")) {
        return Err(Aufruf.into());
    }
    let t0 = Instant::now();
    let osm = lesen(inputs, randwege, t0)?;
    bauen(&osm, Path::new(out), None, Some((&regionen, eigene)), t0)
}

fn lesen(
    inputs: &[String],
    randwege: &[String],
    t0: Instant,
) -> Result<OsmExtract, Box<dyn Error>> {
    let mut osm = OsmExtract::default();
    for input in inputs {
        osm.read_pbf(Path::new(input))
            .map_err(|e| format!("{input}: {e}"))?;
        println!("gelesen: {input} ({:.1} s)", t0.elapsed().as_secs_f64());
    }
    for datei in randwege {
        osm.read_randwege(&fs::read(datei)?)
            .map_err(|e| format!("{datei}: {e}"))?;
        println!("gelesen: {datei} ({:.1} s)", t0.elapsed().as_secs_f64());
    }
    println!(
        "zusammen: {} Wege, {} Knoten{}",
        osm.way_count(),
        osm.node_count(),
        speicher()
    );
    Ok(osm)
}

/// Baut die Kacheln im Bereich, ohne Bereich alle mit Wegen; mit `region` nur die Kacheln dieser Region.
fn bauen(
    osm: &OsmExtract,
    out: &Path,
    range: Option<TileRange>,
    region: Option<(&Regionen, usize)>,
    t0: Instant,
) -> Ergebnis {
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

    fs::create_dir_all(out)?;
    let (mut count, mut nodes, mut edges, mut bytes) = (0, 0u64, 0u64, 0u64);
    let mut largest = (0, String::new());
    for band in range.bands(STREIFEN) {
        let tiles = match region {
            None => build_tiles(osm, &band),
            Some((regionen, eigene)) => {
                build_tiles_of(osm, &band, |x, y| regionen.besitzer(x, y) == eigene)
            }
        };
        for ((x, y), data) in tiles {
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
