use crate::contracts::{canonical_dxcc_id, is_valid_continent_code};
use crate::normalize::{is_plausible_callsign, normalize_call, split_slash_candidates};
use contest_engine::spec::{ResolvedStation as EngineResolvedStation, StationResolver};
use contest_engine::types::{Callsign, Continent};
use std::collections::HashMap;
use std::io::Read;
use std::path::Path;
use thiserror::Error;

// NOTE:
// This parser intentionally targets the contest-oriented CTY.DAT format used by
// N1MM-style workflows. It is not a full parser for the "Big CTY.DAT" variants
// used by general logging applications.
//
// Supported scope here is the subset needed for contest station resolution:
// - country headers (':'-delimited fields)
// - comma-separated prefix/call entries ending with ';'
// - wildcard prefixes via '*'
// - exact-call entries via '='
// - comment lines beginning with '#'
// - per-entry CQ/ITU/continent overrides: '(cq)', '[itu]', '{continent}'

#[derive(Debug, Clone)]
pub struct CtyDb {
    pub countries: Vec<Country>,
    exact_calls: HashMap<String, Rule>,
    exact_prefixes: HashMap<String, Rule>,
    wildcards: Vec<Rule>,
}

#[derive(Debug, Clone)]
pub struct Country {
    pub name: String,
    pub dxcc: String,
    pub continent: String,
    pub cq_zone: Option<u8>,
    pub itu_zone: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedStation {
    pub dxcc: String,
    pub continent: String,
    pub cq_zone: Option<u8>,
    pub itu_zone: Option<u8>,
    pub is_wve: bool,
    pub is_na: bool,
}

#[derive(Debug, Clone)]
struct Rule {
    key: String,
    country_idx: usize,
    cq_zone: Option<u8>,
    itu_zone: Option<u8>,
    continent: Option<String>,
}

#[derive(Debug, Clone)]
struct ParsedToken {
    key: String,
    is_wild: bool,
    is_exact_call: bool,
    cq_zone: Option<u8>,
    itu_zone: Option<u8>,
    continent: Option<String>,
}

#[derive(Debug, Error)]
pub enum CtyError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid CTY line: {0}")]
    InvalidLine(String),
}

impl CtyDb {
    pub fn from_path(path: &Path) -> Result<Self, CtyError> {
        let file = std::fs::File::open(path)?;
        Self::from_reader(file)
    }

    pub fn from_reader<R: Read>(mut r: R) -> Result<Self, CtyError> {
        let mut text = String::new();
        r.read_to_string(&mut text)?;

        let mut countries = Vec::new();
        let mut exact_calls = HashMap::new();
        let mut exact_prefixes = HashMap::new();
        let mut wildcards = Vec::new();

        let mut current_country: Option<Country> = None;
        let mut prefix_blob = String::new();
        let mut in_comment_continuation = false;

        for raw in text.lines() {
            let line = raw.trim();
            if line.is_empty() {
                continue;
            }
            if in_comment_continuation {
                if line.ends_with(';') {
                    in_comment_continuation = false;
                }
                continue;
            }
            if line.starts_with('#') {
                in_comment_continuation = !line.ends_with(';');
                continue;
            }

            if looks_like_header(line) {
                if let Some(country) = current_country.take() {
                    finalize_country(
                        country,
                        &prefix_blob,
                        &mut countries,
                        &mut exact_calls,
                        &mut exact_prefixes,
                        &mut wildcards,
                    );
                    prefix_blob.clear();
                }

                let mut fields = line.split(':').map(str::trim);
                let name = fields.next().unwrap_or_default().to_string();
                let cq_zone = parse_opt_u8(fields.next());
                let itu_zone = parse_opt_u8(fields.next());
                let continent = fields.next().unwrap_or_default().to_ascii_uppercase();

                for _ in 0..3 {
                    let _ = fields.next();
                }

                let primary_prefix = fields.next().unwrap_or_default();
                if !primary_prefix.is_empty() {
                    prefix_blob.push_str(primary_prefix);
                    prefix_blob.push(',');
                }

                current_country = Some(Country {
                    name,
                    dxcc: normalize_prefix(primary_prefix),
                    continent,
                    cq_zone,
                    itu_zone,
                });
                continue;
            }

            if current_country.is_some() {
                prefix_blob.push_str(line);
                if line.ends_with(';') {
                    let country = current_country.take().expect("country present");
                    finalize_country(
                        country,
                        &prefix_blob,
                        &mut countries,
                        &mut exact_calls,
                        &mut exact_prefixes,
                        &mut wildcards,
                    );
                    prefix_blob.clear();
                }
            }
        }

        if let Some(country) = current_country.take() {
            finalize_country(
                country,
                &prefix_blob,
                &mut countries,
                &mut exact_calls,
                &mut exact_prefixes,
                &mut wildcards,
            );
        }

        wildcards.sort_by(|a, b| b.key.len().cmp(&a.key.len()).then_with(|| a.key.cmp(&b.key)));

        Ok(Self {
            countries,
            exact_calls,
            exact_prefixes,
            wildcards,
        })
    }

