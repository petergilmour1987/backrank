# Using Backrank

How to run the engine and talk to it. For installing it, see the
[README](../README.md#installing).

## Command line

```sh
backrank                                # UCI mode: reads commands on stdin
backrank perft <depth> [position]       # perft split by first move
backrank --version                      # print the version
backrank --help                         # print the usage
```

From a source checkout, use `cargo run --release --` in place of `backrank`.

## Using it as a UCI engine

Run with no arguments, `backrank` reads UCI commands on stdin and answers on stdout. To use
it in a chess GUI (Cute Chess, Arena, BanksiaGUI and so on), add the binary as a UCI engine.
If you built from source, it's at `target/release/backrank`. To try it by hand:

```
$ cargo run --release
uci
id name Backrank <version>
id author petergilmour1987
option name Move Overhead type spin default 20 min 0 max 5000
uciok
position startpos moves e2e4
go depth 5
info depth 1 seldepth 3 score cp 10 nodes 24 nps 24000 time 0 pv b8c6
info depth 2 seldepth 6 score cp -40 nodes 199 nps 199000 time 0 pv b8c6 b1c3
info depth 3 seldepth 7 score cp 10 nodes 1592 nps 1592000 time 0 pv b8c6 b1c3 g8f6
info depth 4 seldepth 12 score cp -35 nodes 16651 nps 16651000 time 1 pv g8f6 e4e5 f6e4 b1c3
info depth 5 seldepth 15 score cp 0 nodes 216590 nps 10313809 time 21 pv e7e5 b1c3 b8c6 g1f3 g8f6
bestmove e7e5
quit
```

It supports the standard commands: `uci`, `isready`, `setoption`, `ucinewgame`,
`position`, `go` (with `wtime`, `btime`, `winc`, `binc`, `movestogo`, `depth`, `nodes`,
`movetime` and `infinite`), `stop` and `quit`. A bare `go` searches until `stop`. `go mate`
and `searchmoves` are accepted but not yet supported, and pondering isn't offered.

| Option | Default | Effect |
|---|---|---|
| `Move Overhead` | 20 | Milliseconds kept in reserve per move for communication delays (0 to 5000). |

Two non-standard commands help with debugging:

| Command | Effect |
|---|---|
| `d` | Print the board, its FEN and its Zobrist key. |
| `go perft <depth>` | Perft from the current position, in Stockfish's output format. It runs in the background, so `stop` ends it early. |

If a `position` command is invalid (a bad FEN or an illegal move), the engine reports it
with `info string` and keeps the previous position.

## Perft

`perft` counts every legal move sequence to a given depth, which is how move generation
is tested. The output is split by first move and uses the same format as Stockfish's
`go perft`:

```sh
cargo run --release -- perft 5                                  # from the start position
cargo run --release -- perft 4 fen "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1"
cargo run --release -- perft 3 startpos moves e2e4 e7e5         # after some moves
```

The position follows UCI `position` syntax: `[startpos | fen <FEN>] [moves <move>...]`.
Timing goes to stderr, so stdout can be compared with Stockfish's directly.
