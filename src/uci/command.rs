//! Parsing lines from the GUI into commands.
//!
//! Parsing is tolerant: tokens may be separated by any whitespace, and unknown tokens are
//! skipped, as the UCI specification asks.

use std::str::FromStr;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Uci,
    Debug(bool),
    IsReady,
    SetOption {
        name: String,
        value: Option<String>,
    },
    Register,
    UciNewGame,
    /// The arguments of `position`. They are turned into a position by the engine thread,
    /// which can report errors and keep the previous position.
    Position(Vec<String>),
    Go(GoParams),
    Stop,
    PonderHit,
    Quit,
    /// Non-standard `d`: print the board, FEN and key.
    Display,
    /// Anything unrecognised, kept for the error message.
    Unknown(String),
}

/// The arguments of `go`. Times are in milliseconds and may be negative: some GUIs send
/// negative remaining time once a clock has run out.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GoParams {
    pub wtime: Option<i64>,
    pub btime: Option<i64>,
    pub winc: Option<i64>,
    pub binc: Option<i64>,
    pub movestogo: Option<u32>,
    pub depth: Option<u32>,
    pub nodes: Option<u64>,
    pub mate: Option<u32>,
    pub movetime: Option<i64>,
    pub infinite: bool,
    pub ponder: bool,
    pub searchmoves: Vec<String>,
    /// Non-standard `go perft <depth>`.
    pub perft: Option<u32>,
}

/// Parses one line from the GUI. Returns `None` for a blank line.
pub fn parse(line: &str) -> Option<Command> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    let (&first, rest) = tokens.split_first()?;
    Some(match first {
        "uci" => Command::Uci,
        "debug" => Command::Debug(rest.first() == Some(&"on")),
        "isready" => Command::IsReady,
        "setoption" => {
            parse_setoption(rest).unwrap_or_else(|| Command::Unknown(line.trim().into()))
        }
        "register" => Command::Register,
        "ucinewgame" => Command::UciNewGame,
        "position" => Command::Position(rest.iter().map(|t| t.to_string()).collect()),
        "go" => Command::Go(parse_go(rest)),
        "stop" => Command::Stop,
        "ponderhit" => Command::PonderHit,
        "quit" => Command::Quit,
        "d" => Command::Display,
        _ => Command::Unknown(line.trim().to_string()),
    })
}

/// `setoption name <name> [value <value>]`. Both the name and the value may contain spaces.
fn parse_setoption(tokens: &[&str]) -> Option<Command> {
    let (&first, rest) = tokens.split_first()?;
    if first != "name" {
        return None;
    }
    let value_at = rest.iter().position(|&t| t == "value");
    let (name, value) = match value_at {
        Some(i) => (&rest[..i], Some(rest[i + 1..].join(" "))),
        None => (rest, None),
    };
    if name.is_empty() {
        return None;
    }
    Some(Command::SetOption {
        name: name.join(" "),
        value,
    })
}

