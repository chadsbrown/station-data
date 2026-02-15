use contest_engine::spec::{CabrilloQso, ContestSpec, ResolvedStation, SpecSession, Value};
use contest_engine::types::{Band, Callsign, Continent};
use station_data::{CtyDb, DomainPack};
use std::collections::HashMap;
use std::path::PathBuf;

#[test]
fn station_data_wires_into_contest_engine() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let cty_path = root.join("tests/fixtures/cty_sample.dat");
    let spec_path = root.join("tests/fixtures/cqww.json");

    let resolver = CtyDb::from_path(&cty_path).expect("cty fixture should load");
    let domains = DomainPack::builtin();
    let spec = ContestSpec::from_path(&spec_path).expect("cqww spec should load");

    let source = ResolvedStation::new("W", Continent::NA, true, true);
    let mut config: HashMap<String, Value> = HashMap::new();
    config.insert("my_cq_zone".to_string(), Value::Int(5));

    let mut session = SpecSession::new(spec, source, config, resolver, domains).unwrap();

    let q1 = session
        .apply_qso(Band::B20, Callsign::new("DL1ABC"), "599 14")
        .expect("first qso should apply");
    let q2 = session
        .apply_qso(Band::B15, Callsign::new("VE3EJ"), "599 04")
        .expect("second qso should apply");

    assert!(q1.claimed_score > 0);
    assert_eq!(q2.total_qsos, 2);
    assert!(q2.total_points >= q1.total_points);

    let entries: Vec<CabrilloQso> = Vec::new();
    let cab = session.export_cabrillo("N9UNX", "SINGLE-OP", &entries);
    assert!(cab.contains("START-OF-LOG"));
    assert!(cab.contains("CONTEST:"));
}
