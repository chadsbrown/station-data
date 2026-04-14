# Call History Module Improvements

## Motivation

The `station-data` history module currently provides only a bare-bones
`InMemoryHistory` with a fixed four-field `HistoryHint` struct (`call`, `name`,
`loc`, `cq_zone`).  Meanwhile, clogger's `logger-runtime` contains a
full-featured `.ch` file parser and generic key-value record store that handles
arbitrary contest-specific columns.  Moving those capabilities into
`station-data` makes the history module reusable by any contest logger, not just
clogger.

This plan covers three areas: generalizing the data model, adding `.ch` file
parsing, and wiring the improvements through the existing service facade/SCP
integration.

---

## Current State

### station-data (`src/history.rs`)

- `HistoryHint` — fixed struct: `call`, `name: Option<String>`,
  `loc: Option<String>`, `cq_zone: Option<u8>`
- `HistoryProvider` trait — `fn lookup(&self, call: &str) -> Option<HistoryHint>`
  (requires `Send + Sync`)
- `InMemoryHistory` — `HashMap<String, HistoryHint>`, manual `insert()`, no file
  loading
- One unit test

### clogger (`logger-runtime/src/call_history.rs`)

- `CallHistoryDb` — `HashMap<String, HashMap<String, String>>` storing arbitrary
  column-name/value pairs parsed from N1MM `.ch` files
- Full `!!Order!!` header parsing, comment/blank-line skipping, RFC 4180 quoted
  CSV handling via the `csv` crate
- `CallHistoryLookup` trait returns `Option<Vec<(String, String)>>` — generic
  pairs, not a fixed struct
- Contest-specific `history_field_mapping()` bridges column names to form fields
- Six unit tests covering: basic parse, exact hit/miss, trailing-comma headers,
  comments/blanks, quoted fields with embedded commas

### Gap summary

| Capability | station-data | clogger |
|---|---|---|
| Arbitrary columns | no (4 fixed fields) | yes (HashMap<String, String>) |
| `.ch` file parser | none | full (csv crate, RFC 4180) |
| File loading | none | `CallHistoryDb::load(path)` |
| Parse from string | none | `CallHistoryDb::parse(content)` |
| Thread safety | `Send + Sync` on trait | `Send + Sync` via `Box<dyn>` |
| Test coverage | 1 test | 6 tests |
| Facade integration | metrics + optional provider | n/a (app-level wiring) |

---

## Plan

### Phase 1 — Generalize the data model

Replace the fixed `HistoryHint` struct with a generic record type that can carry
arbitrary columns, matching what `.ch` files actually contain.

#### 1a. New `HistoryRecord` type

```rust
/// A single call-history record with arbitrary column data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryRecord {
    pub call: String,
    /// Column-name / value pairs, e.g. [("Name", "ALICE"), ("CqZone", "5")].
    /// Column names preserve the casing from the source file header.
    pub fields: Vec<(String, String)>,
}
```

Using `Vec<(String, String)>` rather than `HashMap` keeps insertion order
(matching the `.ch` column order) and is consistent with the return type that
clogger's `CallHistoryLookup` already uses.  A convenience method
`get(&self, col: &str) -> Option<&str>` should provide case-insensitive
field lookup.

#### 1b. Update `HistoryProvider` trait

```rust
pub trait HistoryProvider: Send + Sync {
    fn lookup(&self, call: &str) -> Option<HistoryRecord>;
}
```

The return type changes from `HistoryHint` to `HistoryRecord`.

#### 1c. Deprecation path for `HistoryHint`

Keep `HistoryHint` temporarily with a `From<&HistoryRecord>` impl that maps
well-known column names to the struct fields:

- `"Name"` -> `name`
- `"Loc"` / `"State"` / `"Sect"` -> `loc` (first match wins)
- `"CqZone"` -> `cq_zone` (parsed as `u8`)

This lets any downstream code that currently pattern-matches on `HistoryHint`
migrate at its own pace.  Remove `HistoryHint` in a follow-up once all consumers
have migrated.

#### 1d. Update `InMemoryHistory`

Change internal storage to `HashMap<String, HistoryRecord>`.  The existing
`insert()` method signature changes to accept `HistoryRecord`.

