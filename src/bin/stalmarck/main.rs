//! `stalmarck`: run Stålmarck's proof procedure on a `.prop`, `.cnf` or
//! `.tri` file.
//!
//! Exit status follows the SAT-competition convention: 10 when a model was
//! found (SAT / INVALID), 20 when refuted (UNSAT / VALID), 0 when unknown,
//! and 1 on errors.

mod sha256;

use std::fmt::Write as _;
use std::io::Read;
use std::path::Path;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use stalmarck::{
    BranchState, Cnf, Config, Formula, Model, ProofResult, Report, SatResult,
    Solver, Stats, Triples, Unknown,
};

const USAGE: &str = "\
usage: stalmarck [OPTIONS] <FILE | ->

Decides satisfiability (or validity) of a propositional formula with
Stalmarck's proof procedure.

Input formats (chosen by extension, or with --format):
  .prop   formula text, e.g. 'a & (b | c) -> d'
  .cnf    DIMACS CNF
  .tri    triple file (docs/triple-format.md)

Options:
  --mode sat|valid         question to answer (default: sat)
  -k, --depth N            maximum Dilemma depth (default: 2)
  --no-sharing             do not share identical subformulas
  --branch-state S         rollback (default) or clone
  --max-branches N         stop after N Dilemma applications
  --timeout-ms N           stop after N milliseconds
  --format F               prop, cnf or tri (required for stdin)
  --json                   print a result-v1 JSON record
  --relation               include the canonical final relation
  --id ID                  input id for the JSON record (default: file stem)
  --emit-triples           print the triple file and exit
  -h, --help               print this help
  -V, --version            print the version

Exit status: 10 SAT/INVALID, 20 UNSAT/VALID, 0 UNKNOWN, 1 error.";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Format {
    Prop,
    Cnf,
    Tri,
}

impl Format {
    fn name(self) -> &'static str {
        match self {
            Format::Prop => "prop",
            Format::Cnf => "cnf",
            Format::Tri => "tri",
        }
    }
}

enum Input {
    Formula(Formula),
    Cnf(Cnf),
    Triples(Triples),
}

struct Options {
    path: String,
    format: Option<Format>,
    valid: bool,
    config: Config,
    json: bool,
    relation: bool,
    id: Option<String>,
    emit_triples: bool,
}

fn parse_args() -> Result<Options, String> {
    let mut args = std::env::args().skip(1);
    let mut o = Options {
        path: String::new(),
        format: None,
        valid: false,
        config: Config::default(),
        json: false,
        relation: false,
        id: None,
        emit_triples: false,
    };
    let mut path = None;
    while let Some(arg) = args.next() {
        let mut value = |name: &str| {
            args.next().ok_or_else(|| format!("{name} needs a value"))
        };
        let number = |name: &str, v: String| {
            v.parse::<u64>()
                .map_err(|_| format!("{name}: '{v}' is not a number"))
        };
        match arg.as_str() {
            "-h" | "--help" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            "-V" | "--version" => {
                println!("stalmarck {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            "--mode" => match value("--mode")?.as_str() {
                "sat" => o.valid = false,
                "valid" => o.valid = true,
                m => return Err(format!("--mode: unknown mode '{m}'")),
            },
            "-k" | "--depth" => {
                let v = value("--depth")?;
                o.config.max_depth = u32::try_from(number("--depth", v)?)
                    .map_err(|_| "--depth: too large".to_string())?;
            }
            "--no-sharing" => o.config.sharing = false,
            "--branch-state" => {
                o.config.branch_state = match value("--branch-state")?.as_str()
                {
                    "rollback" => BranchState::Rollback,
                    "clone" => BranchState::Clone,
                    s => return Err(format!("--branch-state: unknown '{s}'")),
                }
            }
            "--max-branches" => {
                let v = value("--max-branches")?;
                o.config.max_branches = Some(number("--max-branches", v)?);
            }
            "--timeout-ms" => {
                let v = value("--timeout-ms")?;
                let ms = number("--timeout-ms", v)?;
                o.config.timeout = Some(Duration::from_millis(ms));
            }
            "--format" => {
                o.format = Some(match value("--format")?.as_str() {
                    "prop" => Format::Prop,
                    "cnf" => Format::Cnf,
                    "tri" => Format::Tri,
                    f => return Err(format!("--format: unknown '{f}'")),
                })
            }
            "--json" => o.json = true,
            "--relation" => o.relation = true,
            "--id" => o.id = Some(value("--id")?),
            "--emit-triples" => o.emit_triples = true,
            a if a.starts_with('-') && a != "-" => {
                return Err(format!("unknown option '{a}'"));
            }
            _ if path.is_some() => return Err("more than one input".into()),
            _ => path = Some(arg),
        }
    }
    o.path = path.ok_or("no input file (see --help)")?;
    if o.format.is_none() {
        o.format =
            match Path::new(&o.path).extension().and_then(|e| e.to_str()) {
                Some("prop") => Some(Format::Prop),
                Some("cnf") => Some(Format::Cnf),
                Some("tri") => Some(Format::Tri),
                _ => return Err("cannot tell the format; use --format".into()),
            };
    }
    Ok(o)
}

fn ms(d: Duration) -> String {
    format!("{:.3}", d.as_secs_f64() * 1000.0)
}

fn json_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

struct Run {
    verdict: &'static str,
    unknown: Option<Unknown>,
    model: Option<Model>,
    report: Option<(Stats, String, stalmarck::Timing)>,
}

fn verdict(o: &Options, triples: &Triples) -> Run {
    let solver = Solver::new(o.config.clone());
    fn parts<R>(
        r: Report<R>,
    ) -> (R, Option<(Stats, String, stalmarck::Timing)>) {
        let rel = r.relation.to_string();
        (r.result, Some((r.stats, rel, r.timing)))
    }
    if o.valid {
        let (result, report) = parts(solver.prove_triples(triples));
        let (verdict, unknown, model) = match result {
            ProofResult::Valid => ("VALID", None, None),
            ProofResult::Invalid(m) => ("INVALID", None, Some(m)),
            ProofResult::Unknown(u) => ("UNKNOWN", Some(u), None),
        };
        Run { verdict, unknown, model, report }
    } else {
        let (result, report) = parts(solver.check_sat_triples(triples));
        let (verdict, unknown, model) = match result {
            SatResult::Unsat => ("UNSAT", None, None),
            SatResult::Sat(m) => ("SAT", None, Some(m)),
            SatResult::Unknown(u) => ("UNKNOWN", Some(u), None),
        };
        Run { verdict, unknown, model, report }
    }
}

fn unknown_name(u: Unknown) -> &'static str {
    match u {
        Unknown::DepthExhausted => "depth_exhausted",
        Unknown::BranchLimit => "branch_limit",
        Unknown::Timeout => "timeout",
        Unknown::ModelCheckFailed => "model_check_failed",
    }
}

