//! One-shot: load + save throne.db so outbound_json is re-exported in
//! upstream ExportToJson shape (fixes empty Address/Name in Qt Throne).
//!
//!   cargo run -p throne-storage --example repair_outbound_json -- /path/to/throne.db

use std::env;
use std::path::PathBuf;
use throne_storage::Database;

fn main() {
    let paths: Vec<PathBuf> = env::args().skip(1).map(PathBuf::from).collect();
    if paths.is_empty() {
        eprintln!("usage: repair_outbound_json <throne.db> [more.db...]");
        std::process::exit(2);
    }
    for path in paths {
        eprintln!("repairing {} …", path.display());
        let db = Database::open(&path).unwrap_or_else(|e| {
            eprintln!("open failed: {e}");
            std::process::exit(1);
        });
        let state = db.load_state().unwrap_or_else(|e| {
            eprintln!("load failed: {e}");
            std::process::exit(1);
        });
        let n = state.all_profiles().len();
        db.save_state(&state).unwrap_or_else(|e| {
            eprintln!("save failed: {e}");
            std::process::exit(1);
        });
        // Verify
        let re = db.load_state().unwrap();
        let mut ok = 0usize;
        let mut bad = 0usize;
        for p in re.all_profiles() {
            match serde_json::from_str::<serde_json::Value>(&p.outbound_json) {
                Ok(v) if throne_domain::is_upstream_shaped_outbound(&v) => ok += 1,
                _ => {
                    bad += 1;
                    eprintln!("  still bad id={} type={}", p.id, p.profile_type.as_str());
                }
            }
        }
        eprintln!("  profiles={n} upstream_shaped={ok} bad={bad}");
    }
}
