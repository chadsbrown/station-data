use station_data::{CtyDb, DomainSource, ScpDb, StationDataFacade, SuperCheck};
use std::env;
use std::path::Path;
use std::time::Instant;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        print_usage_and_exit();
    }

    match args[1].as_str() {
        "resolve-cty" => {
            if args.len() != 4 {
                print_usage_and_exit();
            }
            resolve_cty(Path::new(&args[2]), &args[3]);
        }
        "bench-cty" => {
            if args.len() < 3 || args.len() > 4 {
                print_usage_and_exit();
            }
            let iterations = if args.len() == 4 {
                args[3].parse::<usize>().unwrap_or_else(|_| {
                    eprintln!("error: iterations must be a positive integer");
                    std::process::exit(64);
                })
            } else {
                100_000
            };
            bench_cty(Path::new(&args[2]), iterations);
        }
        "bench-cty-calls" => {
            if args.len() < 4 || args.len() > 5 {
                print_usage_and_exit();
            }
            let passes = if args.len() == 5 {
                args[4].parse::<usize>().unwrap_or_else(|_| {
                    eprintln!("error: passes must be a positive integer");
                    std::process::exit(64);
                })
            } else {
                1
            };
            bench_cty_calls(Path::new(&args[2]), Path::new(&args[3]), passes);
        }
        "check-scp" => {
            if args.len() != 4 {
                print_usage_and_exit();
            }
            check_scp(Path::new(&args[2]), &args[3]);
        }
        "suggest-scp" => {
            if args.len() < 4 || args.len() > 5 {
                print_usage_and_exit();
            }
            let max_results = if args.len() == 5 {
                args[4].parse::<usize>().unwrap_or_else(|_| {
                    eprintln!("error: max_results must be a positive integer");
                    std::process::exit(64);
                })
            } else {
                10
            };
            suggest_scp(Path::new(&args[2]), &args[3], max_results);
        }
        "suggest-scp-n1" => {
            if args.len() < 4 || args.len() > 5 {
                print_usage_and_exit();
            }
            let max_results = if args.len() == 5 {
                args[4].parse::<usize>().unwrap_or_else(|_| {
                    eprintln!("error: max_results must be a positive integer");
                    std::process::exit(64);
                })
            } else {
                10
            };
            suggest_scp_n1(Path::new(&args[2]), &args[3], max_results);
        }
        "bench-scp-contains" => {
            if args.len() < 4 || args.len() > 5 {
                print_usage_and_exit();
            }
            let passes = if args.len() == 5 {
                args[4].parse::<usize>().unwrap_or_else(|_| {
                    eprintln!("error: passes must be a positive integer");
                    std::process::exit(64);
                })
            } else {
                1
            };
            bench_scp_contains(Path::new(&args[2]), Path::new(&args[3]), passes);
        }
        "bench-scp-search" => {
            if args.len() < 5 || args.len() > 6 {
                print_usage_and_exit();
            }
            let max_results = args[4].parse::<usize>().unwrap_or_else(|_| {
                eprintln!("error: max_results must be a positive integer");
                std::process::exit(64);
            });
            if max_results == 0 {
                eprintln!("error: max_results must be > 0");
                std::process::exit(64);
            }
            let passes = if args.len() == 6 {
                args[5].parse::<usize>().unwrap_or_else(|_| {
                    eprintln!("error: passes must be a positive integer");
                    std::process::exit(64);
                })
            } else {
                1
            };
            bench_scp_search(
                Path::new(&args[2]),
                Path::new(&args[3]),
                max_results,
                passes,
            );
        }
        "bench-scp-suggest" => {
            if args.len() < 5 || args.len() > 6 {
                print_usage_and_exit();
            }
            let max_results = args[4].parse::<usize>().unwrap_or_else(|_| {
                eprintln!("error: max_results must be a positive integer");
                std::process::exit(64);
            });
            if max_results == 0 {
                eprintln!("error: max_results must be > 0");
                std::process::exit(64);
            }
            let passes = if args.len() == 6 {
                args[5].parse::<usize>().unwrap_or_else(|_| {
                    eprintln!("error: passes must be a positive integer");
                    std::process::exit(64);
                })
            } else {
                1
            };
            bench_scp_suggest(
                Path::new(&args[2]),
                Path::new(&args[3]),
                max_results,
                passes,
            );
        }
        "bench-scp-n1" => {
            if args.len() < 5 || args.len() > 6 {
                print_usage_and_exit();
            }
            let max_results = args[4].parse::<usize>().unwrap_or_else(|_| {
                eprintln!("error: max_results must be a positive integer");
                std::process::exit(64);
            });
            if max_results == 0 {
                eprintln!("error: max_results must be > 0");
                std::process::exit(64);
            }
            let passes = if args.len() == 6 {
                args[5].parse::<usize>().unwrap_or_else(|_| {
                    eprintln!("error: passes must be a positive integer");
                    std::process::exit(64);
                })
            } else {
                1
            };
            bench_scp_n1(Path::new(&args[2]), Path::new(&args[3]), max_results, passes);
        }
        "facade-status" => {
            if args.len() < 4 || args.len() > 6 {
                print_usage_and_exit();
            }
            let cty = Path::new(&args[2]);
            let domain_source = parse_domain_source_arg(&args[3]);
            let call = args.get(4).map(String::as_str);
            let domain = args.get(5).map(String::as_str);
            facade_status(cty, domain_source, call, domain);
        }
        _ => print_usage_and_exit(),
    }
}