fn main() -> ExitCode {
    let o = match parse_args() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("stalmarck: {e}");
            return ExitCode::from(1);
        }
    };
    let total = Instant::now();
    let mut bytes = Vec::new();
    let read = if o.path == "-" {
        std::io::stdin().read_to_end(&mut bytes).map(|_| ())
    } else {
        std::fs::read(&o.path).map(|b| bytes = b)
    };
    if let Err(e) = read {
        eprintln!("stalmarck: {}: {e}", o.path);
        return ExitCode::from(1);
    }
    let format = o.format.unwrap();
    let id = o.id.clone().unwrap_or_else(|| {
        Path::new(&o.path)
            .file_stem()
            .map_or("stdin".into(), |s| s.to_string_lossy().into_owned())
    });
    let hash = sha256::hex_digest(&bytes);

    // Parse and decompose.
    let t0 = Instant::now();
    let parsed = match std::str::from_utf8(&bytes) {
        Err(e) => Err(format!("input is not UTF-8: {e}")),
        Ok(text) => match format {
            Format::Prop => text
                .parse::<Formula>()
                .map(Input::Formula)
                .map_err(|e| e.to_string()),
            Format::Cnf => Cnf::parse_dimacs(text)
                .map(Input::Cnf)
                .map_err(|e| e.to_string()),
            Format::Tri => Triples::parse_text(text)
                .map(Input::Triples)
                .map_err(|e| e.to_string()),
        },
    };
    let parse_time = t0.elapsed();
    let input = match parsed {
        Ok(input) => input,
        Err(msg) => {
            eprintln!("stalmarck: {}: {msg}", o.path);
            if o.json {
                println!("{}", error_record(&o, &id, &hash, format, &msg));
            }
            return ExitCode::from(1);
        }
    };
    let t1 = Instant::now();
    let triples = match input {
        Input::Formula(f) => Triples::from_formula(&f, o.config.sharing),
        Input::Cnf(c) => Triples::from_cnf(&c, o.config.sharing),
        Input::Triples(t) => t,
    };
    let decompose_time = (format != Format::Tri).then(|| t1.elapsed());

    if o.emit_triples {
        print!("{}", triples.to_text());
        return ExitCode::SUCCESS;
    }

    let run = verdict(&o, &triples);
    let (stats, relation, timing) = run.report.clone().unwrap();
    let total_time = total.elapsed();

    if o.json {
        let mut j = String::from("{\"schema_version\":1,");
        let _ = write!(
            j,
            "\"implementation\":{{\"name\":\"rust\",\"version\":{},\"git_commit\":null,\"runtime\":null}},",
            json_string(env!("CARGO_PKG_VERSION"))
        );
        let c = &o.config;
        let opt =
            |v: Option<u64>| v.map_or("null".to_string(), |v| v.to_string());
        let _ = write!(
            j,
            "\"config\":{{\"mode\":\"{}\",\"track\":\"A\",\"max_depth\":{},\"sharing\":{},\"threads\":1,\"max_branches\":{},\"timeout_ms\":{},\"variant\":{}}},",
            if o.valid { "valid" } else { "sat" },
            c.max_depth,
            c.sharing,
            opt(c.max_branches),
            opt(c.timeout.map(|t| t.as_millis() as u64)),
            match c.branch_state {
                BranchState::Rollback => "null".to_string(),
                BranchState::Clone => json_string("branch-state=clone"),
            }
        );
        let _ = write!(
            j,
            "\"input\":{{\"id\":{},\"sha256\":\"{hash}\",\"format\":\"{}\",\"variables\":{},\"triples\":{},\"nodes\":{}}},",
            json_string(&id),
            format.name(),
            triples.variables().len(),
            triples.triples().len(),
            triples.node_count()
        );
        let _ = write!(j, "\"result\":{{\"verdict\":\"{}\"", run.verdict);
        if let Some(u) = run.unknown {
            let _ = write!(j, ",\"unknown_reason\":\"{}\"", unknown_name(u));
        }
        if let Some(m) = &run.model {
            let fields: Vec<String> = m
                .iter()
                .map(|(k, v)| format!("{}:{v}", json_string(k)))
                .collect();
            let _ = write!(j, ",\"model\":{{{}}}", fields.join(","));
        }
        if o.relation {
            let _ = write!(j, ",\"relation\":{}", json_string(&relation));
        }
        j.push_str("},");
        let _ = write!(
            j,
            "\"timing\":{{\"parse_ms\":{},\"decompose_ms\":{},\"solver_init_ms\":{},\"solve_ms\":{},\"saturation_ms\":null,\"dilemma_ms\":null,\"merge_ms\":{},\"total_ms\":{}}},",
            ms(parse_time),
            decompose_time.map_or("null".to_string(), ms),
            ms(timing.init),
            ms(timing.solve),
            ms(timing.merge),
            ms(total_time)
        );
        j.push_str("\"memory\":{\"peak_rss_bytes\":null},");
        let _ = write!(
            j,
            "\"counters\":{{\"triples_processed\":{},\"deductions_attempted\":{},\"deductions_successful\":{},\"merges\":{},\"worklist_insertions\":{},\"worklist_removals\":{},\"dilemma_branches\":{},\"dilemma_productive\":{}}},",
            stats.triples_processed,
            stats.deductions_attempted,
            stats.deductions_successful,
            stats.merges,
            stats.worklist_insertions,
            stats.worklist_removals,
            stats.dilemma_branches,
            stats.dilemma_productive
        );
        j.push_str("\"error\":null}");
        println!("{j}");
    } else {
        let mut line = format!("s {}", run.verdict);
        if let Some(u) = run.unknown {
            let _ = write!(line, " ({})", unknown_name(u));
        }
        println!("{line}");
        if let Some(m) = &run.model {
            let values: Vec<String> = m
                .iter()
                .map(|(k, v)| if v { k.to_string() } else { format!("!{k}") })
                .collect();
            println!("v {}", values.join(" "));
        }
        if o.relation {
            println!("r {relation}");
        }
        println!(
            "c depth={} branches={} merges={} solve={}ms",
            o.config.max_depth,
            stats.dilemma_branches,
            stats.merges,
            ms(timing.solve)
        );
    }
    ExitCode::from(match run.verdict {
        "SAT" | "INVALID" => 10,
        "UNSAT" | "VALID" => 20,
        _ => 0,
    })
}