fn parse_go(tokens: &[&str]) -> GoParams {
    let mut params = GoParams::default();
    let mut i = 0;
    while i < tokens.len() {
        match tokens[i] {
            "wtime" => params.wtime = number(tokens, &mut i),
            "btime" => params.btime = number(tokens, &mut i),
            "winc" => params.winc = number(tokens, &mut i),
            "binc" => params.binc = number(tokens, &mut i),
            "movestogo" => params.movestogo = number(tokens, &mut i),
            "depth" => params.depth = number(tokens, &mut i),
            "nodes" => params.nodes = number(tokens, &mut i),
            "mate" => params.mate = number(tokens, &mut i),
            "movetime" => params.movetime = number(tokens, &mut i),
            "perft" => params.perft = number(tokens, &mut i),
            "infinite" => params.infinite = true,
            "ponder" => params.ponder = true,
            "searchmoves" => {
                while let Some(m) = tokens.get(i + 1).filter(|t| is_uci_move(t)) {
                    params.searchmoves.push(m.to_string());
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    params
}

/// Parses the token after `tokens[*i]` as a number, consuming it only if it parses.
fn number<T: FromStr>(tokens: &[&str], i: &mut usize) -> Option<T> {
    let value = tokens.get(*i + 1)?.parse().ok()?;
    *i += 1;
    Some(value)
}

/// True for strings shaped like a UCI move: `e2e4`, `e7e8q`.
fn is_uci_move(text: &str) -> bool {
    let b = text.as_bytes();
    let square = |f: u8, r: u8| (b'a'..=b'h').contains(&f) && (b'1'..=b'8').contains(&r);
    match b {
        [f1, r1, f2, r2] => square(*f1, *r1) && square(*f2, *r2),
        [f1, r1, f2, r2, p] => square(*f1, *r1) && square(*f2, *r2) && b"nbrq".contains(p),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_commands() {
        assert_eq!(parse("uci"), Some(Command::Uci));
        assert_eq!(parse("  isready  "), Some(Command::IsReady));
        assert_eq!(parse("ucinewgame"), Some(Command::UciNewGame));
        assert_eq!(parse("stop"), Some(Command::Stop));
        assert_eq!(parse("quit"), Some(Command::Quit));
        assert_eq!(parse("debug on"), Some(Command::Debug(true)));
        assert_eq!(parse("debug off"), Some(Command::Debug(false)));
        assert_eq!(parse("d"), Some(Command::Display));
        assert_eq!(parse(""), None);
        assert_eq!(parse("   \t "), None);
        assert_eq!(
            parse("xyzzy 1 2"),
            Some(Command::Unknown("xyzzy 1 2".into()))
        );
    }

    #[test]
    fn setoption_names_and_values_with_spaces() {
        assert_eq!(
            parse("setoption name Move Overhead value 50"),
            Some(Command::SetOption {
                name: "Move Overhead".into(),
                value: Some("50".into())
            })
        );
        assert_eq!(
            parse("setoption name Clear Hash"),
            Some(Command::SetOption {
                name: "Clear Hash".into(),
                value: None
            })
        );
        assert_eq!(
            parse("setoption name Book File value /tmp/my book.bin"),
            Some(Command::SetOption {
                name: "Book File".into(),
                value: Some("/tmp/my book.bin".into())
            })
        );
        assert!(matches!(
            parse("setoption value 3"),
            Some(Command::Unknown(_))
        ));
        assert!(matches!(parse("setoption name"), Some(Command::Unknown(_))));
    }

    #[test]
    fn position_arguments_are_kept_for_later() {
        assert_eq!(
            parse("position startpos moves e2e4"),
            Some(Command::Position(vec![
                "startpos".into(),
                "moves".into(),
                "e2e4".into()
            ]))
        );
    }

    #[test]
    fn go_with_clocks_and_limits() {
        let Some(Command::Go(go)) =
            parse("go wtime 30000 btime -150 winc 100 binc 100 movestogo 12")
        else {
            panic!("not a go command");
        };
        assert_eq!(go.wtime, Some(30_000));
        assert_eq!(go.btime, Some(-150));
        assert_eq!((go.winc, go.binc), (Some(100), Some(100)));
        assert_eq!(go.movestogo, Some(12));
        assert!(!go.infinite);

        let Some(Command::Go(go)) = parse("go depth 6 nodes 10000 movetime 500") else {
            panic!("not a go command");
        };
        assert_eq!(
            (go.depth, go.nodes, go.movetime),
            (Some(6), Some(10_000), Some(500))
        );
    }

    #[test]
    fn go_flags_searchmoves_and_perft() {
        let Some(Command::Go(go)) = parse("go infinite searchmoves e2e4 d2d4 ponder") else {
            panic!("not a go command");
        };
        assert!(go.infinite && go.ponder);
        assert_eq!(go.searchmoves, ["e2e4", "d2d4"]);

        let Some(Command::Go(go)) = parse("go perft 4") else {
            panic!("not a go command");
        };
        assert_eq!(go.perft, Some(4));
        assert_eq!(parse("go"), Some(Command::Go(GoParams::default())));
    }

    #[test]
    fn go_skips_bad_values_without_eating_keywords() {
        let Some(Command::Go(go)) = parse("go wtime infinite depth x movetime 100") else {
            panic!("not a go command");
        };
        assert_eq!(go.wtime, None);
        assert!(go.infinite);
        assert_eq!(go.depth, None);
        assert_eq!(go.movetime, Some(100));
    }
}
