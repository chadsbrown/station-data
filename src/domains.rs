use crate::contracts::canonical_domain_value;
use contest_engine::spec::DomainProvider;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Clone, Default)]
pub struct DomainPack {
    map: HashMap<String, Arc<[String]>>,
}

#[derive(Debug, Error)]
pub enum DomainError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

impl DomainPack {
    pub fn from_dir(path: &Path) -> Result<Self, DomainError> {
        let mut map = HashMap::new();
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if !file_type.is_file() {
                continue;
            }

            let p = entry.path();
            let Some(stem) = p.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };

            let text = std::fs::read_to_string(&p)?;
            map.insert(stem.to_string(), parse_domain_values(&text));
        }

        Ok(Self { map })
    }

    pub fn builtin() -> Self {
        let mut map = HashMap::new();
        map.insert(
            "dxcc_entities".to_string(),
            parse_domain_values(include_str!("../data/domains/dxcc_entities.txt")),
        );
        map.insert(
            "arrl_dx_wve_multipliers".to_string(),
            parse_domain_values(include_str!(
                "../data/domains/arrl_dx_wve_multipliers.txt"
            )),
        );
        map.insert(
            "naqp_multipliers".to_string(),
            parse_domain_values(include_str!("../data/domains/naqp_multipliers.txt")),
        );
        Self { map }
    }

    pub fn values(&self, name: &str) -> Option<Arc<[String]>> {
        self.map.get(name).cloned()
    }
}

impl DomainProvider for DomainPack {
    fn values(&self, domain_name: &str) -> Option<Arc<[String]>> {
        self.values(domain_name)
    }
}

fn parse_domain_values(text: &str) -> Arc<[String]> {
    let values: Vec<String> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(canonical_domain_value)
        .collect();
    Arc::<[String]>::from(values)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_pack_has_expected_domains() {
        let domains = DomainPack::builtin();

        let dxcc = domains.values("dxcc_entities").unwrap();
        assert!(!dxcc.is_empty());
        assert_eq!(dxcc.first().unwrap(), "1A");
        assert_eq!(dxcc.last().unwrap(), "ZS8");

        let arrl = domains.values("arrl_dx_wve_multipliers").unwrap();
        assert!(!arrl.is_empty());
        assert_eq!(arrl.first().unwrap(), "AL");

        let naqp = domains.values("naqp_multipliers").unwrap();
        assert!(!naqp.is_empty());
        assert_eq!(naqp.first().unwrap(), "4U1U");
        assert!(naqp.iter().any(|v| v == "MA"));
    }
}