fn error_record(
    o: &Options,
    id: &str,
    hash: &str,
    format: Format,
    msg: &str,
) -> String {
    let c = &o.config;
    format!(
        "{{\"schema_version\":1,\"implementation\":{{\"name\":\"rust\",\"version\":{}}},\"config\":{{\"mode\":\"{}\",\"track\":\"A\",\"max_depth\":{},\"sharing\":{}}},\"input\":{{\"id\":{},\"sha256\":\"{hash}\",\"format\":\"{}\",\"variables\":0,\"triples\":0,\"nodes\":1}},\"result\":{{\"verdict\":\"ERROR\"}},\"timing\":{{\"parse_ms\":null,\"decompose_ms\":null,\"solver_init_ms\":null,\"solve_ms\":null,\"total_ms\":null}},\"memory\":{{\"peak_rss_bytes\":null}},\"counters\":{{\"triples_processed\":0,\"deductions_attempted\":0,\"deductions_successful\":0,\"merges\":0,\"worklist_insertions\":0,\"worklist_removals\":0,\"dilemma_branches\":0,\"dilemma_productive\":0}},\"error\":{{\"kind\":\"parse\",\"message\":{}}}}}",
        json_string(env!("CARGO_PKG_VERSION")),
        if o.valid { "valid" } else { "sat" },
        c.max_depth,
        c.sharing,
        json_string(id),
        format.name(),
        json_string(msg)
    )
}