fn resolve_cty(path: &Path, call: &str) {
    let db = match CtyDb::from_path(path) {
        Ok(db) => db,
        Err(err) => {
            eprintln!("error: failed to load CTY from {}: {err}", path.display());
            std::process::exit(1);
        }
    };

    match db.lookup(call) {
        Some(hit) => {
            println!("call: {}", call.trim().to_ascii_uppercase());
            println!("dxcc: {}", hit.dxcc);
            println!("continent: {}", hit.continent);
            if let Some(cq) = hit.cq_zone {
                println!("cq_zone: {cq}");
            }
            if let Some(itu) = hit.itu_zone {
                println!("itu_zone: {itu}");
            }
            println!("is_wve: {}", hit.is_wve);
            println!("is_na: {}", hit.is_na);
        }
        None => {
            println!("call: {}", call.trim().to_ascii_uppercase());
            println!("resolved: false");
            std::process::exit(2);
        }
    }
}

fn bench_cty(path: &Path, iterations: usize) {
    if iterations == 0 {
        eprintln!("error: iterations must be > 0");
        std::process::exit(64);
    }

    let corpus = benchmark_calls();
    let load_start = Instant::now();
    let db = match CtyDb::from_path(path) {
        Ok(db) => db,
        Err(err) => {
            eprintln!("error: failed to load CTY from {}: {err}", path.display());
            std::process::exit(1);
        }
    };
    let load_elapsed = load_start.elapsed();

    let mut resolved = 0usize;
    let mut misses = 0usize;

    let lookup_start = Instant::now();
    for i in 0..iterations {
        let call = corpus[i % corpus.len()];
        if db.lookup(call).is_some() {
            resolved += 1;
        } else {
            misses += 1;
        }
    }
    let lookup_elapsed = lookup_start.elapsed();
    let ns_per_lookup = lookup_elapsed.as_nanos() / iterations as u128;

    println!("cty_path: {}", path.display());
    println!("countries: {}", db.countries.len());
    println!("corpus_size: {}", corpus.len());
    println!("iterations: {iterations}");
    println!("parse_ms: {:.3}", load_elapsed.as_secs_f64() * 1000.0);
    println!("lookup_ms: {:.3}", lookup_elapsed.as_secs_f64() * 1000.0);
    println!("ns_per_lookup: {ns_per_lookup}");
    println!("resolved: {resolved}");
    println!("misses: {misses}");
}

