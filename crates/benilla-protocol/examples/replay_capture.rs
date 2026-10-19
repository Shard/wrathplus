//! Replay a recorded build-12340 capture (`WOW_WRATH_PACKET_RECORD`) through the protocol layer and
//! report what each opcode did, cross-referenced with `docs/WRATH-OPCODE-COVERAGE.md`.
//!
//! ```sh
//! cargo run -p benilla-protocol --example replay_capture -- <capture.azbwrpk> [--coverage <md>]
//! ```
//!
//! Findings, in order of interest: opcodes the coverage table calls handled or capture-only that no
//! decoder accepted here (a decode mismatch or a regression; the adapter answers a malformed body
//! for a known family as unhandled), decode failures, then unhandled opcodes the table already
//! lists as missing. Captures hold private gameplay data: keep them and this report local.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use benilla_protocol::wrath_capture::{self, Route};

/// `(name, S→C status)` per opcode from the coverage table.
fn coverage(path: &Path) -> BTreeMap<u16, (String, String)> {
    let Ok(text) = std::fs::read_to_string(path) else {
        eprintln!(
            "(no coverage table at {}; statuses omitted)",
            path.display()
        );
        return BTreeMap::new();
    };
    let mut out = BTreeMap::new();
    for line in text.lines().filter(|l| l.starts_with("| `0x")) {
        let c: Vec<&str> = line.split('|').map(str::trim).collect();
        if c.len() < 5 {
            continue;
        }
        let Ok(opcode) = u16::from_str_radix(c[1].trim_matches('`').trim_start_matches("0x"), 16)
        else {
            continue;
        };
        let (dir, status) = (c[3], c[4]);
        let s2c = match dir {
            "S→C" => status,
            "S→C / C→S" => status.split('/').next().unwrap_or("").trim(),
            _ => continue,
        };
        out.insert(
            opcode,
            (c[2].trim_matches('`').to_string(), s2c.to_string()),
        );
    }
    out
}

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(file) = args.first() else {
        eprintln!(
            "usage: replay_capture <capture.azbwrpk> [--coverage <WRATH-OPCODE-COVERAGE.md>]"
        );
        return std::process::ExitCode::from(2);
    };
    let table = args
        .iter()
        .position(|a| a == "--coverage")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/WRATH-OPCODE-COVERAGE.md")
        });
    let bytes = match std::fs::read(file) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("{file}: {e}");
            return std::process::ExitCode::from(2);
        }
    };
    let capture = match wrath_capture::read(&bytes) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{file}: {e}");
            return std::process::ExitCode::from(2);
        }
    };
    let table = coverage(&table);
    let (census, _) = wrath_capture::census(&capture.records);
    let span_s = capture
        .records
        .last()
        .map_or(0.0, |r| r.elapsed_us as f64 / 1e6);
    println!(
        "capture: {} records over {span_s:.1} s{}; {} packets replayed (bundles split)",
        capture.records.len(),
        if capture.truncated_tail {
            ", final record cut off"
        } else {
            ""
        },
        census.packets
    );
    let name = |o: u16| table.get(&o).map_or("?", |(n, _)| n.as_str());
    let status = |o: u16| table.get(&o).map_or("?", |(_, s)| s.as_str());

    let regressions: Vec<_> = census
        .opcodes
        .iter()
        .filter(|(o, t)| t.unhandled > 0 && matches!(status(**o), "handled" | "capture-only"))
        .collect();
    println!(
        "\nFINDINGS: table says handled/capture-only, nothing decoded it ({}):",
        regressions.len()
    );
    for (o, t) in &regressions {
        println!(
            "  {o:#06x} {:<40} {} of {} unhandled ({})",
            name(**o),
            t.unhandled,
            t.seen,
            status(**o)
        );
    }
    println!("\nFINDINGS: decode failures ({}):", census.failures.len());
    for f in census.failures.iter().take(50) {
        println!(
            "  #{:<6} {:#06x} {:<40} {}",
            f.index,
            f.opcode,
            name(f.opcode),
            f.error
        );
    }
    if census.failures.len() > 50 {
        println!("  … {} more", census.failures.len() - 50);
    }
    println!("\nunhandled, as the table expects or unknown to it:");
    for (o, n) in census.unhandled() {
        if !matches!(status(o), "handled" | "capture-only") {
            println!("  {o:#06x} {:<40} {n:>6}  ({})", name(o), status(o));
        }
    }
    println!("\nper opcode (route, seen, handled, unhandled, failed, events, table status):");
    let mut rows: Vec<_> = census.opcodes.iter().collect();
    rows.sort_by(|a, b| b.1.seen.cmp(&a.1.seen).then(a.0.cmp(b.0)));
    for (o, t) in rows {
        let route = match t.route {
            Some(Route::Bundle) => "bundle",
            Some(Route::Session) => "session",
            Some(Route::Relay) => "relay",
            Some(Route::Adapter) | None => "adapter",
        };
        println!(
            "  {o:#06x} {:<40} {route:<8} {:>6} {:>6} {:>6} {:>6} {:>6}  {}",
            name(*o),
            t.seen,
            t.handled,
            t.unhandled,
            t.failed,
            t.events,
            status(*o)
        );
    }
    println!("\nsession events:");
    for (kind, n) in &census.events {
        println!("  {kind:<40} {n:>6}");
    }
    if regressions.is_empty() && census.failures.is_empty() {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::from(1)
    }
}