    pub fn lookup(&self, call: &str) -> Option<ResolvedStation> {
        let candidates = lookup_candidates(call);
        let mut best: Option<(&Rule, usize)> = None;

        for candidate in candidates {
            if let Some((rule, matched_len)) = self.lookup_candidate(&candidate) {
                match best {
                    None => best = Some((rule, matched_len)),
                    Some((_, best_len)) if matched_len > best_len => best = Some((rule, matched_len)),
                    _ => {}
                }
            }
        }

        best.map(|(rule, _)| {
            let country = &self.countries[rule.country_idx];
            let dxcc = canonical_dxcc(&country.dxcc);
            let continent = rule
                .continent
                .clone()
                .unwrap_or_else(|| country.continent.clone());
            let cq_zone = rule.cq_zone.or(country.cq_zone);
            let itu_zone = rule.itu_zone.or(country.itu_zone);
            ResolvedStation {
                is_wve: dxcc == "W" || dxcc == "VE",
                is_na: continent == "NA",
                dxcc,
                continent,
                cq_zone,
                itu_zone,
            }
        })
    }

    fn lookup_candidate(&self, candidate: &str) -> Option<(&Rule, usize)> {
        if let Some(rule) = self.exact_calls.get(candidate) {
            return Some((rule, candidate.len()));
        }

        for len in (1..=candidate.len()).rev() {
            let prefix = &candidate[..len];
            if let Some(rule) = self.exact_prefixes.get(prefix) {
                return Some((rule, len));
            }
        }

        for rule in &self.wildcards {
            if candidate.starts_with(&rule.key) {
                return Some((rule, rule.key.len()));
            }
        }

        None
    }
}

impl StationResolver for CtyDb {
    fn resolve(&self, call: &Callsign) -> Result<EngineResolvedStation, String> {
        let resolved = self
            .lookup(call.as_str())
            .ok_or_else(|| format!("unknown callsign {}", call.as_str()))?;

        let continent = parse_continent(&resolved.continent)
            .ok_or_else(|| format!("unknown continent {}", resolved.continent))?;

        Ok(EngineResolvedStation::new(
            resolved.dxcc,
            continent,
            resolved.is_wve,
            resolved.is_na,
        ))
    }
}

fn parse_continent(s: &str) -> Option<Continent> {
    match s.trim().to_ascii_uppercase().as_str() {
        "NA" => Some(Continent::NA),
        "SA" => Some(Continent::SA),
        "EU" => Some(Continent::EU),
        "AF" => Some(Continent::AF),
        "AS" => Some(Continent::AS),
        "OC" => Some(Continent::OC),
        "AN" => Some(Continent::AN),
        _ => None,
    }
}

fn looks_like_header(line: &str) -> bool {
    line.matches(':').count() >= 7
}

fn parse_opt_u8(s: Option<&str>) -> Option<u8> {
    s.and_then(|v| v.trim().parse::<u8>().ok())
}

fn finalize_country(
    country: Country,
    prefix_blob: &str,
    countries: &mut Vec<Country>,
    exact_calls: &mut HashMap<String, Rule>,
    exact_prefixes: &mut HashMap<String, Rule>,
    wildcards: &mut Vec<Rule>,
) {
    let idx = countries.len();
    let entries = prefix_blob
        .split(';')
        .next()
        .unwrap_or(prefix_blob)
        .split(',')
        .map(str::trim)
        .filter(|t| !t.is_empty());

    for token in entries {
        if let Some(parsed) = parse_prefix_token(token) {
            let rule = Rule {
                key: parsed.key,
                country_idx: idx,
                cq_zone: parsed.cq_zone,
                itu_zone: parsed.itu_zone,
                continent: parsed.continent,
            };
            if parsed.is_wild {
                wildcards.push(rule);
            } else if parsed.is_exact_call {
                exact_calls.insert(rule.key.clone(), rule);
            } else {
                exact_prefixes.insert(rule.key.clone(), rule);
            }
        }
    }

    countries.push(country);
}