fn benchmark_calls() -> &'static [&'static str] {
    &[
        "N9UNX", "K1AR", "W1AW", "VE3EJ", "DL1ABC", "F5IN", "EA8/NN1N", "BA4DL/0", "JA1ABC",
        "ZS6EZ", "VK2AA", "LU1DZ", "PY2NY", "G3TXF", "OH2BH", "SM6CNN", "PA3AAV", "SP2FAX",
        "9A1A", "S50C", "YU1JW", "SV1GA", "5B4AIF", "4X1VF", "A45XR", "VU2PTT", "HS0ZIA",
        "YB1AQY", "9M2TO", "DU1IVT", "KH6LC", "KL7RA", "KP4AA", "TI2CC", "XE1AAA", "PZ5RA",
        "VP8LP", "ZS8W", "3B8CF", "C6AUM", "C91CCY", "ET3AA", "5R8UI", "7Q7CT", "A61BK",
        "EX8MLE", "UN7LZ", "RA9AA", "R0FA", "UA0SC", "BY1RX", "BD7IHN", "HL1VAU", "BV2LA",
        "VR2XMT", "E21EIC", "9V1YC", "A35JP", "ZL1BQD", "T88AQ", "KH2L", "FO/F6BCW",
        "FK8GM", "3D2AG", "C21TS", "P29VCX", "S79VU", "V51WH", "3DA0RU", "A92AA", "EP2LMA",
        "4L1MA", "JY9FC", "HZ1SK", "T77C", "9A2WA", "OE1CIW", "LX1NO", "HB9BZA", "OK1RR",
        "OM2VL", "LZ2HM", "YO9WF", "ER3CT", "ZA1EM", "E73Y", "Z35T", "TA2DS", "TF3DC",
        "OX3LX", "OY1CT", "JW7QIA", "LA7GIA", "OH0Z", "SV9CVY", "CT3KN", "CU2KG", "ZZ9ZZ",
    ]
}

fn bench_cty_calls(cty_path: &Path, calls_path: &Path, passes: usize) {
    if passes == 0 {
        eprintln!("error: passes must be > 0");
        std::process::exit(64);
    }

    let calls = match load_calls_file(calls_path) {
        Ok(calls) => calls,
        Err(err) => {
            eprintln!(
                "error: failed to load calls file {}: {err}",
                calls_path.display()
            );
            std::process::exit(1);
        }
    };
    if calls.is_empty() {
        eprintln!(
            "error: calls file {} did not contain any calls",
            calls_path.display()
        );
        std::process::exit(64);
    }

    let load_start = Instant::now();
    let db = match CtyDb::from_path(cty_path) {
        Ok(db) => db,
        Err(err) => {
            eprintln!("error: failed to load CTY from {}: {err}", cty_path.display());
            std::process::exit(1);
        }
    };
    let load_elapsed = load_start.elapsed();

    let mut resolved = 0usize;
    let mut misses = 0usize;
    let lookups = calls.len() * passes;

    let lookup_start = Instant::now();
    for _ in 0..passes {
        for call in &calls {
            if db.lookup(call).is_some() {
                resolved += 1;
            } else {
                misses += 1;
            }
        }
    }
    let lookup_elapsed = lookup_start.elapsed();
    let ns_per_lookup = lookup_elapsed.as_nanos() / lookups as u128;

    println!("cty_path: {}", cty_path.display());
    println!("calls_path: {}", calls_path.display());
    println!("countries: {}", db.countries.len());
    println!("corpus_size: {}", calls.len());
    println!("passes: {passes}");
    println!("iterations: {lookups}");
    println!("parse_ms: {:.3}", load_elapsed.as_secs_f64() * 1000.0);
    println!("lookup_ms: {:.3}", lookup_elapsed.as_secs_f64() * 1000.0);
    println!("ns_per_lookup: {ns_per_lookup}");
    println!("resolved: {resolved}");
    println!("misses: {misses}");
}

fn load_calls_file(path: &Path) -> Result<Vec<String>, std::io::Error> {
    load_non_comment_lines(path, true)
}

