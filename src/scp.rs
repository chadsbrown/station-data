use crate::normalize::{is_plausible_callsign, normalize_call};
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::Path;
use thiserror::Error;

pub trait SuperCheck: Send + Sync {
    fn search(&self, pattern: &str, max_results: usize) -> Vec<String>;
    fn contains(&self, call: &str) -> bool;
    fn suggest(&self, partial: &str, max_results: usize) -> Vec<ScpSuggestion>;
    fn suggest_with_context(
        &self,
        partial: &str,
        max_results: usize,
        ctx: &ScpSuggestContext,
    ) -> Vec<ScpSuggestion>;
    fn suggest_n_plus_one(&self, partial: &str, max_results: usize) -> Vec<ScpSuggestion>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScpSuggestion {
    pub call: String,
    pub score: i32,
    pub reason: String,
}

#[derive(Debug, Clone, Default)]
pub struct ScpSuggestContext {
    pub needed_mults: HashSet<String>,
    pub recent_worked: HashSet<String>,
    pub recent_spots: HashSet<String>,
    pub history_hits: HashSet<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ScpDb {
    calls: Vec<String>,
    exact: HashSet<String>,
    call_to_idx: HashMap<String, usize>,
    prefix_index: HashMap<String, Vec<usize>>,
    del1_index: HashMap<String, Vec<usize>>,
    swap1_index: HashMap<String, Vec<usize>>,
}

#[derive(Debug, Error)]
pub enum ScpError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

impl ScpDb {
    pub fn from_path(path: &Path) -> Result<Self, ScpError> {
        let file = std::fs::File::open(path)?;
        Self::from_reader(file)
    }

    pub fn from_reader<R: Read>(mut r: R) -> Result<Self, ScpError> {
        let mut text = String::new();
        r.read_to_string(&mut text)?;

        let mut calls = Vec::new();
        let mut exact = HashSet::new();
        let mut call_to_idx = HashMap::new();

        for raw in text.lines() {
            let norm = normalize_call(raw);
            if !is_plausible_callsign(&norm) {
                continue;
            }

            if exact.insert(norm.clone()) {
                call_to_idx.insert(norm.clone(), calls.len());
                calls.push(norm);
            }
        }

        let mut prefix_index: HashMap<String, Vec<usize>> = HashMap::new();
        let mut del1_index: HashMap<String, Vec<usize>> = HashMap::new();
        let mut swap1_index: HashMap<String, Vec<usize>> = HashMap::new();
        for (idx, call) in calls.iter().enumerate() {
            let key = prefix_key(call);
            prefix_index.entry(key).or_default().push(idx);
            for sig in delete_one_variants(call) {
                del1_index.entry(sig).or_default().push(idx);
            }
            for sig in swap_adjacent_variants(call) {
                swap1_index.entry(sig).or_default().push(idx);
            }
        }

        Ok(Self {
            calls,
            exact,
            call_to_idx,
            prefix_index,
            del1_index,
            swap1_index,
        })
    }
}

impl SuperCheck for ScpDb {
    fn search(&self, pattern: &str, max_results: usize) -> Vec<String> {
        if max_results == 0 {
            return Vec::new();
        }

        let pattern = pattern.trim().to_ascii_uppercase();
        if pattern.is_empty() {
            return Vec::new();
        }

        let candidates: Vec<usize> = if let Some(key) = pattern_prefix_key(&pattern) {
            self.prefix_index.get(&key).cloned().unwrap_or_default()
        } else {
            (0..self.calls.len()).collect()
        };

        let mut out = Vec::new();
        for idx in candidates {
            let candidate = &self.calls[idx];
            if wildcard_matches(&pattern, candidate) {
                out.push(candidate.clone());
                if out.len() >= max_results {
                    break;
                }
            }
        }

        out
    }

    fn contains(&self, call: &str) -> bool {
        let call = normalize_call(call);
        self.exact.contains(&call)
    }

    fn suggest(&self, partial: &str, max_results: usize) -> Vec<ScpSuggestion> {
        if max_results == 0 {
            return Vec::new();
        }

        let partial = partial.trim().to_ascii_uppercase();
        if partial.len() < 2 {
            return Vec::new();
        }

        let key: String = partial.chars().take(2).collect();
        let candidates: Vec<usize> = self
            .prefix_index
            .get(&key)
            .cloned()
            .unwrap_or_else(|| (0..self.calls.len()).collect());

        let mut ranked = Vec::new();
        for idx in candidates {
            let call = &self.calls[idx];
            if let Some((score, reason)) = rank_suggestion(&partial, call) {
                ranked.push(ScpSuggestion {
                    call: call.clone(),
                    score,
                    reason: reason.to_string(),
                });
            }
        }

        ranked.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.call.cmp(&b.call)));
        ranked.truncate(max_results);
        ranked
    }

