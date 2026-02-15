pub mod contracts;
pub mod cty;
pub mod domains;
pub mod history;
pub mod normalize;
pub mod scp;
pub mod service;

pub use cty::{Country, CtyDb, CtyError, ResolvedStation};
pub use domains::{DomainError, DomainPack};
pub use history::{HistoryHint, HistoryProvider, InMemoryHistory};
pub use normalize::{is_plausible_callsign, normalize_call, split_slash_candidates, strip_suffixes};
pub use scp::{ScpDb, ScpError, ScpSuggestContext, ScpSuggestion, SuperCheck};
pub use service::{
    DataVersionInfo, DomainSource, StationDataError, StationDataFacade, StationDataMetricsSnapshot,
    StationDataSnapshot,
};