#### Files changed

- `src/history.rs` — new types, updated trait, updated `InMemoryHistory`

---

### Phase 2 — Add `.ch` file parser

Port clogger's `.ch` parsing logic into station-data as a standalone loader that
produces an `InMemoryHistory`.

#### 2a. New `ChFileParser`

Add a parser (in `src/history.rs` or a new `src/history/ch_parser.rs` submodule)
that converts `.ch` file content into records:

```rust
impl InMemoryHistory {
    /// Parse an N1MM-format `.ch` file from a string.
    pub fn from_ch_content(content: &str) -> Result<Self, HistoryError>;

    /// Load an N1MM-format `.ch` file from disk.
    pub fn from_ch_file(path: &Path) -> Result<Self, HistoryError>;
}
```

Parsing rules (matching clogger's implementation):

1. Skip blank lines and lines starting with `#`.
2. The `!!Order!!` sentinel line defines column names. Everything after the
   sentinel (comma-separated) is a column header. The `Call` column is
   identified case-insensitively and used as the record key.
3. Subsequent lines are data rows. Fields are parsed as RFC 4180 CSV (quoted
   fields may contain commas).
4. Callsigns are normalized to uppercase.
5. Empty field values are omitted from the record.
6. A data line before the `!!Order!!` header is an error.

#### 2b. New `HistoryError` type

```rust
#[derive(Debug, thiserror::Error)]
pub enum HistoryError {
    #[error("data line before !!Order!! header")]
    MissingHeader,
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("CSV parse error: {0}")]
    Csv(#[from] csv::Error),
}
```

#### 2c. Add `csv` dependency

Add `csv = "1"` to `Cargo.toml`.  This is a small, well-maintained crate
(already used by clogger) and avoids reimplementing RFC 4180 quoting.

#### 2d. Port tests from clogger

Bring over all six test cases from clogger's `call_history.rs`:

- `parse_basic` — 4 records parsed correctly
- `exact_lookup_hit` — verify all fields returned
- `exact_lookup_miss` — `None` for unknown callsign
- `trailing_comma_in_header` — extra trailing comma handled
- `comments_and_blanks_ignored` — `#` lines and empty lines skipped
- `quoted_field_with_comma_preserves_alignment` — embedded commas don't shift
  columns

Add additional tests:

- Multiple `!!Order!!` lines (second one resets columns — or error; decide on
  semantics)
- Windows-style `\r\n` line endings
- Mixed-case `Call` header (e.g., `CALL`, `call`)
- Empty file (no header, no data) — returns empty history, not an error

#### Files changed

- `src/history.rs` (or new `src/history/` submodule)
- `Cargo.toml` — add `csv` dependency

---

### Phase 3 — Wire through the facade and SCP

#### 3a. Update `StationDataFacade`

`StationDataFacade::history_lookup()` currently returns `Option<HistoryHint>`.
Change it to return `Option<HistoryRecord>`.  The metrics tracking
(`history_calls`, `history_hits`, `history_misses`) stays as-is.

#### 3b. Update `StationDataSnapshot`

The `history: Option<Arc<dyn HistoryProvider>>` field already uses the trait, so
it picks up the new return type automatically once the trait is updated.

#### 3c. Convenience loader on facade

Add a helper so callers don't have to construct an `InMemoryHistory` and wrap it
in `Arc` themselves:

```rust
impl StationDataFacade {
    pub fn load_from_paths_with_ch(
        cty_path: &Path,
        domains: DomainSource,
        ch_path: Option<&Path>,
    ) -> Result<Self> {
        let history: Option<Arc<dyn HistoryProvider>> = match ch_path {
            Some(p) => Some(Arc::new(InMemoryHistory::from_ch_file(p)?)),
            None => None,
        };
        Self::load_from_paths(cty_path, domains, history)
    }
}
```

The existing `load_from_paths()` signature is unchanged — callers who provide
their own `HistoryProvider` impl still pass it directly.

#### 3d. Auto-populate `ScpSuggestContext.history_hits`

Currently, applications must manually populate `ScpSuggestContext.history_hits`
with callsigns they've seen in history.  Add an optional convenience method on
the facade:

```rust
impl StationDataFacade {
    /// Returns the set of all callsigns in the loaded history database.
    /// Useful for populating `ScpSuggestContext.history_hits`.
    pub fn history_callsigns(&self) -> HashSet<String>;
}
```

This requires adding a method to the `HistoryProvider` trait:

```rust
pub trait HistoryProvider: Send + Sync {
    fn lookup(&self, call: &str) -> Option<HistoryRecord>;

    /// Return all known callsigns (for SCP boost scoring).
    /// Default impl returns empty set.
    fn callsigns(&self) -> HashSet<String> {
        HashSet::new()
    }
}
```

`InMemoryHistory` overrides this to return its keyset.

#### 3e. Update CLI tool

Add a `history` subcommand to `station_data_tool`:

```
station_data_tool history <file.ch> lookup <CALL>
station_data_tool history <file.ch> stats
```

- `lookup` — prints the record for a callsign (or "not found")
- `stats` — prints record count and column names from the header

This makes the `.ch` parser testable from the command line without needing
clogger.

#### Files changed

- `src/service.rs` — return type, convenience loader, callsigns method
- `src/history.rs` — `callsigns()` default method on trait, override on
  `InMemoryHistory`
- `src/bin/station_data_tool.rs` — new `history` subcommand
- `src/lib.rs` — export `HistoryRecord`, `HistoryError`

---

### Phase 4 — Migrate clogger to use station-data's history

Once phases 1-3 land, clogger can drop its own `.ch` parser and history store.

#### 4a. Replace `CallHistoryDb` with station-data

In `logger-runtime/src/call_history.rs`, replace the custom implementation with
a thin adapter:

```rust
use station_data::{InMemoryHistory, HistoryRecord};

pub struct CallHistoryDb {
    inner: InMemoryHistory,
}

impl CallHistoryDb {
    pub fn load(path: &Path) -> Result<Self> {
        let inner = InMemoryHistory::from_ch_file(path)?;
        Ok(Self { inner })
    }
}

impl CallHistoryLookup for CallHistoryDb {
    fn lookup(&self, call_norm: &str) -> Option<Vec<(String, String)>> {
        self.inner.lookup(call_norm).map(|rec| rec.fields)
    }
}
```

This is structurally identical to how `ScpDb` already wraps
`station_data::ScpDb`.

#### 4b. Remove duplicated code

Delete the `parse()`, `parse_csv_line()` functions and associated tests from
clogger.  The tests now live in station-data.

#### 4c. Remove `csv` dependency from clogger

`csv` moves to station-data only.  Remove it from
`logger-runtime/Cargo.toml`.

#### Files changed

- `logger-runtime/src/call_history.rs` — thin wrapper
- `logger-runtime/Cargo.toml` — remove `csv`

---

## Implementation Order and Dependencies

```
Phase 1 (data model)
  |
  v
Phase 2 (parser)      -- can develop parser tests in parallel with phase 1
  |                       once the data model is settled
  v
Phase 3 (facade/SCP)  -- depends on phases 1+2
  |
  v
Phase 4 (clogger migration) -- depends on phase 3
```

Phases 1 and 2 can be developed together in one PR.  Phase 3 is a separate PR.
Phase 4 is a clogger-side PR that bumps the station-data dependency.

## Risks and Considerations

- **Breaking change to `HistoryProvider` trait**: Any code implementing the trait
  outside of station-data will need to update.  The `HistoryHint` compat shim
  (phase 1c) softens the blow for consumers that only *read* history results, but
  implementors must update their `lookup()` return type.  Since station-data is
  not yet published to crates.io and the known consumers are limited, this is
  acceptable.

- **`csv` crate addition**: Adds a new dependency to station-data.  The `csv`
  crate is pure Rust, well-maintained, and small.  The alternative (hand-rolling
  RFC 4180 parsing) is error-prone and not worth the dep savings.

- **Column name conventions**: Different `.ch` files may use different column
  names for the same concept (e.g., `Sect` vs `Section`, `State` vs `Loc`).
  The parser should preserve column names as-is; normalization/aliasing is a
  consuming-application concern (via their field mappings), not the parser's job.