fn load_scp_or_exit(path: &Path) -> ScpDb {
    match ScpDb::from_path(path) {
        Ok(db) => db,
        Err(err) => {
            eprintln!("error: failed to load SCP file {}: {err}", path.display());
            std::process::exit(1);
        }
    }
}

fn check_scp(path: &Path, call: &str) {
    let db = load_scp_or_exit(path);

    let normalized = call.trim().to_ascii_uppercase();
    let in_scp = db.contains(&normalized);
    println!("call: {normalized}");
    println!("in_scp: {in_scp}");

    if !in_scp {
        std::process::exit(2);
    }
}

fn suggest_scp(path: &Path, partial: &str, max_results: usize) {
    let db = load_scp_or_exit(path);

    let suggestions = db.suggest(partial, max_results);
    println!("partial: {}", partial.trim().to_ascii_uppercase());
    println!("max_results: {max_results}");
    for s in suggestions {
        println!("{} score={} reason={}", s.call, s.score, s.reason);
    }
}

fn suggest_scp_n1(path: &Path, partial: &str, max_results: usize) {
    let db = load_scp_or_exit(path);

    let suggestions = db.suggest_n_plus_one(partial, max_results);
    println!("partial: {}", partial.trim().to_ascii_uppercase());
    println!("max_results: {max_results}");
    for s in suggestions {
        println!("{} score={} reason={}", s.call, s.score, s.reason);
    }
}

fn bench_scp_contains(scp_path: &Path, calls_path: &Path, passes: usize) {
    if passes == 0 {
        eprintln!("error: passes must be > 0");
        std::process::exit(64);
    }

    let calls = match load_non_comment_lines(calls_path, true) {
        Ok(calls) => calls,
        Err(err) => {
            eprintln!(
                "error: failed to load calls file {}: {err}",
                calls_path.display()
            );
            std::process::exit(1);
        }
    };
    if calls.is_empty() {
        eprintln!(
            "error: calls file {} did not contain any calls",
            calls_path.display()
        );
        std::process::exit(64);
    }

    let load_start = Instant::now();
    let db = load_scp_or_exit(scp_path);
    let load_elapsed = load_start.elapsed();

    let mut hits = 0usize;
    let mut misses = 0usize;
    let lookups = calls.len() * passes;
    let lookup_start = Instant::now();
    for _ in 0..passes {
        for call in &calls {
            if db.contains(call) {
                hits += 1;
            } else {
                misses += 1;
            }
        }
    }
    let lookup_elapsed = lookup_start.elapsed();
    let ns_per_lookup = lookup_elapsed.as_nanos() / lookups as u128;

    println!("scp_path: {}", scp_path.display());
    println!("calls_path: {}", calls_path.display());
    println!("corpus_size: {}", calls.len());
    println!("passes: {passes}");
    println!("iterations: {lookups}");
    println!("parse_ms: {:.3}", load_elapsed.as_secs_f64() * 1000.0);
    println!("lookup_ms: {:.3}", lookup_elapsed.as_secs_f64() * 1000.0);
    println!("ns_per_lookup: {ns_per_lookup}");
    println!("hits: {hits}");
    println!("misses: {misses}");
}

