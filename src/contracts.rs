//! Shared schema contracts for station-data consumers.
//!
//! These helpers enforce canonical text forms used across crates:
//! - DXCC ids are uppercase and normalized (e.g. `K` -> `W`)
//! - continent codes use the 2-letter set `NA SA EU AF AS OC AN`
//! - domain values are uppercase and trimmed

pub const CONTINENT_CODES: &[&str] = &["NA", "SA", "EU", "AF", "AS", "OC", "AN"];

pub fn canonical_dxcc_id(raw: &str) -> String {
    let norm = raw.trim().to_ascii_uppercase();
    if norm == "K" {
        "W".to_string()
    } else {
        norm
    }
}

pub fn canonical_domain_value(raw: &str) -> String {
    raw.trim().to_ascii_uppercase()
}

pub fn is_valid_continent_code(raw: &str) -> bool {
    let code = raw.trim().to_ascii_uppercase();
    CONTINENT_CODES.iter().any(|v| *v == code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonicalizes_dxcc_k_to_w() {
        assert_eq!(canonical_dxcc_id("K"), "W");
        assert_eq!(canonical_dxcc_id("ve"), "VE");
    }

    #[test]
    fn validates_continents() {
        assert!(is_valid_continent_code("na"));
        assert!(is_valid_continent_code("EU"));
        assert!(!is_valid_continent_code("XX"));
    }
}
