use crate::cty::{CtyDb, CtyError};
use crate::domains::{DomainError, DomainPack};
use crate::history::{HistoryHint, HistoryProvider};
use contest_engine::spec::{DomainProvider, ResolvedStation as EngineResolvedStation, StationResolver};
use contest_engine::types::Callsign;
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::SystemTime;
use thiserror::Error;

#[derive(Debug, Clone)]
pub enum DomainSource {
    Builtin,
    Dir(PathBuf),
}

#[derive(Debug, Clone)]
pub struct DataVersionInfo {
    pub cty_source: String,
    pub cty_fingerprint: String,
    pub domains_source: String,
    pub domains_fingerprint: String,
    pub loaded_at: SystemTime,
}

#[derive(Clone)]
pub struct StationDataSnapshot {
    pub cty: CtyDb,
    pub domains: DomainPack,
    pub history: Option<Arc<dyn HistoryProvider>>,
    pub version: DataVersionInfo,
}

#[derive(Debug, Default)]
pub struct StationDataMetrics {
    resolve_calls: AtomicU64,
    resolve_hits: AtomicU64,
    resolve_misses: AtomicU64,
    domain_calls: AtomicU64,
    domain_hits: AtomicU64,
    domain_misses: AtomicU64,
    history_calls: AtomicU64,
    history_hits: AtomicU64,
    history_misses: AtomicU64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StationDataMetricsSnapshot {
    pub resolve_calls: u64,
    pub resolve_hits: u64,
    pub resolve_misses: u64,
    pub domain_calls: u64,
    pub domain_hits: u64,
    pub domain_misses: u64,
    pub history_calls: u64,
    pub history_hits: u64,
    pub history_misses: u64,
}

#[derive(Debug, Error)]
pub enum StationDataError {
    #[error("cty load failed: {0}")]
    Cty(#[from] CtyError),
    #[error("domain load failed: {0}")]
    Domain(#[from] DomainError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub struct StationDataFacade {
    snapshot: RwLock<Arc<StationDataSnapshot>>,
    metrics: StationDataMetrics,
}

impl StationDataMetrics {
    pub fn snapshot(&self) -> StationDataMetricsSnapshot {
        StationDataMetricsSnapshot {
            resolve_calls: self.resolve_calls.load(Ordering::Relaxed),
            resolve_hits: self.resolve_hits.load(Ordering::Relaxed),
            resolve_misses: self.resolve_misses.load(Ordering::Relaxed),
            domain_calls: self.domain_calls.load(Ordering::Relaxed),
            domain_hits: self.domain_hits.load(Ordering::Relaxed),
            domain_misses: self.domain_misses.load(Ordering::Relaxed),
            history_calls: self.history_calls.load(Ordering::Relaxed),
            history_hits: self.history_hits.load(Ordering::Relaxed),
            history_misses: self.history_misses.load(Ordering::Relaxed),
        }
    }
}

impl StationDataFacade {
    pub fn new(snapshot: StationDataSnapshot) -> Self {
        Self {
            snapshot: RwLock::new(Arc::new(snapshot)),
            metrics: StationDataMetrics::default(),
        }
    }

    pub fn load_from_paths(
        cty_path: &Path,
        domains: DomainSource,
        history: Option<Arc<dyn HistoryProvider>>,
    ) -> Result<Self, StationDataError> {
        let snapshot = load_snapshot(cty_path, domains, history)?;
        Ok(Self::new(snapshot))
    }

    pub fn reload_from_paths(
        &self,
        cty_path: &Path,
        domains: DomainSource,
        history: Option<Arc<dyn HistoryProvider>>,
    ) -> Result<(), StationDataError> {
        let next = Arc::new(load_snapshot(cty_path, domains, history)?);
        let mut guard = self.snapshot.write().expect("station-data lock poisoned");
        *guard = next;
        Ok(())
    }

    pub fn snapshot(&self) -> Arc<StationDataSnapshot> {
        self.snapshot
            .read()
            .expect("station-data lock poisoned")
            .clone()
    }

    pub fn version(&self) -> DataVersionInfo {
        self.snapshot().version.clone()
    }

    pub fn metrics(&self) -> StationDataMetricsSnapshot {
        self.metrics.snapshot()
    }

    pub fn resolve_call(&self, call: &str) -> Option<crate::cty::ResolvedStation> {
        self.metrics.resolve_calls.fetch_add(1, Ordering::Relaxed);
        let snapshot = self.snapshot();
        let hit = snapshot.cty.lookup(call);
        if hit.is_some() {
            self.metrics.resolve_hits.fetch_add(1, Ordering::Relaxed);
        } else {
            self.metrics.resolve_misses.fetch_add(1, Ordering::Relaxed);
        }
        hit
    }

    pub fn domain_values(&self, domain_name: &str) -> Option<Arc<[String]>> {
        self.metrics.domain_calls.fetch_add(1, Ordering::Relaxed);
        let snapshot = self.snapshot();
        let hit = snapshot.domains.values(domain_name);
        if hit.is_some() {
            self.metrics.domain_hits.fetch_add(1, Ordering::Relaxed);
        } else {
            self.metrics.domain_misses.fetch_add(1, Ordering::Relaxed);
        }
        hit
    }

    pub fn history_lookup(&self, call: &str) -> Option<HistoryHint> {
        self.metrics.history_calls.fetch_add(1, Ordering::Relaxed);
        let snapshot = self.snapshot();
        let hit = snapshot.history.as_ref().and_then(|h| h.lookup(call));
        if hit.is_some() {
            self.metrics.history_hits.fetch_add(1, Ordering::Relaxed);
        } else {
            self.metrics.history_misses.fetch_add(1, Ordering::Relaxed);
        }
        hit
    }
}

impl StationResolver for StationDataFacade {
    fn resolve(&self, call: &Callsign) -> Result<EngineResolvedStation, String> {
        self.metrics.resolve_calls.fetch_add(1, Ordering::Relaxed);
        let snapshot = self.snapshot();
        let result = snapshot.cty.resolve(call);
        if result.is_ok() {
            self.metrics.resolve_hits.fetch_add(1, Ordering::Relaxed);
        } else {
            self.metrics.resolve_misses.fetch_add(1, Ordering::Relaxed);
        }
        result
    }
}

impl DomainProvider for StationDataFacade {
    fn values(&self, domain_name: &str) -> Option<Arc<[String]>> {
        self.domain_values(domain_name)
    }
}

fn load_snapshot(
    cty_path: &Path,
    domains: DomainSource,
    history: Option<Arc<dyn HistoryProvider>>,
) -> Result<StationDataSnapshot, StationDataError> {
    let cty = CtyDb::from_path(cty_path)?;
    let cty_fingerprint = fingerprint_file(cty_path)?;
    let cty_source = cty_path.display().to_string();

    let (domains_pack, domains_source, domains_fingerprint) = match domains {
        DomainSource::Builtin => (
            DomainPack::builtin(),
            "builtin".to_string(),
            "builtin".to_string(),
        ),
        DomainSource::Dir(path) => {
            let fp = fingerprint_dir(&path)?;
            let pack = DomainPack::from_dir(&path)?;
            (pack, path.display().to_string(), fp)
        }
    };

    Ok(StationDataSnapshot {
        cty,
        domains: domains_pack,
        history,
        version: DataVersionInfo {
            cty_source,
            cty_fingerprint,
            domains_source,
            domains_fingerprint,
            loaded_at: SystemTime::now(),
        },
    })
}

fn fingerprint_file(path: &Path) -> Result<String, std::io::Error> {
    let bytes = fs::read(path)?;
    let mut hasher = DefaultHasher::new();
    path.to_string_lossy().hash(&mut hasher);
    bytes.hash(&mut hasher);
    Ok(format!("{:016x}", hasher.finish()))
}

fn fingerprint_dir(path: &Path) -> Result<String, std::io::Error> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            entries.push(entry.path());
        }
    }
    entries.sort();

    let mut hasher = DefaultHasher::new();
    path.to_string_lossy().hash(&mut hasher);
    for p in entries {
        p.to_string_lossy().hash(&mut hasher);
        fs::read(&p)?.hash(&mut hasher);
    }
    Ok(format!("{:016x}", hasher.finish()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn facade_tracks_metrics_and_versions() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let cty = root.join("tests/fixtures/cty_sample.dat");
        let facade = StationDataFacade::load_from_paths(&cty, DomainSource::Builtin, None).unwrap();

        assert!(facade.resolve_call("K1ABC").is_some());
        assert!(facade.domain_values("dxcc_entities").is_some());
        assert!(facade.history_lookup("N9UNX").is_none());

        let m = facade.metrics();
        assert_eq!(m.resolve_calls, 1);
        assert_eq!(m.resolve_hits, 1);
        assert_eq!(m.domain_calls, 1);
        assert_eq!(m.domain_hits, 1);
        assert_eq!(m.history_calls, 1);
        assert_eq!(m.history_misses, 1);

        let v = facade.version();
        assert!(v.cty_source.ends_with("tests/fixtures/cty_sample.dat"));
        assert_eq!(v.domains_source, "builtin");
    }
}
