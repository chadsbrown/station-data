const DEFAULT_SUFFIXES: &[&str] = &["P", "M", "MM", "QRP", "AM", "A"];

pub fn strip_suffixes(call: &str) -> String {
    let mut cur = call.trim().to_ascii_uppercase();
    loop {
        let mut changed = false;
        for suffix in DEFAULT_SUFFIXES {
            let needle = format!("/{suffix}");
            if cur.ends_with(&needle) {
                cur.truncate(cur.len() - needle.len());
                changed = true;
                break;
            }
        }
        if !changed {
            break;
        }
    }
    cur
}

pub fn normalize_call(call: &str) -> String {
    strip_suffixes(call)
}

pub fn is_plausible_callsign(s: &str) -> bool {
    let s = s.trim().to_ascii_uppercase();
    let len = s.len();
    if !(3..=12).contains(&len) {
        return false;
    }

    let mut has_alpha = false;
    let mut has_digit = false;
    for c in s.chars() {
        if c.is_ascii_alphabetic() {
            has_alpha = true;
        } else if c.is_ascii_digit() {
            has_digit = true;
        } else if c != '/' {
            return false;
        }
    }

    has_alpha && has_digit
}

pub fn split_slash_candidates(call: &str) -> Vec<String> {
    let normalized = normalize_call(call);
    if !normalized.contains('/') {
        return if is_plausible_callsign(&normalized) {
            vec![normalized]
        } else {
            Vec::new()
        };
    }

    let mut out = Vec::new();
    for part in normalized.split('/') {
        let part = strip_suffixes(part);
        if !part.is_empty() && is_plausible_callsign(&part) && !out.iter().any(|x| x == &part) {
            out.push(part);
        }
    }

    out.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_portable_suffixes() {
        assert_eq!(normalize_call("k1abc/p"), "K1ABC");
        assert_eq!(normalize_call("K1ABC/MM"), "K1ABC");
    }

    #[test]
    fn split_candidates_from_slashed_calls() {
        assert_eq!(split_slash_candidates("EA8/NN1N"), vec!["NN1N", "EA8"]);
    }

    #[test]
    fn plausibility_check() {
        assert!(is_plausible_callsign("K1ABC"));
        assert!(!is_plausible_callsign("ABC"));
        assert!(!is_plausible_callsign("K1-ABC"));
    }
}
