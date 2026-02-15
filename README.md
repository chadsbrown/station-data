# station-data

`station-data` is a synchronous Rust library for contest-station reference data:

- CTY.DAT callsign resolution
- domain pack loading/provisioning for `contest-engine`
- optional call history interface
- SCP lookup and suggestion (including N+1 / Damerau<=1 style suggestions)

This crate is designed for low-latency contest workflows and direct integration with local projects like:

- `contest-engine`
- logger/UI code paths that need real-time SCP suggestions

## Status

Implemented:

- CTY parser + resolver (`CtyDb`)
- `contest_engine::spec::StationResolver` implementation
- domain pack provider (`DomainPack`)
- `contest_engine::spec::DomainProvider` implementation
- optional history trait + in-memory implementation
- SCP database (`ScpDb`) with:
  - exact membership (`contains`)
  - wildcard search (`search`)
  - ranked suggestions (`suggest`)
  - N+1 suggestions (`suggest_n_plus_one`, Damerau-Levenshtein <= 1)
- snapshot/versioned facade (`StationDataFacade`) with metrics

## CTY Format Scope

This parser intentionally targets the **contest-oriented / N1MM-style CTY.DAT** format.

It is **not** a full "Big CTY.DAT" parser for every logging-focused variant.

Supported CTY features:

- country headers (`:`-delimited)
- country prefix/call blocks terminated by `;`
- wildcard prefixes (`*`)
- exact-call entries (`=CALL`)
- comment lines beginning with `#` (including wrapped comment blocks)
- per-entry overrides:
  - `(cq)`
  - `[itu]`
  - `{continent}`

## Library Layout

- `src/cty.rs`: CTY parsing, indexing, lookup, `StationResolver` impl
- `src/domains.rs`: domain loading/embedding, `DomainProvider` impl
- `src/history.rs`: optional history trait + in-memory implementation
- `src/scp.rs`: SCP loader, indexes, search/suggest APIs
- `src/service.rs`: facade, snapshot/versioning, metrics, reload support
- `src/contracts.rs`: canonical schema helpers (DXCC, continent, domain value rules)
- `src/normalize.rs`: callsign normalization helpers

## Core Types

### CTY

- `CtyDb`
  - `from_path`, `from_reader`
  - `lookup(&str) -> Option<ResolvedStation>`
- `ResolvedStation`
  - `dxcc`, `continent`, `cq_zone`, `itu_zone`, `is_wve`, `is_na`

### Domains

- `DomainPack`
  - `builtin()`
  - `from_dir(path)`
  - `values(name)`

Built-in domains are embedded from `data/domains/`:

- `dxcc_entities`
- `arrl_dx_wve_multipliers`
- `naqp_multipliers`

### History

- `HistoryProvider` trait
- `HistoryHint`
- `InMemoryHistory`

### SCP

- `ScpDb`
  - `from_reader`
- `SuperCheck` trait methods:
  - `contains(call)`
  - `search(pattern, max_results)`
  - `suggest(partial, max_results)`
  - `suggest_with_context(partial, max_results, ctx)`
  - `suggest_n_plus_one(partial, max_results)`

- `ScpSuggestContext`
  - optional reranking boosts from app context:
    - `needed_mults`
    - `recent_worked`
    - `recent_spots`
    - `history_hits`

## Integration with contest-engine

`station-data` directly implements:

- `contest_engine::spec::StationResolver` for `CtyDb` and `StationDataFacade`
- `contest_engine::spec::DomainProvider` for `DomainPack` and `StationDataFacade`

### Minimal session wiring

```rust
use contest_engine::spec::{ContestSpec, ResolvedStation, SpecSession, Value};
use contest_engine::types::{Band, Callsign, Continent};
use station_data::{CtyDb, DomainPack};
use std::collections::HashMap;

let resolver = CtyDb::from_path("./wl_cty.dat".as_ref())?;
let domains = DomainPack::builtin();
let spec = ContestSpec::from_path("../contest-engine/specs/cqww_cw.json")?;

let source = ResolvedStation::new("W", Continent::NA, true, true);
let mut config = HashMap::new();
config.insert("my_cq_zone".to_string(), Value::Int(5));

let mut session = SpecSession::new(spec, source, config, resolver, domains)?;
let _ = session.apply_qso(Band::B20, Callsign::new("DL1ABC"), "599 14")?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

## StationDataFacade

`StationDataFacade` is a synchronous top-level provider for CTY/domains/history.

- Snapshot-based data model (`Arc` + swap on reload)
- Version metadata with fingerprints
- Metrics counters for resolve/domain/history calls and hit/miss counts

### Loading

```rust
use station_data::{DomainSource, StationDataFacade};

let facade = StationDataFacade::load_from_paths(
    "./wl_cty.dat".as_ref(),
    DomainSource::Builtin,
    None,
)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

### Reloading

```rust
# use station_data::{DomainSource, StationDataFacade};
# let facade = StationDataFacade::load_from_paths("./wl_cty.dat".as_ref(), DomainSource::Builtin, None)?;
facade.reload_from_paths(
    "./wl_cty.dat".as_ref(),
    DomainSource::Builtin,
    None,
)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

## CLI Tool

This repo includes `station_data_tool` for local verification and benchmarks.

Run:

```bash
cargo run --bin station_data_tool -- <command> ...
```

Commands:

- `resolve-cty <cty.dat> <callsign>`
- `bench-cty <cty.dat> [iterations]`
- `bench-cty-calls <cty.dat> <calls.txt> [passes]`
- `check-scp <scp.txt> <callsign>`
- `suggest-scp <scp.txt> <partial> [max_results]`
- `suggest-scp-n1 <scp.txt> <partial> [max_results]`
- `bench-scp-contains <scp.txt> <calls.txt> [passes]`
- `bench-scp-search <scp.txt> <patterns.txt> <max_results> [passes]`
- `bench-scp-suggest <scp.txt> <partials.txt> <max_results> [passes]`
- `bench-scp-n1 <scp.txt> <partials.txt> <max_results> [passes]`
- `facade-status <cty.dat> <builtin|domains_dir> [call] [domain]`

### Example

```bash
cargo run --bin station_data_tool -- resolve-cty ./wl_cty.dat N9UNX
cargo run --bin station_data_tool -- suggest-scp MASTER.SCP N9U 10
cargo run --bin station_data_tool -- suggest-scp-n1 MASTER.SCP N9VNX 10
cargo run --bin station_data_tool -- facade-status ./wl_cty.dat builtin N9UNX dxcc_entities
```

## Performance Notes

- CTY lookups are in-memory indexed prefix matches.
- SCP suggestions are indexed for low-latency UI use.
- N+1 suggestions use candidate indexes + exact Damerau<=1 verification.

Use the included benchmark commands to profile on your own files/hardware.

## Design Choices

- Synchronous by design (no runtime dependency on async executors).
- SCP suggestion APIs are kept separate from the facade’s scoring/provider role.
- Canonical schema helpers enforce shared rules for DXCC/continent/domain values.

## Development

Build/test:

```bash
cargo test
```

Key test assets are in `tests/fixtures/`.

## License

No license file is currently included in this repository.