    fn suggest_with_context(
        &self,
        partial: &str,
        max_results: usize,
        ctx: &ScpSuggestContext,
    ) -> Vec<ScpSuggestion> {
        let mut ranked = self.suggest(partial, max_results.saturating_mul(4).max(max_results));
        for s in &mut ranked {
            if ctx.needed_mults.contains(&s.call) {
                s.score += 350;
                s.reason.push_str("+mult");
            }
            if ctx.recent_spots.contains(&s.call) {
                s.score += 200;
                s.reason.push_str("+spot");
            }
            if ctx.history_hits.contains(&s.call) {
                s.score += 125;
                s.reason.push_str("+hist");
            }
            if ctx.recent_worked.contains(&s.call) {
                s.score -= 75;
                s.reason.push_str("+worked");
            }
        }
        ranked.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.call.cmp(&b.call)));
        ranked.truncate(max_results);
        ranked
    }

    fn suggest_n_plus_one(&self, partial: &str, max_results: usize) -> Vec<ScpSuggestion> {
        if max_results == 0 {
            return Vec::new();
        }

        let partial = partial.trim().to_ascii_uppercase();
        if partial.len() < 3 {
            return Vec::new();
        }

        let mut candidate_ids = HashSet::new();

        if let Some(&idx) = self.call_to_idx.get(&partial) {
            candidate_ids.insert(idx);
        }
        if let Some(ids) = self.del1_index.get(&partial) {
            candidate_ids.extend(ids.iter().copied());
        }
        if let Some(ids) = self.swap1_index.get(&partial) {
            candidate_ids.extend(ids.iter().copied());
        }
        for sig in delete_one_variants(&partial) {
            if let Some(ids) = self.del1_index.get(&sig) {
                candidate_ids.extend(ids.iter().copied());
            }
            if let Some(&idx) = self.call_to_idx.get(&sig) {
                candidate_ids.insert(idx);
            }
        }

        let mut ranked = Vec::new();
        for idx in candidate_ids {
            let call = &self.calls[idx];
            if let Some(dist) = damerau_levenshtein_leq1(&partial, call) {
                ranked.push(ScpSuggestion {
                    call: call.clone(),
                    score: 8_000 - dist as i32 * 100,
                    reason: "n+1".to_string(),
                });
            }
        }

        ranked.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.call.cmp(&b.call)));
        ranked.truncate(max_results);
        ranked
    }
}

fn prefix_key(call: &str) -> String {
    call.chars().take(2).collect::<String>()
}

fn pattern_prefix_key(pattern: &str) -> Option<String> {
    let literal_prefix: String = pattern
        .chars()
        .take_while(|c| *c != '*' && *c != '?')
        .collect();

    if literal_prefix.len() >= 2 {
        Some(literal_prefix.chars().take(2).collect())
    } else {
        None
    }
}

fn wildcard_matches(pattern: &str, candidate: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let s: Vec<char> = candidate.chars().collect();

    let mut pi = 0usize;
    let mut si = 0usize;
    let mut star: Option<usize> = None;
    let mut star_match = 0usize;

    while si < s.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == s[si]) {
            pi += 1;
            si += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            pi += 1;
            star_match = si;
        } else if let Some(star_pos) = star {
            pi = star_pos + 1;
            star_match += 1;
            si = star_match;
        } else {
            return false;
        }
    }

    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }

    pi == p.len()
}

fn rank_suggestion(partial: &str, candidate: &str) -> Option<(i32, &'static str)> {
    if candidate == partial {
        return Some((10_000, "exact"));
    }

    if candidate.starts_with(partial) {
        let diff = (candidate.len() as i32 - partial.len() as i32).abs();
        return Some((9_000 - diff, "prefix"));
    }

    if let Some(gap_penalty) = subsequence_gap_penalty(partial, candidate) {
        return Some((7_000 - gap_penalty as i32, "subsequence"));
    }

    if let Some(distance) = bounded_edit_distance(partial, candidate, 1) {
        return Some((6_000 - distance as i32 * 100, "edit"));
    }

    if candidate.contains(partial) {
        return Some((5_000, "contains"));
    }

    None
}

fn subsequence_gap_penalty(needle: &str, haystack: &str) -> Option<usize> {
    let n: Vec<char> = needle.chars().collect();
    let h: Vec<char> = haystack.chars().collect();
    if n.is_empty() {
        return Some(0);
    }

    let mut i = 0usize;
    let mut first = None;
    let mut last = None;
    for (idx, ch) in h.iter().enumerate() {
        if *ch == n[i] {
            if first.is_none() {
                first = Some(idx);
            }
            last = Some(idx);
            i += 1;
            if i == n.len() {
                break;
            }
        }
    }

    if i != n.len() {
        return None;
    }

    let f = first.unwrap_or(0);
    let l = last.unwrap_or(f);
    let span = l.saturating_sub(f) + 1;
    Some(span.saturating_sub(n.len()))
}

fn bounded_edit_distance(a: &str, b: &str, max_dist: usize) -> Option<usize> {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (n, m) = (a.len(), b.len());

    if n.abs_diff(m) > max_dist {
        return None;
    }

    let mut prev: Vec<usize> = (0..=m).collect();
    let mut cur = vec![0usize; m + 1];

    for i in 1..=n {
        cur[0] = i;
        let mut row_min = cur[0];
        for j in 1..=m {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
            row_min = row_min.min(cur[j]);
        }
        if row_min > max_dist {
            return None;
        }
        std::mem::swap(&mut prev, &mut cur);
    }

    let dist = prev[m];
    if dist <= max_dist { Some(dist) } else { None }
}

