<div align="center">

  <img src="icon.png" alt="Backrank" width="128">

  <h3>Backrank</h3>

  A free UCI chess engine, written in Rust.
  <br>
  <strong>[Explore Backrank docs »][usage-link]</strong>
  <br>
  <br>
  [Download][releases-link]
  ·
  [Report bug][issue-link]
  ·
  [Changelog][changelog-link]
  ·
  [Roadmap][roadmap-link]

  [![Release pipeline][pipeline-badge]][pipeline-link]
  [![License][license-badge]][license-link]
  <br>
  [![Latest release][release-badge]][releases-link]
  [![Commits since release][commits-badge]][commits-link]
  [![Downloads][downloads-badge]][releases-link]
  <br>
  [![Stockfish ladder][ladder-badge]][ladder-link]
  [![Rust version][rust-badge]][cargo-link]
  <br>
  [![Protocol: UCI][uci-badge]][uci-link]
  [![Platforms][platforms-badge]][releases-link]
  [![Engine dependencies: none][deps-badge]][cargo-link]

</div>

## Overview

Backrank is a **free UCI chess engine** written in Rust. It analyses chess positions and
chooses a move using alpha-beta search with iterative deepening, quiescence search, move
ordering and time management, and it evaluates positions by material and piece placement.

Backrank **does not include a graphical user interface** (GUI). To play against it or analyse
with it, add it as a UCI engine to a GUI such as Cute Chess, Arena or BanksiaGUI. See the
[documentation][usage-link] for the commands it supports.

**Status:** in development towards v1. Backrank is measured by playing Stockfish's
strength-limited settings on a difficulty ladder that gets harder as it improves. At 8+0.08, it
beats `UCI_Elo` 1900 and scores 58.5% against 2000, for an estimated rating of about 2060. That
estimate is probably optimistic at this fast time control. Every run is recorded in
[`results/ladder.csv`][ladder-link].

## Installing

Prebuilt binaries are on the
[releases page](https://github.com/petergilmour1987/backrank/releases). Until
v1.0.0, releases are marked as pre-releases.

| Archive | For |
|---|---|
| `backrank-macos-universal.tar.gz` | macOS on Apple Silicon or Intel |
| `backrank-linux-x86-64.tar.gz` | Linux on x86-64, any distribution (statically linked) |
| `backrank-linux-arm64.tar.gz` | Linux on ARM64, any distribution (statically linked) |
| `backrank-windows-x86-64.zip` | Windows on x86-64 |
| `backrank-windows-arm64.zip` | Windows on ARM64 |

Unpack the archive and add the `backrank` binary to your chess GUI as a UCI engine. x86-64
builds need a CPU with POPCNT and SSE4.2, which nearly every PC from the last 15 years has.
If yours doesn't, [build from source](#compiling-backrank).

The binaries aren't code signed yet:

- **macOS:** a browser download is blocked until you run
  `xattr -d com.apple.quarantine backrank`. A chess GUI may fail to start the engine until
  you do. Downloads made with `curl` aren't affected.
- **Windows:** SmartScreen or your antivirus may warn about an unknown publisher.

Each release includes a `SHA256SUMS` file, so you can check a download with
`sha256sum -c SHA256SUMS --ignore-missing` (on macOS,
`shasum -a 256 -c SHA256SUMS --ignore-missing`). To check that GitHub Actions built it from
this repository, run `gh attestation verify <file> --repo petergilmour1987/backrank`. The
repository used to be called `backrank-chess-engine` and v0.2.0 was built under that name, so
for that release use `--repo petergilmour1987/backrank-chess-engine`.

## Files

Each release archive contains the `backrank` binary, [`README.md`][readme-link] and
[`LICENSE`][license-link]. The source repository also has:

  * [`src`][src-link], the engine: a library holding all the chess logic, and a thin binary.

  * [`tests`][tests-link], integration tests, including perft and the UCI protocol.

  * [`docs`][docs-link], the [usage guide][usage-link] and the project's plans.

  * [`xtask`][xtask-link], development tooling for matches and releases, run as `cargo xtask`.

  * [`tools`][tools-link], the definition of the Stockfish ladder.

  * [`results`][results-link], the history of every ladder and SPRT run.

  * [`CHANGELOG.md`][changelog-link] and [`CONTRIBUTING.md`][contributing-link].

## Contributing

__See the [Contributing Guide][contributing-link].__

### Testing strength

A change to search or evaluation has to prove itself before it merges. It first plays the
version it would replace in a sequential probability ratio test (SPRT), and then plays the
current rung of the Stockfish ladder. Both results are committed to
[`results`][results-link] alongside the change.

### Improving the code

The [chessprogramming wiki][programming-link] explains many of the techniques Backrank uses,
with plenty of background. The ideas to try next are listed in the [roadmap][roadmap-link].

## Compiling Backrank

Backrank is written in Rust, and the engine uses only the standard library. It builds on macOS,
Linux and Windows, on x86-64 and ARM64. Install Rust with [rustup][rustup-link], which picks up
the version pinned in `rust-toolchain.toml` by itself, and then run:

```
cargo build --release
```

The binary is at `target/release/backrank`.

## Terms of use

Copyright (C) 2026 petergilmour1987

Backrank is free software: you can redistribute it and/or modify it under the terms of the
[**GNU General Public License**][license-link] as published by the Free Software Foundation,
either version 3 of the License, or (at your option) any later version.

In practice, you can use it however you like, including sharing it, selling it, or building your
own project on it. If you distribute Backrank or something based on it, you must include the
license and make the full source code available under the same terms.

It is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even
the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.

## Acknowledgements

Backrank's evaluation is [Tomasz Michniewski's Simplified Evaluation Function][eval-link]. Its
strength is tested against [Stockfish][stockfish-link], using [fastchess][fastchess-link] to run
the matches and opening books from [official-stockfish/books][books-link]. These test tools
aren't part of the release binaries.


