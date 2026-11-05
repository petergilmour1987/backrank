//! `cargo xtask ladder`: play Backrank against a rung of the Stockfish difficulty ladder,
//! record the result, and move up a rung once it has been earned.
//!
//! The rungs are defined in `tools/ladder.toml`. The rung Backrank is on is kept in
//! `results/state.toml`, ratings measured by `cargo xtask calibrate` in
//! `results/calibration.toml`, and every run adds a row to `results/ladder.csv`.

use crate::fastchess::{EngineSpec, Match};
use crate::stats::{Elo, Record};
use crate::{engine, paths, time};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;
use std::path::{Path, PathBuf};

#[derive(clap::Args)]
pub struct Args {
    /// Play this rung instead of the current one. Only the current rung can promote.
    #[arg(long)]
    rung: Option<String>,
    /// Number of games (default: from tools/ladder.toml).
    #[arg(long)]
    games: Option<u32>,
    /// Time control as seconds+increment (default: from tools/ladder.toml).
    #[arg(long)]
    tc: Option<String>,
    /// Games played at the same time (default: from tools/ladder.toml).
    #[arg(long)]
    concurrency: Option<u32>,
    /// After moving up, carry on with the next rung until one is not passed.
    #[arg(long)]
    climb: bool,
    /// Seed for the opening order (default: random). Recorded with the result.
    #[arg(long)]
    seed: Option<u64>,
    /// Free text recorded with the result, such as the feature being measured.
    #[arg(long, default_value = "")]
    note: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ladder {
    pub defaults: Defaults,
    #[serde(rename = "rung")]
    pub rungs: Vec<Rung>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Defaults {
    pub tc: String,
    pub games: u32,
    pub concurrency: u32,
    pub promote_at: f64,
    pub min_games: u32,
    pub regression_below: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rung {
    pub name: String,
    /// sfproxy: probability of a random move instead of Stockfish Skill Level 0's.
    pub blunder: Option<f64>,
    /// Stockfish limited to this UCI_Elo.
    pub uci_elo: Option<u32>,
}

impl Rung {
    /// The engine this rung puts on the board, under the rung's name.
    pub fn engine(&self, sfproxy: &Path, stockfish: &Path) -> EngineSpec {
        match (self.blunder, self.uci_elo) {
            (Some(p), _) => EngineSpec::new(&self.name, sfproxy)
                .arg("--stockfish")
                .arg(stockfish.display().to_string())
                .arg("--blunder")
                .arg(p.to_string()),
            (None, Some(elo)) => EngineSpec::new(&self.name, stockfish)
                .option("UCI_LimitStrength", "true")
                .option("UCI_Elo", &elo.to_string()),
            (None, None) => EngineSpec::new(&self.name, stockfish),
        }
    }

    pub fn describe(&self) -> String {
        match (self.blunder, self.uci_elo) {
            (Some(p), _) if p >= 1.0 => "random legal moves".to_string(),
            (Some(p), _) => format!("Stockfish Skill 0, {:.0}% random moves", p * 100.0),
            (None, Some(elo)) => format!("Stockfish UCI_Elo {elo}"),
            (None, None) => "Stockfish at full strength".to_string(),
        }
    }
}

impl Ladder {
    pub fn load() -> Result<Ladder> {
        let path = paths::ladder();
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let ladder: Ladder =
            toml::from_str(&text).with_context(|| format!("invalid {}", path.display()))?;
        ladder.validate()?;
        Ok(ladder)
    }

    fn validate(&self) -> Result<()> {
        ensure!(!self.rungs.is_empty(), "the ladder has no rungs");
        let d = &self.defaults;
        ensure!(
            d.games >= 2 && d.games.is_multiple_of(2),
            "defaults.games must be even"
        );
        for (i, rung) in self.rungs.iter().enumerate() {
            ensure!(
                !rung.name.starts_with("backrank"),
                "rung names may not start with \"backrank\""
            );
            ensure!(
                !self.rungs[..i].iter().any(|r| r.name == rung.name),
                "rung {} appears twice",
                rung.name
            );
            ensure!(
                rung.blunder.is_none() || rung.uci_elo.is_none(),
                "rung {} sets both blunder and uci_elo",
                rung.name
            );
            if let Some(p) = rung.blunder {
                ensure!(
                    (0.0..=1.0).contains(&p),
                    "rung {}: blunder must be 0 to 1",
                    rung.name
                );
            }
            if let Some(elo) = rung.uci_elo {
                ensure!(
                    (1320..=3190).contains(&elo),
                    "rung {}: Stockfish's UCI_Elo runs from 1320 to 3190",
                    rung.name
                );
            }
        }
        Ok(())
    }

    pub fn index(&self, name: &str) -> Result<usize> {
        self.rungs
            .iter()
            .position(|r| r.name == name)
            .with_context(|| format!("there is no rung named {name:?} in tools/ladder.toml"))
    }

    /// The rung's rating and where it comes from, if it has one: Stockfish's own scale for
    /// UCI_Elo rungs, `cargo xtask calibrate` for the rest.
    pub fn rating(&self, rung: &Rung, calibration: &Calibration) -> Option<Rating> {
        if let Some(elo) = rung.uci_elo {
            return Some(Rating {
                value: f64::from(elo),
                bound: Bound::Exact,
            });
        }
        calibration
            .get(&rung.name)
            .filter(|c| Some(c.blunder) == rung.blunder)
            .map(|c| Rating {
                value: c.rating,
                bound: c.bound,
            })
    }

    /// Reasons a calibrated rating may be out of date: measured with a different Stockfish
    /// or time control, or against a neighbour whose rating has changed since.
    pub fn stale_calibration(
        &self,
        rung: &Rung,
        calibration: &Calibration,
        stockfish: &str,
        tc: &str,
    ) -> Vec<String> {
        let Some(c) = calibration.get(&rung.name) else {
            return Vec::new();
        };
        let mut reasons = Vec::new();
        if Some(c.blunder) != rung.blunder {
            reasons.push(format!(
                "it was measured with blunder {}, but the rung now has {:?}",
                c.blunder, rung.blunder
            ));
        }
        if c.stockfish != stockfish {
            reasons.push(format!(
                "it was measured with {}, not {stockfish}",
                c.stockfish
            ));
        }
        if c.tc != tc {
            reasons.push(format!("it was measured at {}, not {tc}", c.tc));
        }
        let neighbour = self.rungs.iter().find(|r| r.name == c.against);
        match neighbour.and_then(|r| self.rating(r, calibration)) {
            Some(now) if (now.value - c.against_rating).abs() < 0.5 => {}
            Some(now) => reasons.push(format!(
                "{} was rated {:.0} then and is rated {now} now",
                c.against, c.against_rating
            )),
            None => reasons.push(format!("{} no longer has a rating", c.against)),
        }
        reasons
    }
}

/// A rating, which a whitewash during calibration only bounds from one side.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rating {
    pub value: f64,
    pub bound: Bound,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Bound {
    Exact,
    /// The true rating is at most this.
    AtMost,
    /// The true rating is at least this.
    AtLeast,
}

impl std::fmt::Display for Rating {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let prefix = match self.bound {
            Bound::Exact => "",
            Bound::AtMost => "at most ",
            Bound::AtLeast => "at least ",
        };
        write!(f, "{prefix}{:.0}", self.value)
    }
}

/// A rating measured by `cargo xtask calibrate`.
#[derive(Clone, Serialize, Deserialize)]
pub struct Calibrated {
    pub rating: f64,
    pub bound: Bound,
    /// The rung's blunder value when measured. If tools/ladder.toml has changed it since,
    /// the rating no longer applies.
    pub blunder: f64,
    /// The rung it was measured against, and that rung's rating at the time.
    pub against: String,
    pub against_rating: f64,
    pub games: u32,
    pub score: f64,
    pub tc: String,
    pub stockfish: String,
    pub date: String,
}

pub type Calibration = BTreeMap<String, Calibrated>;

pub fn load_calibration() -> Result<Calibration> {
    let path = paths::calibration();
    if !path.exists() {
        return Ok(Calibration::new());
    }
    let text = std::fs::read_to_string(&path)?;
    toml::from_str(&text).with_context(|| format!("invalid {}", path.display()))
}

pub fn save_calibration(calibration: &Calibration) -> Result<()> {
    let text = format!(
        "# Ratings of ladder rungs measured by `cargo xtask calibrate`. Each rung played its\n\
         # stronger neighbour; see `against`.\n\n{}",
        toml::to_string(calibration)?
    );
    engine::write_atomic(&paths::calibration(), text.as_bytes())
}

#[derive(Serialize, Deserialize)]
struct State {
    current: String,
}

fn load_state(ladder: &Ladder) -> Result<State> {
    let path = paths::ladder_state();
    let state = if path.exists() {
        let text = std::fs::read_to_string(&path)?;
        toml::from_str(&text).with_context(|| format!("invalid {}", path.display()))?
    } else {
        State {
            current: ladder.rungs[0].name.clone(),
        }
    };
    ladder.index(&state.current).with_context(|| {
        format!(
            "{} points at a rung that tools/ladder.toml doesn't have; set `current` to one \
             of its rungs",
            path.display()
        )
    })?;
    Ok(state)
}

fn save_state(state: &State) -> Result<()> {
    let text = format!(
        "# The ladder rung Backrank is currently on. `cargo xtask ladder` updates this when it\n\
         # moves up.\n{}",
        toml::to_string(state)?
    );
    engine::write_atomic(&paths::ladder_state(), text.as_bytes())
}

/// A random opening seed, kept below 2^63 so any reasonable parser reads it.
pub fn random_seed() -> u64 {
    RandomState::new().hash_one(std::process::id()) >> 1
}

/// Checks the files `cargo xtask setup` installs are there.
pub fn ensure_tools() -> Result<()> {
    for required in [paths::fastchess(), paths::book(), paths::stockfish()] {
        ensure!(
            required.exists(),
            "{} is missing; run `cargo xtask setup` first",
            required.display()
        );
    }
    Ok(())
}

pub fn run(args: &Args) -> Result<()> {
    ensure_tools()?;
    let ladder = Ladder::load()?;
    let calibration = load_calibration()?;
    let mut state = load_state(&ladder)?;
    let d = &ladder.defaults;
    let games = args.games.unwrap_or(d.games);
    ensure!(
        games >= 2 && games.is_multiple_of(2),
        "--games must be an even number of at least 2"
    );
    let tc = args.tc.clone().unwrap_or_else(|| d.tc.clone());
    let concurrency = args.concurrency.unwrap_or(d.concurrency);

    let label = engine::version_label()?;
    let backrank = engine::snapshot(&engine::build_backrank()?, &label)?;
    let sfproxy = engine::build_sfproxy()?;
    let stockfish_name = engine::id_name(&paths::stockfish())?;

    let mut rung_name = args.rung.clone().unwrap_or_else(|| state.current.clone());
    loop {
        let index = ladder.index(&rung_name)?;
        let current = ladder.index(&state.current)?;
        let rung = &ladder.rungs[index];
        let seed = args.seed.unwrap_or_else(random_seed);
        println!(
            "ladder: Backrank {label} vs {} ({}), {games} games at {tc}",
            rung.name,
            rung.describe()
        );

        let stamp = time::file_stamp();
        let pgn_path = paths::results_pgn().join(format!("ladder-{}-{stamp}.pgn", rung.name));
        let log_path = paths::results_logs().join(format!("ladder-{}-{stamp}.log", rung.name));
        let m = Match {
            engines: [
                EngineSpec::new("backrank", &backrank),
                rung.engine(&sfproxy, &paths::stockfish()),
            ],
            tc: &tc,
            rounds: games / 2,
            concurrency,
            book: &paths::book(),
            pgn_out: &pgn_path,
            log: &log_path,
            seed: Some(seed),
            sprt: None,
        };
        m.run()?;
        let tally = m.tally("backrank", &rung.name)?;
        let record = tally.record();
        let rating = ladder.rating(rung, &calibration);
        let (low, high) = record.elo_interval();

        println!();
        println!(
            "ladder: vs {}: W{} D{} L{} ({:.1}%), Elo {} [{}, {}], LOS {:.1}%",
            rung.name,
            record.wins,
            record.draws,
            record.losses,
            record.score() * 100.0,
            Elo(record.elo()),
            Elo(low),
            Elo(high),
            record.los() * 100.0
        );
        if let Some(rating) = rating {
            let prefix = match rating.bound {
                Bound::Exact => "",
                Bound::AtMost => "at most ",
                Bound::AtLeast => "at least ",
            };
            println!(
                "ladder: estimated rating {prefix}{} [{}, {}] (rung rated {rating}; the \
                 interval doesn't include the rung's own uncertainty)",
                whole(rating.value + record.elo()),
                whole(rating.value + low),
                whole(rating.value + high),
            );
        }
        for reason in ladder.stale_calibration(rung, &calibration, &stockfish_name, &tc) {
            println!(
                "ladder: warning: the calibrated rating of {} may be stale: {reason}",
                rung.name
            );
        }

        // Anything that makes the result unrepresentative blocks promotion.
        let mut blockers = Vec::new();
        let played = record.games() + tally.unfinished;
        if played != games || tally.unfinished > 0 {
            println!(
                "ladder: warning: {} of {games} games finished; see {}",
                record.games(),
                log_path.display()
            );
            blockers.push("incomplete");
        }
        if !tally.own_faults.is_empty() {
            println!(
                "ladder: BUG: Backrank lost games by {:?}; see {}",
                tally.own_faults,
                pgn_path.display()
            );
            blockers.push("Backrank faults");
        }
        if !tally.opponent_faults.is_empty() {
            println!(
                "ladder: warning: {} lost games by {:?}, which Backrank won without earning \
                 them; see {}",
                rung.name,
                tally.opponent_faults,
                pgn_path.display()
            );
            blockers.push("opponent faults");
        }
        if tc != d.tc {
            blockers.push("non-standard time control");
        }
        if record.games() < d.min_games {
            blockers.push("too few games");
        }

        let passed = blockers.is_empty() && record.score() >= d.promote_at;
        let outcome = if index == current && passed {
            match ladder.rungs.get(index + 1) {
                Some(next) => {
                    state.current = next.name.clone();
                    save_state(&state)?;
                    println!("ladder: PASSED, moving up to {}", next.name);
                    format!("promoted to {}", next.name)
                }
                None => {
                    println!("ladder: PASSED the top rung");
                    "passed the top rung".to_string()
                }
            }
        } else if index < current && record.score() < d.regression_below {
            println!(
                "ladder: WARNING: scored below {:.0}% on {}, which was passed before: a \
                 regression?",
                d.regression_below * 100.0,
                rung.name
            );
            "regression?".to_string()
        } else if index == current {
            println!(
                "ladder: not passed (needs {:.0}% over at least {} games at {}, with no faults \
                 on either side)",
                d.promote_at * 100.0,
                d.min_games,
                d.tc
            );
            if blockers.is_empty() {
                "stayed".to_string()
            } else {
                format!("stayed ({})", blockers.join("; "))
            }
        } else {
            "checked".to_string()
        };

        append_row(&Row {
            label: &label,
            rung: rung.name.clone(),
            rating,
            stockfish: &stockfish_name,
            tc: &tc,
            record,
            unfinished: tally.unfinished,
            faults: &tally.own_faults,
            opponent_faults: tally.opponent_faults.values().sum(),
            seed,
            outcome: &outcome,
            note: &args.note,
        })?;
        println!("ladder: result added to {}", paths::ladder_csv().display());

        let moved_up = outcome.starts_with("promoted");
        if !(args.climb && moved_up) {
            return Ok(());
        }
        rung_name = state.current.clone();
    }
}

/// A rating for display: a whole number, or `?` when the score was 0% or 100%.
fn whole(x: f64) -> String {
    if x.is_finite() {
        format!("{x:.0}")
    } else {
        "?".to_string()
    }
}

struct Row<'a> {
    label: &'a str,
    rung: String,
    rating: Option<Rating>,
    stockfish: &'a str,
    tc: &'a str,
    record: Record,
    unfinished: u32,
    faults: &'a BTreeMap<String, u32>,
    opponent_faults: u32,
    seed: u64,
    outcome: &'a str,
    note: &'a str,
}

const HEADER: &str = "date,commit,bench_nodes,rung,rung_rating,rung_bound,stockfish,tc,games,\
wins,draws,losses,unfinished,score_pct,elo,elo_low,elo_high,los_pct,est_rating,est_low,\
est_high,time_forfeits,illegal_moves,crashes,opponent_faults,seed,outcome,note";

fn append_row(row: &Row) -> Result<()> {
    let r = row.record;
    let (low, high) = r.elo_interval();
    let number = |x: f64| {
        if x.is_finite() {
            format!("{x:.0}")
        } else {
            String::new()
        }
    };
    let (rung_rating, rung_bound, est, est_low, est_high) = match row.rating {
        Some(rating) => (
            number(rating.value),
            match rating.bound {
                Bound::Exact => "exact",
                Bound::AtMost => "at most",
                Bound::AtLeast => "at least",
            }
            .to_string(),
            number(rating.value + r.elo()),
            number(rating.value + low),
            number(rating.value + high),
        ),
        None => Default::default(),
    };
    let fault = |kind: &str| row.faults.get(kind).copied().unwrap_or(0).to_string();
    let fields = [
        time::date_time(),
        row.label.to_string(),
        // Filled in once `backrank bench` exists (Phase 6).
        String::new(),
        row.rung.clone(),
        rung_rating,
        rung_bound,
        row.stockfish.to_string(),
        row.tc.to_string(),
        r.games().to_string(),
        r.wins.to_string(),
        r.draws.to_string(),
        r.losses.to_string(),
        row.unfinished.to_string(),
        format!("{:.1}", r.score() * 100.0),
        number(r.elo()),
        number(low),
        number(high),
        format!("{:.1}", r.los() * 100.0),
        est,
        est_low,
        est_high,
        fault("time forfeit"),
        fault("illegal move"),
        fault("abandoned"),
        row.opponent_faults.to_string(),
        row.seed.to_string(),
        row.outcome.to_string(),
        row.note.to_string(),
    ];
    append_csv(&paths::ladder_csv(), HEADER, &fields)
}

/// Appends one row to a CSV file, writing the header first if the file is new. Refuses to
/// add to a file whose columns are different.
pub fn append_csv(path: &PathBuf, header: &str, fields: &[String]) -> Result<()> {
    use std::io::Write;
    let new = !path.exists();
    if !new {
        let existing = std::fs::read_to_string(path)?;
        let first = existing.lines().next().unwrap_or_default();
        ensure!(
            first == header,
            "{} has different columns from this version of xtask; move it aside to start a new \
             one",
            path.display()
        );
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    if new {
        writeln!(file, "{header}")?;
    }
    let line: Vec<String> = fields.iter().map(|f| csv_field(f)).collect();
    writeln!(file, "{}", line.join(","))?;
    Ok(())
}

/// Quotes a CSV field if it needs it.
fn csv_field(field: &str) -> String {
    if field.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

/// Fails with a clear message if a rung needed for an estimate has no rating.
pub fn require_rating(ladder: &Ladder, rung: &Rung, calibration: &Calibration) -> Result<Rating> {
    match ladder.rating(rung, calibration) {
        Some(rating) => Ok(rating),
        None => bail!(
            "rung {} has no rating yet; run `cargo xtask calibrate` first",
            rung.name
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_committed_ladder_is_valid() {
        let ladder = Ladder::load().unwrap();
        assert_eq!(ladder.rungs[0].name, "random");
        assert_eq!(ladder.rungs.last().unwrap().name, "sf-full");
        let anchor = ladder.index("sf-elo1320").unwrap();
        assert!(ladder.rungs[..anchor].iter().all(|r| r.blunder.is_some()));
        assert!(load_state(&ladder).is_ok());
    }

    #[test]
    fn rejects_bad_ladders() {
        let parse = |text: &str| -> Result<Ladder> {
            let ladder: Ladder = toml::from_str(text)?;
            ladder.validate()?;
            Ok(ladder)
        };
        let defaults = "[defaults]\ntc = \"8+0.08\"\ngames = 240\nconcurrency = 8\n\
                        promote_at = 0.6\nmin_games = 200\nregression_below = 0.4\n";
        assert!(
            parse(&format!(
                "{defaults}[[rung]]\nname = \"a\"\nblunder = 0.5\n"
            ))
            .is_ok()
        );
        for bad in [
            "[[rung]]\nname = \"a\"\nblunder = 1.5\n",
            "[[rung]]\nname = \"a\"\nuci_elo = 1000\n",
            "[[rung]]\nname = \"a\"\nblunder = 0.5\nuci_elo = 1400\n",
            "[[rung]]\nname = \"a\"\n[[rung]]\nname = \"a\"\n",
            "[[rung]]\nname = \"backrank-x\"\n",
            "[[rung]]\nname = \"a\"\nskill = 3\n",
        ] {
            assert!(parse(&format!("{defaults}{bad}")).is_err(), "{bad}");
        }
    }

    #[test]
    fn rung_engines() {
        let ladder = Ladder::load().unwrap();
        let spec = |name: &str| {
            let rung = &ladder.rungs[ladder.index(name).unwrap()];
            rung.engine(Path::new("/proxy"), Path::new("/sf"))
        };
        let proxy = spec("blunder60");
        assert_eq!(proxy.cmd, Path::new("/proxy"));
        assert_eq!(proxy.args, ["--stockfish", "/sf", "--blunder", "0.6"]);
        let limited = spec("sf-elo1500");
        assert_eq!(limited.cmd, Path::new("/sf"));
        assert_eq!(
            limited.options,
            [
                ("UCI_LimitStrength".to_string(), "true".to_string()),
                ("UCI_Elo".to_string(), "1500".to_string())
            ]
        );
        assert!(spec("sf-full").options.is_empty());
    }

    #[test]
    fn ratings_come_from_stockfish_or_calibration() {
        let ladder = Ladder::load().unwrap();
        let rung = |name: &str| &ladder.rungs[ladder.index(name).unwrap()];
        let mut calibration = Calibration::new();
        assert_eq!(ladder.rating(rung("blunder10"), &calibration), None);
        assert_eq!(
            ladder
                .rating(rung("sf-elo1400"), &calibration)
                .unwrap()
                .value,
            1400.0
        );
        calibration.insert(
            "blunder10".into(),
            Calibrated {
                rating: 1100.0,
                bound: Bound::AtMost,
                blunder: 0.1,
                against: "sf-elo1320".into(),
                against_rating: 1320.0,
                games: 240,
                score: 0.2,
                tc: "8+0.08".into(),
                stockfish: "Stockfish 19".into(),
                date: "2026-11-05 12:00:00".into(),
            },
        );
        let rated = ladder.rating(rung("blunder10"), &calibration).unwrap();
        assert_eq!(rated.to_string(), "at most 1100");
        assert_eq!(ladder.rating(rung("sf-full"), &calibration), None);
        let fresh =
            ladder.stale_calibration(rung("blunder10"), &calibration, "Stockfish 19", "8+0.08");
        assert!(fresh.is_empty(), "{fresh:?}");
        let stale =
            ladder.stale_calibration(rung("blunder10"), &calibration, "Stockfish 20", "4+0.04");
        assert_eq!(stale.len(), 2, "{stale:?}");

        // A rating measured with a different blunder value no longer applies.
        calibration.get_mut("blunder10").unwrap().blunder = 0.15;
        assert_eq!(ladder.rating(rung("blunder10"), &calibration), None);
    }

    #[test]
    fn csv_fields_are_quoted_when_needed() {
        assert_eq!(csv_field("plain"), "plain");
        assert_eq!(csv_field("a, b"), "\"a, b\"");
        assert_eq!(csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
    }
}