fn bench_scp_search(scp_path: &Path, patterns_path: &Path, max_results: usize, passes: usize) {
    if passes == 0 {
        eprintln!("error: passes must be > 0");
        std::process::exit(64);
    }

    let patterns = match load_non_comment_lines(patterns_path, true) {
        Ok(lines) => lines,
        Err(err) => {
            eprintln!(
                "error: failed to load patterns file {}: {err}",
                patterns_path.display()
            );
            std::process::exit(1);
        }
    };
    if patterns.is_empty() {
        eprintln!(
            "error: patterns file {} did not contain any patterns",
            patterns_path.display()
        );
        std::process::exit(64);
    }

    let load_start = Instant::now();
    let db = load_scp_or_exit(scp_path);
    let load_elapsed = load_start.elapsed();

    let searches = patterns.len() * passes;
    let mut total_matches = 0usize;
    let search_start = Instant::now();
    for _ in 0..passes {
        for pattern in &patterns {
            total_matches += db.search(pattern, max_results).len();
        }
    }
    let search_elapsed = search_start.elapsed();
    let ns_per_search = search_elapsed.as_nanos() / searches as u128;

    println!("scp_path: {}", scp_path.display());
    println!("patterns_path: {}", patterns_path.display());
    println!("pattern_count: {}", patterns.len());
    println!("passes: {passes}");
    println!("iterations: {searches}");
    println!("max_results: {max_results}");
    println!("parse_ms: {:.3}", load_elapsed.as_secs_f64() * 1000.0);
    println!("search_ms: {:.3}", search_elapsed.as_secs_f64() * 1000.0);
    println!("ns_per_search: {ns_per_search}");
    println!("total_matches_returned: {total_matches}");
}

fn bench_scp_suggest(scp_path: &Path, patterns_path: &Path, max_results: usize, passes: usize) {
    if passes == 0 {
        eprintln!("error: passes must be > 0");
        std::process::exit(64);
    }

    let partials = match load_non_comment_lines(patterns_path, true) {
        Ok(lines) => lines,
        Err(err) => {
            eprintln!(
                "error: failed to load partials file {}: {err}",
                patterns_path.display()
            );
            std::process::exit(1);
        }
    };
    if partials.is_empty() {
        eprintln!(
            "error: partials file {} did not contain any lines",
            patterns_path.display()
        );
        std::process::exit(64);
    }

    let load_start = Instant::now();
    let db = load_scp_or_exit(scp_path);
    let load_elapsed = load_start.elapsed();

    let iterations = partials.len() * passes;
    let mut total_suggestions = 0usize;
    let search_start = Instant::now();
    for _ in 0..passes {
        for partial in &partials {
            total_suggestions += db.suggest(partial, max_results).len();
        }
    }
    let search_elapsed = search_start.elapsed();
    let ns_per_suggest = search_elapsed.as_nanos() / iterations as u128;

    println!("scp_path: {}", scp_path.display());
    println!("partials_path: {}", patterns_path.display());
    println!("partial_count: {}", partials.len());
    println!("passes: {passes}");
    println!("iterations: {iterations}");
    println!("max_results: {max_results}");
    println!("parse_ms: {:.3}", load_elapsed.as_secs_f64() * 1000.0);
    println!("suggest_ms: {:.3}", search_elapsed.as_secs_f64() * 1000.0);
    println!("ns_per_suggest: {ns_per_suggest}");
    println!("total_suggestions_returned: {total_suggestions}");
}

fn bench_scp_n1(scp_path: &Path, partials_path: &Path, max_results: usize, passes: usize) {
    if passes == 0 {
        eprintln!("error: passes must be > 0");
        std::process::exit(64);
    }

    let partials = match load_non_comment_lines(partials_path, true) {
        Ok(lines) => lines,
        Err(err) => {
            eprintln!(
                "error: failed to load partials file {}: {err}",
                partials_path.display()
            );
            std::process::exit(1);
        }
    };
    if partials.is_empty() {
        eprintln!(
            "error: partials file {} did not contain any lines",
            partials_path.display()
        );
        std::process::exit(64);
    }

    let load_start = Instant::now();
    let db = load_scp_or_exit(scp_path);
    let load_elapsed = load_start.elapsed();

    let iterations = partials.len() * passes;
    let mut total_suggestions = 0usize;
    let start = Instant::now();
    for _ in 0..passes {
        for partial in &partials {
            total_suggestions += db.suggest_n_plus_one(partial, max_results).len();
        }
    }
    let elapsed = start.elapsed();
    let ns_per_suggest = elapsed.as_nanos() / iterations as u128;

    println!("scp_path: {}", scp_path.display());
    println!("partials_path: {}", partials_path.display());
    println!("partial_count: {}", partials.len());
    println!("passes: {passes}");
    println!("iterations: {iterations}");
    println!("max_results: {max_results}");
    println!("parse_ms: {:.3}", load_elapsed.as_secs_f64() * 1000.0);
    println!("suggest_ms: {:.3}", elapsed.as_secs_f64() * 1000.0);
    println!("ns_per_suggest: {ns_per_suggest}");
    println!("total_suggestions_returned: {total_suggestions}");
}