fn damerau_levenshtein_leq1(a: &str, b: &str) -> Option<usize> {
    if a == b {
        return Some(0);
    }

    let ac: Vec<char> = a.chars().collect();
    let bc: Vec<char> = b.chars().collect();
    let la = ac.len();
    let lb = bc.len();

    if la.abs_diff(lb) > 1 {
        return None;
    }

    // Levenshtein <= 1 (insert/delete/substitute)
    if bounded_edit_distance(a, b, 1).is_some() {
        return Some(1);
    }

    // Adjacent transposition (Damerau)
    if la == lb {
        let mut diffs = Vec::new();
        for i in 0..la {
            if ac[i] != bc[i] {
                diffs.push(i);
                if diffs.len() > 2 {
                    return None;
                }
            }
        }
        if diffs.len() == 2 {
            let i = diffs[0];
            let j = diffs[1];
            if j == i + 1 && ac[i] == bc[j] && ac[j] == bc[i] {
                return Some(1);
            }
        }
    }

    None
}

fn delete_one_variants(s: &str) -> Vec<String> {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= 1 {
        return Vec::new();
    }

    let mut out = Vec::with_capacity(chars.len());
    for i in 0..chars.len() {
        let mut v = String::with_capacity(chars.len() - 1);
        for (j, ch) in chars.iter().enumerate() {
            if i != j {
                v.push(*ch);
            }
        }
        out.push(v);
    }
    out
}

fn swap_adjacent_variants(s: &str) -> Vec<String> {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= 1 {
        return Vec::new();
    }

    let mut out = Vec::with_capacity(chars.len() - 1);
    for i in 0..(chars.len() - 1) {
        let mut v = chars.clone();
        v.swap(i, i + 1);
        out.push(v.into_iter().collect());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn load_sample() -> ScpDb {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/scp_sample.txt");
        ScpDb::from_path(&path).unwrap()
    }

    #[test]
    fn search_k1_wildcard() {
        let db = load_sample();
        let hits = db.search("K1*", 10);
        assert!(hits.contains(&"K1ABC".to_string()));
        assert!(hits.contains(&"K1AR".to_string()));
    }

    #[test]
    fn search_question_mark() {
        let db = load_sample();
        let hits = db.search("N?1N", 10);
        assert!(hits.contains(&"NN1N".to_string()));
    }

    #[test]
    fn contains_works() {
        let db = load_sample();
        assert!(db.contains("K1ABC"));
        assert!(!db.contains("ZZ9ZZ"));
    }

    #[test]
    fn suggest_exact_then_prefix() {
        let db = load_sample();
        let hits = db.suggest("K1A", 5);
        assert_eq!(hits[0].call, "K1AR");
        assert_eq!(hits[0].reason, "prefix".to_string());

        let exact = db.suggest("K1ABC", 5);
        assert_eq!(exact[0].call, "K1ABC");
        assert_eq!(exact[0].reason, "exact".to_string());
    }

    #[test]
    fn suggest_supports_fuzzy_partial() {
        let db = load_sample();
        let hits = db.suggest("NN1", 5);
        assert!(hits.iter().any(|h| h.call == "NN1N"));
    }

    #[test]
    fn suggest_n_plus_one_requires_three_chars() {
        let db = load_sample();
        assert!(db.suggest_n_plus_one("K1", 10).is_empty());
    }

    #[test]
    fn suggest_n_plus_one_finds_edit_distance_one() {
        let data = "N9UNX\nK1ABC\n";
        let db = ScpDb::from_reader(data.as_bytes()).unwrap();
        let hits = db.suggest_n_plus_one("N9VNX", 10);
        assert!(hits.iter().any(|h| h.call == "N9UNX"));
    }

    #[test]
    fn suggest_n_plus_one_handles_transpose() {
        let data = "N9UNX\nN9UXN\n";
        let db = ScpDb::from_reader(data.as_bytes()).unwrap();
        let hits = db.suggest_n_plus_one("N9UNX", 10);
        assert!(hits.iter().any(|h| h.call == "N9UXN"));
    }

    #[test]
    fn suggest_n_plus_one_handles_single_insert_delete() {
        let data = "N9UNX\nN9NX\n";
        let db = ScpDb::from_reader(data.as_bytes()).unwrap();
        let hits = db.suggest_n_plus_one("N9UNX", 10);
        assert!(hits.iter().any(|h| h.call == "N9NX"));
        let hits2 = db.suggest_n_plus_one("N9NX", 10);
        assert!(hits2.iter().any(|h| h.call == "N9UNX"));
    }

    #[test]
    fn suggest_with_context_applies_boosts() {
        let db = load_sample();
        let mut ctx = ScpSuggestContext::default();
        ctx.needed_mults.insert("K1ABC".to_string());
        let hits = db.suggest_with_context("K1A", 2, &ctx);
        assert_eq!(hits[0].call, "K1ABC");
        assert!(hits[0].reason.contains("mult"));
    }
}