fn parse_prefix_token(token: &str) -> Option<ParsedToken> {
    let trimmed = token.trim();
    let is_exact_call = trimmed.starts_with('=');
    let mut out = String::new();
    let mut chars = trimmed.trim_start_matches('=').chars().peekable();

    while let Some(c) = chars.peek().copied() {
        if c.is_ascii_alphanumeric() || c == '/' || c == '*' {
            out.push(c.to_ascii_uppercase());
            chars.next();
        } else {
            break;
        }
    }

    if out.is_empty() {
        return None;
    }

    let is_wild = out.contains('*');
    if is_wild {
        out = out.replace('*', "");
    }

    if out.is_empty() {
        return None;
    }

    let remainder: String = chars.collect();
    let cq_zone = find_delimited(&remainder, '(', ')').and_then(|s| s.parse::<u8>().ok());
    let itu_zone = find_delimited(&remainder, '[', ']').and_then(|s| s.parse::<u8>().ok());
    let continent = find_delimited(&remainder, '{', '}')
        .map(|s| s.to_ascii_uppercase())
        .filter(|s| is_valid_continent_code(s));

    Some(ParsedToken {
        key: out,
        is_wild,
        is_exact_call,
        cq_zone,
        itu_zone,
        continent,
    })
}

fn normalize_prefix(prefix: &str) -> String {
    let p = prefix.trim().to_ascii_uppercase();
    let stripped = p.trim_end_matches('*');
    stripped.to_string()
}

fn canonical_dxcc(dxcc: &str) -> String {
    canonical_dxcc_id(dxcc)
}

fn find_delimited(s: &str, start: char, end: char) -> Option<String> {
    let i = s.find(start)?;
    let rem = &s[(i + start.len_utf8())..];
    let j = rem.find(end)?;
    Some(rem[..j].trim().to_string())
}

fn lookup_candidates(call: &str) -> Vec<String> {
    let normalized = normalize_call(call);
    let mut out = Vec::new();

    if is_plausible_callsign(&normalized) {
        out.push(normalized.clone());
    }

    for part in split_slash_candidates(&normalized) {
        if !out.iter().any(|x| x == &part) {
            out.push(part);
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn load_sample() -> CtyDb {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cty_sample.dat");
        CtyDb::from_path(&path).unwrap()
    }

    #[test]
    fn longest_prefix_wins() {
        let db = load_sample();
        let station = db.lookup("EA8XYZ").unwrap();
        assert_eq!(station.dxcc, "EA8");
        assert_eq!(station.continent, "AF");
    }

    #[test]
    fn wildcard_match_works() {
        let db = load_sample();
        let station = db.lookup("DA1AAA").unwrap();
        assert_eq!(station.dxcc, "DL");
    }

    #[test]
    fn slash_normalization_k1abc_p() {
        let db = load_sample();
        let station = db.lookup("K1ABC/P").unwrap();
        assert_eq!(station.dxcc, "W");
    }

    #[test]
    fn ea8_nn1n_prefers_ea8_when_available() {
        let db = load_sample();
        let station = db.lookup("EA8/NN1N").unwrap();
        assert_eq!(station.dxcc, "EA8");
    }

    #[test]
    fn ignores_comment_lines_with_semicolons() {
        let data = "\
United States:05:08:NA:37.0:-95.0:5.0:K:
#   K: K0(4)[7],K5(4)[7];
   K,N,W;
";
        let db = CtyDb::from_reader(data.as_bytes()).unwrap();
        let station = db.lookup("N9UNX").unwrap();
        assert_eq!(station.dxcc, "W");
        assert!(station.is_wve);
    }

    #[test]
    fn exact_call_with_slash_beats_prefix() {
        let data = "\
China:24:44:AS:35.0:-103.0:-8.0:BY:
 BY,BG,=BA4DL/0(23)[42];
";
        let db = CtyDb::from_reader(data.as_bytes()).unwrap();
        let station = db.lookup("BA4DL/0").unwrap();
        assert_eq!(station.dxcc, "BY");
        assert_eq!(station.continent, "AS");
        assert_eq!(station.cq_zone, Some(23));
        assert_eq!(station.itu_zone, Some(42));
    }

    #[test]
    fn per_entry_continent_override_is_applied() {
        let data = "\
Demo Country:10:11:EU:0.0:0.0:0.0:D1:
 D1,DX9{OC};
";
        let db = CtyDb::from_reader(data.as_bytes()).unwrap();
        let station = db.lookup("DX9AAA").unwrap();
        assert_eq!(station.dxcc, "D1");
        assert_eq!(station.continent, "OC");
    }

    #[test]
    fn skips_hash_comment_continuation_lines() {
        let data = "\
China:24:44:AS:35.0:-103.0:-8.0:BY:
#   BY0: BY0A(23)[42],BY0B(23)[42],
      BY0C(23)[42];
   BY,BA,=BA4DL/0(23)[42];
";
        let db = CtyDb::from_reader(data.as_bytes()).unwrap();
        let station = db.lookup("BA4DL/0").unwrap();
        assert_eq!(station.dxcc, "BY");
        assert_eq!(station.cq_zone, Some(23));
        assert_eq!(station.itu_zone, Some(42));
    }
}
