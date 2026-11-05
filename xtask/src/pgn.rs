//! Just enough PGN parsing to score a match: the tag pairs of each game.
//!
//! Results are counted from the PGN rather than from fastchess's console output, so the
//! harness does not depend on fastchess's report format.

use anyhow::{Context, Result, bail};
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    WhiteWins,
    BlackWins,
    Draw,
    /// `*`: the game was interrupted before it finished.
    Unfinished,
}

#[derive(Debug)]
pub struct Game {
    pub white: String,
    pub black: String,
    pub outcome: Outcome,
    /// fastchess writes one of: `normal`, `adjudication`, `abandoned` (crash, disconnect or
    /// stall), `time forfeit`, `illegal move`, `unterminated` (interrupted). `missing` if the
    /// game had no Termination tag.
    pub termination: String,
}

/// Parses every game in `text`. A game's tag section ends when its movetext begins.
pub fn parse(text: &str) -> Result<Vec<Game>> {
    let mut games = Vec::new();
    let mut tags: HashMap<String, String> = HashMap::new();
    let mut in_movetext = false;
    for line in text.lines().map(str::trim) {
        if let Some((name, value)) = parse_tag(line) {
            if in_movetext {
                games.push(build_game(std::mem::take(&mut tags))?);
                in_movetext = false;
            }
            tags.insert(name, value);
        } else if !line.is_empty() && !tags.is_empty() {
            in_movetext = true;
        }
    }
    if !tags.is_empty() {
        games.push(build_game(tags)?);
    }
    Ok(games)
}

fn parse_tag(line: &str) -> Option<(String, String)> {
    let inner = line.strip_prefix('[')?.strip_suffix(']')?;
    let (name, rest) = inner.split_once(' ')?;
    let quoted = rest.trim().strip_prefix('"')?.strip_suffix('"')?;
    let value = quoted.replace("\\\"", "\"").replace("\\\\", "\\");
    Some((name.to_string(), value))
}

fn build_game(mut tags: HashMap<String, String>) -> Result<Game> {
    let mut take = |name: &str| {
        tags.remove(name)
            .with_context(|| format!("PGN game is missing the [{name}] tag"))
    };
    let white = take("White")?;
    let black = take("Black")?;
    let outcome = match take("Result")?.as_str() {
        "1-0" => Outcome::WhiteWins,
        "0-1" => Outcome::BlackWins,
        "1/2-1/2" => Outcome::Draw,
        "*" => Outcome::Unfinished,
        other => bail!("unexpected PGN result {other:?}"),
    };
    // A missing tag is counted as abnormal rather than assumed normal, so a fastchess
    // change can't hide time forfeits or crashes.
    let termination = take("Termination").unwrap_or_else(|_| "missing".to_string());
    Ok(Game {
        white,
        black,
        outcome,
        termination,
    })
}

/// Results from one engine's point of view.
#[derive(Debug, Default, PartialEq)]
pub struct Tally {
    pub wins: u32,
    pub draws: u32,
    pub losses: u32,
    /// Games that never reached a result (`*`).
    pub unfinished: u32,
    /// Games that ended for a reason that points at a bug or an infrastructure problem
    /// (anything other than `normal` or `adjudication`), counted by termination.
    pub abnormal: BTreeMap<String, u32>,
}

impl Tally {
    /// Tallies every game `engine` played in.
    pub fn for_engine(games: &[Game], engine: &str) -> Tally {
        let mut tally = Tally::default();
        for game in games {
            let as_white = game.white == engine;
            if !as_white && game.black != engine {
                continue;
            }
            match (game.outcome, as_white) {
                (Outcome::WhiteWins, true) | (Outcome::BlackWins, false) => tally.wins += 1,
                (Outcome::WhiteWins, false) | (Outcome::BlackWins, true) => tally.losses += 1,
                (Outcome::Draw, _) => tally.draws += 1,
                (Outcome::Unfinished, _) => tally.unfinished += 1,
            }
            if !matches!(game.termination.as_str(), "normal" | "adjudication") {
                *tally.abnormal.entry(game.termination.clone()).or_default() += 1;
            }
        }
        tally
    }

    /// Finished games.
    pub fn games(&self) -> u32 {
        self.wins + self.draws + self.losses
    }

    /// Score in [0, 1]: a win is 1, a draw is 1/2.
    pub fn score(&self) -> f64 {
        if self.games() == 0 {
            return 0.0;
        }
        (f64::from(self.wins) + f64::from(self.draws) / 2.0) / f64::from(self.games())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"[Event "Fastchess Tournament"]
[White "sf-full"]
[Black "sf-skill0"]
[Result "1-0"]
[Termination "normal"]

1. e4 e5 2. Qh5 Nc6 3. Bc4 Nf6 4. Qxf7# 1-0

[Event "Fastchess Tournament"]
[White "sf-skill0"]
[Black "sf-full"]
[Result "1-0"]
[Termination "time forfeit"]

1. e4 {flagged} 1-0

[Event "Fastchess Tournament"]
[White "sf-full"]
[Black "sf-skill0"]
[Result "1/2-1/2"]
[Termination "adjudication"]

1. d4 d5 1/2-1/2
"#;

    #[test]
    fn parses_tags_of_every_game() {
        let games = parse(SAMPLE).unwrap();
        assert_eq!(games.len(), 3);
        assert_eq!(games[0].white, "sf-full");
        assert_eq!(games[1].outcome, Outcome::WhiteWins);
        assert_eq!(games[1].termination, "time forfeit");
        assert_eq!(games[2].outcome, Outcome::Draw);
    }

    #[test]
    fn tallies_from_one_engines_side() {
        let games = parse(SAMPLE).unwrap();
        let tally = Tally::for_engine(&games, "sf-full");
        assert_eq!((tally.wins, tally.draws, tally.losses), (1, 1, 1));
        assert_eq!(tally.abnormal.get("time forfeit"), Some(&1));
        assert_eq!(tally.abnormal.len(), 1);
        assert!((tally.score() - 0.5).abs() < 1e-9);
    }

    #[test]
    fn unescapes_tag_values() {
        let (name, value) = parse_tag(r#"[White "say \"hi\" \\ bye"]"#).unwrap();
        assert_eq!(name, "White");
        assert_eq!(value, r#"say "hi" \ bye"#);
    }

    #[test]
    fn missing_termination_counts_as_abnormal() {
        let games = parse("[White \"a\"]\n[Black \"b\"]\n[Result \"1-0\"]\n\n1. e4 1-0\n").unwrap();
        let tally = Tally::for_engine(&games, "a");
        assert_eq!(tally.wins, 1);
        assert_eq!(tally.abnormal.get("missing"), Some(&1));
    }

    #[test]
    fn rejects_game_without_result() {
        assert!(parse("[White \"a\"]\n[Black \"b\"]\n\n1. e4 *\n").is_err());
    }
}
