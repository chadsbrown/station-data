use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryHint {
    pub call: String,
    pub name: Option<String>,
    pub loc: Option<String>,
    pub cq_zone: Option<u8>,
}

pub trait HistoryProvider: Send + Sync {
    fn lookup(&self, call: &str) -> Option<HistoryHint>;
}

#[derive(Debug, Default, Clone)]
pub struct InMemoryHistory {
    map: HashMap<String, HistoryHint>,
}

impl InMemoryHistory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, hint: HistoryHint) {
        self.map.insert(hint.call.to_ascii_uppercase(), hint);
    }
}

impl HistoryProvider for InMemoryHistory {
    fn lookup(&self, call: &str) -> Option<HistoryHint> {
        self.map.get(&call.trim().to_ascii_uppercase()).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_memory_lookup_works() {
        let mut hist = InMemoryHistory::new();
        hist.insert(HistoryHint {
            call: "K1ABC".to_string(),
            name: Some("ALICE".to_string()),
            loc: Some("MA".to_string()),
            cq_zone: Some(5),
        });

        let got = hist.lookup("k1abc").unwrap();
        assert_eq!(got.name.as_deref(), Some("ALICE"));
        assert_eq!(got.loc.as_deref(), Some("MA"));
        assert_eq!(got.cq_zone, Some(5));
    }
}