fn load_non_comment_lines(path: &Path, uppercase: bool) -> Result<Vec<String>, std::io::Error> {
    let text = std::fs::read_to_string(path)?;
    let mut lines = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        if uppercase {
            lines.push(t.to_ascii_uppercase());
        } else {
            lines.push(t.to_string());
        }
    }
    Ok(lines)
}

fn parse_domain_source_arg(raw: &str) -> DomainSource {
    if raw.eq_ignore_ascii_case("builtin") {
        DomainSource::Builtin
    } else {
        DomainSource::Dir(Path::new(raw).to_path_buf())
    }
}

fn facade_status(
    cty_path: &Path,
    domains: DomainSource,
    call: Option<&str>,
    domain: Option<&str>,
) {
    let facade = match StationDataFacade::load_from_paths(cty_path, domains, None) {
        Ok(facade) => facade,
        Err(err) => {
            eprintln!("error: failed to load StationDataFacade: {err}");
            std::process::exit(1);
        }
    };

    let version = facade.version();
    println!("cty_source: {}", version.cty_source);
    println!("cty_fingerprint: {}", version.cty_fingerprint);
    println!("domains_source: {}", version.domains_source);
    println!("domains_fingerprint: {}", version.domains_fingerprint);
    println!("loaded_at: {:?}", version.loaded_at);

    if let Some(call) = call {
        let hit = facade.resolve_call(call);
        println!("resolve_call({}): {}", call, hit.is_some());
    }
    if let Some(domain_name) = domain {
        let values = facade.domain_values(domain_name);
        println!(
            "domain_values({}): {}",
            domain_name,
            values.as_ref().map(|v| v.len()).unwrap_or(0)
        );
    }

    let m = facade.metrics();
    println!("metrics.resolve_calls: {}", m.resolve_calls);
    println!("metrics.resolve_hits: {}", m.resolve_hits);
    println!("metrics.resolve_misses: {}", m.resolve_misses);
    println!("metrics.domain_calls: {}", m.domain_calls);
    println!("metrics.domain_hits: {}", m.domain_hits);
    println!("metrics.domain_misses: {}", m.domain_misses);
    println!("metrics.history_calls: {}", m.history_calls);
    println!("metrics.history_hits: {}", m.history_hits);
    println!("metrics.history_misses: {}", m.history_misses);
}

fn print_usage_and_exit() -> ! {
    eprintln!("usage:");
    eprintln!("  station_data_tool resolve-cty <cty.dat> <callsign>");
    eprintln!("  station_data_tool bench-cty <cty.dat> [iterations]");
    eprintln!("  station_data_tool bench-cty-calls <cty.dat> <calls.txt> [passes]");
    eprintln!("  station_data_tool check-scp <scp.txt> <callsign>");
    eprintln!("  station_data_tool suggest-scp <scp.txt> <partial> [max_results]");
    eprintln!("  station_data_tool suggest-scp-n1 <scp.txt> <partial> [max_results]");
    eprintln!("  station_data_tool bench-scp-contains <scp.txt> <calls.txt> [passes]");
    eprintln!(
        "  station_data_tool bench-scp-search <scp.txt> <patterns.txt> <max_results> [passes]"
    );
    eprintln!(
        "  station_data_tool bench-scp-suggest <scp.txt> <partials.txt> <max_results> [passes]"
    );
    eprintln!("  station_data_tool bench-scp-n1 <scp.txt> <partials.txt> <max_results> [passes]");
    eprintln!(
        "  station_data_tool facade-status <cty.dat> <builtin|domains_dir> [call] [domain]"
    );
    std::process::exit(64);
}