[books-link]:         https://github.com/official-stockfish/books
[cargo-link]:         https://github.com/petergilmour1987/backrank/blob/main/Cargo.toml
[changelog-link]:     https://github.com/petergilmour1987/backrank/blob/main/CHANGELOG.md
[commits-link]:       https://github.com/petergilmour1987/backrank/commits/main
[contributing-link]:  https://github.com/petergilmour1987/backrank/blob/main/CONTRIBUTING.md
[docs-link]:          https://github.com/petergilmour1987/backrank/tree/main/docs
[eval-link]:          https://www.chessprogramming.org/Simplified_Evaluation_Function
[fastchess-link]:     https://github.com/Disservin/fastchess
[issue-link]:         https://github.com/petergilmour1987/backrank/issues/new
[ladder-link]:        https://github.com/petergilmour1987/backrank/blob/main/results/ladder.csv
[license-link]:       https://github.com/petergilmour1987/backrank/blob/main/LICENSE
[pipeline-link]:      https://github.com/petergilmour1987/backrank/actions/workflows/release.yml
[programming-link]:   https://www.chessprogramming.org/Main_Page
[readme-link]:        https://github.com/petergilmour1987/backrank/blob/main/README.md
[releases-link]:      https://github.com/petergilmour1987/backrank/releases
[results-link]:       https://github.com/petergilmour1987/backrank/tree/main/results
[roadmap-link]:       https://github.com/petergilmour1987/backrank/blob/main/docs/plan-v1.md
[rustup-link]:        https://rustup.rs
[src-link]:           https://github.com/petergilmour1987/backrank/tree/main/src
[stockfish-link]:     https://stockfishchess.org
[tests-link]:         https://github.com/petergilmour1987/backrank/tree/main/tests
[tools-link]:         https://github.com/petergilmour1987/backrank/tree/main/tools
[uci-link]:           https://backscattering.de/chess/uci/
[usage-link]:         https://github.com/petergilmour1987/backrank/blob/main/docs/usage.md
[xtask-link]:         https://github.com/petergilmour1987/backrank/tree/main/xtask

[commits-badge]:      https://img.shields.io/github/commits-since/petergilmour1987/backrank/latest?include_prereleases&style=for-the-badge&labelColor=000000&color=ff0000
[deps-badge]:         https://img.shields.io/badge/engine%20dependencies-none-success?style=for-the-badge&labelColor=000000
[downloads-badge]:    https://img.shields.io/github/downloads/petergilmour1987/backrank/total?style=for-the-badge&labelColor=000000&color=ff0000
[ladder-badge]:       https://img.shields.io/badge/dynamic/toml?url=https%3A%2F%2Fraw.githubusercontent.com%2Fpetergilmour1987%2Fbackrank%2Fmain%2Fresults%2Fstate.toml&query=%24.current&style=for-the-badge&labelColor=000000&label=stockfish%20ladder&color=ff0000
[license-badge]:      https://img.shields.io/github/license/petergilmour1987/backrank?style=for-the-badge&labelColor=000000&label=license&color=success
[pipeline-badge]:     https://img.shields.io/github/actions/workflow/status/petergilmour1987/backrank/release.yml?branch=main&style=for-the-badge&labelColor=000000&label=release%20pipeline&logo=github
[platforms-badge]:    https://img.shields.io/badge/platforms-macOS%20%7C%20Linux%20%7C%20Windows-ff0000?style=for-the-badge&labelColor=000000
[release-badge]:      https://img.shields.io/github/v/release/petergilmour1987/backrank?include_prereleases&style=for-the-badge&labelColor=000000&label=latest%20release&color=ff0000
[rust-badge]:         https://img.shields.io/badge/dynamic/toml?url=https%3A%2F%2Fraw.githubusercontent.com%2Fpetergilmour1987%2Fbackrank%2Fmain%2FCargo.toml&query=%24.package%5B%27rust-version%27%5D&suffix=%2B&style=for-the-badge&labelColor=000000&label=rust&logo=rust&color=ff0000
[uci-badge]:          https://img.shields.io/badge/protocol-UCI-ff0000?style=for-the-badge&labelColor=000000
