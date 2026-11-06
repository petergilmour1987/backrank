//! The version rules: `cargo xtask bump`, `check-release` and `verify-binary`.
//!
//! A version is bumped on the branch, as the last commit of a pull request, so the merge
//! commit already holds the version its tag names. `docs/plan-release.md` has the reasoning.

use crate::{engine, paths, time};
use anyhow::{Context, Result, anyhow, bail, ensure};
use backrank::{ENGINE_NAME, ENGINE_VERSION, movegen, uci};
use std::fmt;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

const UNRELEASED: &str = "## [Unreleased]";

/// Long enough for any reply on a slow CI machine; a stuck engine fails instead of hanging.
const VERIFY_TIMEOUT: Duration = Duration::from_secs(60);

/// A real engine says far less than this to `uci` or `go depth 8`; more means it's stuck.
const MAX_REPLY_LINES: usize = 10_000;

/// Which part of the version to raise.
#[derive(Clone, Copy, clap::ValueEnum)]
pub enum Level {
    Major,
    Minor,
    Patch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Version {
    major: u32,
    minor: u32,
    patch: u32,
}

impl Version {
    fn parse(text: &str) -> Result<Version> {
        let invalid = || format!("{text:?} is not a major.minor.patch version");
        let parts: Vec<&str> = text.split('.').collect();
        let [major, minor, patch] = parts[..] else {
            bail!(invalid());
        };
        Ok(Version {
            major: major.parse().with_context(invalid)?,
            minor: minor.parse().with_context(invalid)?,
            patch: patch.parse().with_context(invalid)?,
        })
    }

    fn bumped(self, level: Level) -> Version {
        match level {
            Level::Major => Version {
                major: self.major + 1,
                minor: 0,
                patch: 0,
            },
            Level::Minor => Version {
                minor: self.minor + 1,
                patch: 0,
                ..self
            },
            Level::Patch => Version {
                patch: self.patch + 1,
                ..self
            },
        }
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

fn package_version(manifest: &str) -> Result<Version> {
    let table: toml::Table = toml::from_str(manifest).context("Cargo.toml doesn't parse")?;
    let version = table
        .get("package")
        .and_then(|package| package.get("version"))
        .and_then(|version| version.as_str())
        .context("Cargo.toml has no package.version")?;
    Version::parse(version)
}

/// `manifest` with `package.version` replaced, and everything else left exactly as written.
fn with_package_version(manifest: &str, version: Version) -> Result<String> {
    let mut in_package = false;
    let mut replaced = false;
    let mut out = String::with_capacity(manifest.len());
    for line in manifest.split_inclusive('\n') {
        if line.starts_with('[') {
            in_package = line.trim_end() == "[package]";
        }
        if in_package && !replaced && line.starts_with("version") {
            out.push_str(&format!("version = \"{version}\"\n"));
            replaced = true;
        } else {
            out.push_str(line);
        }
    }
    ensure!(replaced, "Cargo.toml has no version line under [package]");
    Ok(out)
}

/// The byte range of the text under the line starting with `heading`, up to the next
/// version heading.
fn section_range(changelog: &str, heading: &str) -> Option<(usize, usize)> {
    let mut offset = 0;
    let mut start = None;
    for line in changelog.split_inclusive('\n') {
        let next = offset + line.len();
        match start {
            None if line.starts_with(heading) => start = Some(next),
            Some(from) if line.starts_with("## [") => return Some((from, offset)),
            _ => {}
        }
        offset = next;
    }
    start.map(|from| (from, changelog.len()))
}

/// Whether a changelog section says anything, beyond headings such as `### Added`.
fn has_notes(section: &str) -> bool {
    section
        .lines()
        .map(str::trim)
        .any(|line| !line.is_empty() && !line.starts_with('#'))
}

/// `changelog` with a heading for `version` added, so the unreleased notes now sit under it.
fn release_changelog(changelog: &str, version: Version, date: &str) -> Result<String> {
    let (start, end) = section_range(changelog, UNRELEASED)
        .with_context(|| format!("CHANGELOG.md has no `{UNRELEASED}` heading"))?;
    ensure!(
        has_notes(&changelog[start..end]),
        "CHANGELOG.md has nothing under `{UNRELEASED}`: write the release notes first"
    );
    Ok(format!(
        "{}\n## [{version}] - {date}\n{}",
        &changelog[..start],
        &changelog[start..]
    ))
}

#[derive(clap::Args)]
pub struct BumpArgs {
    /// Major for a breaking UCI or command line change, minor for a change meant to alter
    /// play or a new feature, patch for a fix or speed-up.
    #[arg(value_enum)]
    level: Level,
    /// Stage the changes instead of committing them.
    #[arg(long)]
    no_commit: bool,
}

pub fn bump(args: &BumpArgs) -> Result<()> {
    const BASE: &str = "origin/main";
    ensure!(
        engine::git(&["rev-parse", "--abbrev-ref", "HEAD"])? != "main",
        "bump on a feature branch: main only changes through pull requests"
    );
    // Offline, the last fetched main still gives a usable answer, and CI has the final say.
    let _ = engine::git(&["fetch", "--quiet", "origin", "main"]);
    ensure!(
        engine::git(&["merge-base", "--is-ancestor", BASE, "HEAD"]).is_ok(),
        "merge {BASE} into this branch first, so the new version follows the latest one"
    );
    let dirty = engine::git(&[
        "diff",
        "--name-only",
        "HEAD",
        "--",
        ".",
        ":(exclude)CHANGELOG.md",
    ])?;
    ensure!(dirty.is_empty(), "commit or stash these first:\n{dirty}");

    let manifest_path = paths::root().join("Cargo.toml");
    let original_manifest = std::fs::read_to_string(&manifest_path)?;
    let current = package_version(&original_manifest)?;
    let base = package_version(&engine::git(&["show", &format!("{BASE}:Cargo.toml")])?)?;
    ensure!(
        current == base,
        "this branch already moves the version from {base} to {current}; \
         revert that change to bump again"
    );
    let next = base.bumped(args.level);

    let changelog_path = paths::root().join("CHANGELOG.md");
    let original_changelog =
        std::fs::read_to_string(&changelog_path).context("can't read CHANGELOG.md")?;
    let date = time::date_time();
    let changelog = release_changelog(&original_changelog, next, &date[..10])?;
    let manifest = with_package_version(&original_manifest, next)?;

    let lock_path = paths::root().join("Cargo.lock");
    let originals = [
        (&manifest_path, original_manifest.into_bytes()),
        (&changelog_path, original_changelog.into_bytes()),
        (&lock_path, std::fs::read(&lock_path)?),
    ];
    let result = apply_bump(
        &manifest_path,
        manifest.as_bytes(),
        &changelog_path,
        changelog.as_bytes(),
        next,
        args.no_commit,
    );
    if let Err(err) = result {
        // Put everything back, so fixing the cause and running it again just works. The
        // changelog may hold uncommitted notes, so restore its bytes rather than checking
        // it out.
        for (path, bytes) in &originals {
            let _ = engine::write_atomic(path, bytes);
        }
        let _ = engine::git(&[
            "reset",
            "--quiet",
            "--",
            "Cargo.toml",
            "Cargo.lock",
            "CHANGELOG.md",
        ]);
        return Err(err.context("the bump failed, and its changes were undone"));
    }
    if args.no_commit {
        println!("bump: {base} to {next}, staged");
    } else {
        println!("bump: {base} to {next}, committed");
    }
    Ok(())
}

fn apply_bump(
    manifest_path: &Path,
    manifest: &[u8],
    changelog_path: &Path,
    changelog: &[u8],
    next: Version,
    no_commit: bool,
) -> Result<()> {
    engine::write_atomic(manifest_path, manifest)?;
    engine::write_atomic(changelog_path, changelog)?;
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let status = Command::new(cargo)
        .args(["update", "--workspace", "--offline", "--quiet"])
        .current_dir(paths::root())
        .status()
        .context("failed to run cargo update")?;
    ensure!(status.success(), "updating Cargo.lock failed");
    engine::git(&["add", "Cargo.toml", "Cargo.lock", "CHANGELOG.md"])?;
    if !no_commit {
        engine::git(&["commit", "--quiet", "-m", &format!("Release v{next}")])?;
    }
    Ok(())
}

/// Whether a changed path can change what the release binaries do.
fn ships(path: &str) -> bool {
    path.starts_with("src/")
        || path.starts_with(".cargo/")
        || path == "Cargo.toml"
        || path == "rust-toolchain.toml"
}

/// What the version rules look at in a pull request.
struct Facts<'a> {
    base: Version,
    head: Version,
    changed: &'a [String],
    base_tagged: bool,
    head_tagged: bool,
    changelog: &'a str,
    sprt_rows_added: u32,
    ladder_rows_added: u32,
    no_strength_test: bool,
}

fn problems(facts: &Facts) -> Vec<String> {
    let (base, head) = (facts.base, facts.head);
    let mut problems = Vec::new();
    if head < base {
        problems.push(format!("the version goes down, from {base} to {head}"));
    } else if head == base {
        // An untagged version on main never got released, so a fix can ship under it.
        if facts.base_tagged && facts.changed.iter().any(|path| ships(path)) {
            problems.push(format!(
                "this changes what ships, but the version is still {base}: \
                 run `cargo xtask bump <major|minor|patch>`"
            ));
        }
    } else {
        let steps = [Level::Major, Level::Minor, Level::Patch].map(|level| base.bumped(level));
        if !steps.contains(&head) {
            problems.push(format!(
                "{head} isn't one step up from {base}: expected {}, {} or {}",
                steps[0], steps[1], steps[2]
            ));
        }
        if facts.head_tagged {
            problems.push(format!("tag v{head} already exists"));
        }
        let notes = section_range(facts.changelog, &format!("## [{head}]"))
            .map(|(start, end)| &facts.changelog[start..end]);
        if !notes.is_some_and(has_notes) {
            problems.push(format!("CHANGELOG.md has no notes under `## [{head}]`"));
        }
    }

    let changes_play = facts
        .changed
        .iter()
        .any(|path| path.starts_with("src/search/") || path.starts_with("src/eval/"));
    if changes_play && !facts.no_strength_test {
        for (rows, file) in [
            (facts.sprt_rows_added, "results/sprt.csv"),
            (facts.ladder_rows_added, "results/ladder.csv"),
        ] {
            if rows == 0 {
                problems.push(format!(
                    "this changes search or eval, but adds no rows to {file}: \
                     commit the run, or label the pull request `no-strength-test`"
                ));
            }
        }
    }
    problems
}

#[derive(clap::Args)]
pub struct CheckArgs {
    /// The branch the pull request merges into.
    #[arg(long, default_value = "origin/main")]
    base: String,
    /// Accept a search or eval change without new SPRT and ladder rows, for one that can't
    /// change play.
    #[arg(long)]
    no_strength_test: bool,
}

fn tag_exists(version: Version) -> bool {
    engine::git(&[
        "rev-parse",
        "--quiet",
        "--verify",
        &format!("refs/tags/v{version}"),
    ])
    .is_ok()
}

fn rows_added(range: &str, file: &str) -> Result<u32> {
    net_rows_added(&engine::git(&["diff", "--numstat", range, "--", file])?)
}

/// New lines in a `git diff --numstat` line. An edited row shows as one added and one deleted,
/// which isn't a new run.
fn net_rows_added(numstat: &str) -> Result<u32> {
    let mut columns = numstat.split_whitespace();
    let (Some(added), Some(deleted)) = (columns.next(), columns.next()) else {
        return Ok(0);
    };
    let count = |text: &str| -> Result<u32> {
        text.parse()
            .with_context(|| format!("unexpected numstat: {numstat}"))
    };
    Ok(count(added)?.saturating_sub(count(deleted)?))
}

pub fn check_release(args: &CheckArgs) -> Result<()> {
    let base = package_version(&engine::git(&[
        "show",
        &format!("{}:Cargo.toml", args.base),
    ])?)?;
    let head = package_version(&engine::git(&["show", "HEAD:Cargo.toml"])?)?;
    let range = format!("{}...HEAD", args.base);
    // Without renames, moving a file reports both its old and new path. With -z, unusual
    // names aren't quoted, which would hide them from the path rules.
    let changed: Vec<String> = engine::git(&["diff", "--name-only", "--no-renames", "-z", &range])?
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(str::to_string)
        .collect();
    let changelog = engine::git(&["show", "HEAD:CHANGELOG.md"]).unwrap_or_default();
    let problems = problems(&Facts {
        base,
        head,
        changed: &changed,
        base_tagged: tag_exists(base),
        head_tagged: tag_exists(head),
        changelog: &changelog,
        sprt_rows_added: rows_added(&range, "results/sprt.csv")?,
        ladder_rows_added: rows_added(&range, "results/ladder.csv")?,
        no_strength_test: args.no_strength_test,
    });
    if problems.is_empty() {
        println!(
            "check-release: OK ({base} to {head}, {} files)",
            changed.len()
        );
        return Ok(());
    }
    for problem in &problems {
        eprintln!("check-release: {problem}");
    }
    bail!("the pull request breaks {} version rules", problems.len())
}

/// The end of a reply, for error messages.
fn last_lines(lines: &[String]) -> &[String] {
    &lines[lines.len().saturating_sub(5)..]
}

/// An engine process being talked to over pipes.
struct Session {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
}

impl Session {
    fn start(binary: &Path, args: &[&str]) -> Result<Session> {
        let mut child = Command::new(binary)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("failed to start {}", binary.display()))?;
        let stdin = child.stdin.take().expect("stdin is piped");
        let stdout = child.stdout.take().expect("stdout is piped");
        // Bounded, so an engine stuck printing can't fill memory faster than it's read.
        let (sender, lines) = mpsc::sync_channel(1024);
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        Ok(Session {
            child,
            stdin,
            lines,
        })
    }

    fn send(&mut self, command: &str) -> Result<()> {
        writeln!(self.stdin, "{command}")?;
        self.stdin.flush()?;
        Ok(())
    }

    /// Reads lines up to and including the first one for which `done` is true.
    fn read_until(&self, done: impl Fn(&str) -> bool) -> Result<Vec<String>> {
        let deadline = Instant::now() + VERIFY_TIMEOUT;
        let mut lines = Vec::new();
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            // Checked here too, as `recv_timeout` keeps returning lines that are already
            // waiting, however late it is.
            ensure!(
                !left.is_zero(),
                "timed out; last lines {:?}",
                last_lines(&lines)
            );
            ensure!(
                lines.len() < MAX_REPLY_LINES,
                "more than {MAX_REPLY_LINES} lines without the expected reply; last lines {:?}",
                last_lines(&lines)
            );
            match self.lines.recv_timeout(left) {
                Ok(line) => {
                    let finished = done(&line);
                    lines.push(line);
                    if finished {
                        return Ok(lines);
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    bail!("timed out; last lines {:?}", last_lines(&lines))
                }
                Err(RecvTimeoutError::Disconnected) => {
                    bail!("engine exited; last lines {:?}", last_lines(&lines))
                }
            }
        }
    }

    /// Waits for the process to exit by itself and checks that it succeeded.
    fn expect_clean_exit(&mut self) -> Result<()> {
        let deadline = Instant::now() + VERIFY_TIMEOUT;
        while Instant::now() < deadline {
            if let Some(status) = self.child.try_wait()? {
                ensure!(status.success(), "engine exited with {status}");
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        bail!("engine did not exit after `quit`")
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[derive(clap::Args)]
pub struct VerifyArgs {
    /// The binary to test, such as the one unpacked from a release archive.
    binary: PathBuf,
}

/// Runs a built binary as a GUI would. It must report this checkout's version and play a
/// legal move, which also proves it starts at all on this machine.
pub fn verify_binary(args: &VerifyArgs) -> Result<()> {
    let binary = &args.binary;
    let expected = format!("backrank {ENGINE_VERSION}");
    let mut version = Session::start(binary, &["--version"])?;
    let reported = version
        .read_until(|_| true)
        .context("`--version` printed nothing")?;
    ensure!(
        reported[0].trim() == expected,
        "`--version` should print `{expected}`, got {:?}",
        reported[0]
    );
    version.expect_clean_exit()?;

    let mut engine = Session::start(binary, &[])?;
    engine.send("uci")?;
    let lines = engine.read_until(|line| line == "uciok")?;
    let id = format!("id name {ENGINE_NAME} {ENGINE_VERSION}");
    ensure!(
        lines.contains(&id),
        "`uci` should report `{id}`, got {lines:?}"
    );
    engine.send("isready")?;
    engine.read_until(|line| line == "readyok")?;
    engine.send("position startpos")?;
    engine.send("go depth 8")?;
    let lines = engine.read_until(|line| line.starts_with("bestmove "))?;
    ensure!(
        lines.iter().any(|line| line.starts_with("info depth 8 ")),
        "`go depth 8` should reach depth 8, got {:?}",
        last_lines(&lines)
    );
    let reply = lines.last().expect("read_until returns the final line");
    let best = reply
        .split_whitespace()
        .nth(1)
        .with_context(|| format!("no move in {reply:?}"))?;
    let mut pos = uci::parse_position(&["startpos"]).map_err(|e| anyhow!("{e}"))?;
    ensure!(
        movegen::find_legal_move(&mut pos, best).is_some(),
        "{best} is not a legal move from the start position"
    );
    engine.send("quit")?;
    engine.expect_clean_exit()?;
    println!(
        "verify-binary: {} plays {best}: OK",
        id.trim_start_matches("id name ")
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    #[test]
    fn versions_parse_and_bump() {
        assert_eq!(v("1.2.3").to_string(), "1.2.3");
        assert!(Version::parse("1.2").is_err());
        assert!(Version::parse("1.2.3.4").is_err());
        assert!(Version::parse("1.2.x").is_err());
        assert_eq!(v("1.2.3").bumped(Level::Patch), v("1.2.4"));
        assert_eq!(v("1.2.3").bumped(Level::Minor), v("1.3.0"));
        assert_eq!(v("1.2.3").bumped(Level::Major), v("2.0.0"));
        assert_eq!(v("0.9.1").bumped(Level::Major), v("1.0.0"));
        assert!(v("0.10.0") > v("0.9.9"));
    }

    #[test]
    fn replaces_only_the_package_version() {
        let manifest = "[workspace]\nversion = \"9.9.9\"\n\n[package]\nname = \"backrank\"\n\
                        version = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\n\
                        version = \"1.0\"\n";
        let updated = with_package_version(manifest, v("0.2.0")).unwrap();
        assert_eq!(
            updated,
            manifest.replace("version = \"0.1.0\"", "version = \"0.2.0\"")
        );
        assert!(with_package_version("[workspace]\n", v("0.2.0")).is_err());
    }

    #[test]
    fn reads_the_real_manifest_and_changelog() {
        let root = paths::root();
        let manifest = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();
        let version = package_version(&manifest).unwrap();
        assert_eq!(version.to_string(), ENGINE_VERSION);
        let changelog = std::fs::read_to_string(root.join("CHANGELOG.md")).unwrap();
        assert!(section_range(&changelog, UNRELEASED).is_some());
    }

    const CHANGELOG: &str = "# Changelog\n\n## [Unreleased]\n\n### Fixed\n\n- A bug.\n\n\
                             ## [0.1.0] - 2026-01-01\n\n- First.\n";

    #[test]
    fn finds_changelog_sections() {
        let section = |heading| {
            section_range(CHANGELOG, heading).map(|(start, end)| CHANGELOG[start..end].trim())
        };
        assert_eq!(section(UNRELEASED), Some("### Fixed\n\n- A bug."));
        assert_eq!(section("## [0.1.0]"), Some("- First."));
        assert_eq!(section("## [0.2.0]"), None);
    }

    #[test]
    fn moves_unreleased_notes_under_the_new_version() {
        let released = release_changelog(CHANGELOG, v("0.2.0"), "2026-02-03").unwrap();
        assert_eq!(
            released,
            "# Changelog\n\n## [Unreleased]\n\n## [0.2.0] - 2026-02-03\n\n### Fixed\n\n\
             - A bug.\n\n## [0.1.0] - 2026-01-01\n\n- First.\n"
        );
        // Headings alone aren't notes.
        let headings = "## [Unreleased]\n\n### Added\n\n## [0.1.0]\n\n- First.\n";
        assert!(release_changelog(headings, v("0.2.0"), "2026-02-03").is_err());
        // Nothing is left to release, so a second bump is refused.
        assert!(release_changelog(&released, v("0.3.0"), "2026-02-04").is_err());
        assert!(release_changelog("# Changelog\n", v("0.2.0"), "2026-02-03").is_err());
    }

    /// A pull request that changes the engine, bumps it and has notes.
    fn good_facts<'a>(changed: &'a [String], changelog: &'a str) -> Facts<'a> {
        Facts {
            base: v("0.1.0"),
            head: v("0.2.0"),
            changed,
            base_tagged: true,
            head_tagged: false,
            changelog,
            sprt_rows_added: 0,
            ladder_rows_added: 0,
            no_strength_test: false,
        }
    }

    const NOTES: &str = "## [Unreleased]\n\n## [0.2.0] - 2026-02-03\n\n- New.\n";

    fn paths(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn a_bump_with_notes_passes() {
        let changed = paths(&["src/main.rs"]);
        assert_eq!(problems(&good_facts(&changed, NOTES)), Vec::<String>::new());
    }

    #[test]
    fn changes_that_ship_need_a_bump() {
        for path in [
            "src/main.rs",
            "Cargo.toml",
            "rust-toolchain.toml",
            ".cargo/config.toml",
        ] {
            let changed = paths(&[path]);
            let facts = Facts {
                head: v("0.1.0"),
                ..good_facts(&changed, "")
            };
            assert_eq!(problems(&facts).len(), 1, "{path}");
        }
    }

    #[test]
    fn changes_that_do_not_ship_need_no_bump_even_on_a_released_version() {
        let changed = paths(&[
            "README.md",
            "docs/plan-v1.md",
            "results/ladder.csv",
            "xtask/src/main.rs",
            "xtask/Cargo.toml",
            "Cargo.lock",
            "tests/uci.rs",
            ".github/workflows/ci.yml",
        ]);
        let facts = Facts {
            head: v("0.1.0"),
            head_tagged: true,
            ..good_facts(&changed, "")
        };
        assert_eq!(problems(&facts), Vec::<String>::new());
    }

    #[test]
    fn a_fix_can_ship_under_a_version_that_never_got_released() {
        let changed = paths(&["src/main.rs"]);
        let facts = Facts {
            head: v("0.1.0"),
            base_tagged: false,
            ..good_facts(&changed, "")
        };
        assert_eq!(problems(&facts), Vec::<String>::new());
    }

    #[test]
    fn rejects_bad_version_changes() {
        let changed = paths(&["src/main.rs"]);
        let with = |head, tagged, changelog| {
            problems(&Facts {
                head: v(head),
                head_tagged: tagged,
                ..good_facts(&changed, changelog)
            })
        };
        assert!(with("0.0.9", false, NOTES)[0].contains("goes down"));
        assert!(with("0.3.0", false, "## [0.3.0]\n\n- New.\n")[0].contains("one step"));
        assert!(with("0.1.2", false, "## [0.1.2]\n\n- New.\n")[0].contains("one step"));
        assert!(with("0.2.0", true, NOTES)[0].contains("already exists"));
        assert!(with("0.2.0", false, "")[0].contains("no notes"));
        assert!(
            with("0.2.0", false, "## [0.2.0] - 2026-02-03\n\n## [0.1.0]\n")[0].contains("no notes")
        );
        assert!(
            with("0.2.0", false, "## [0.2.0]\n\n### Added\n\n## [0.1.0]\n")[0].contains("no notes")
        );
        // All three kinds of step are fine.
        assert!(with("0.1.1", false, "## [0.1.1]\n\n- New.\n").is_empty());
        assert!(with("1.0.0", false, "## [1.0.0]\n\n- New.\n").is_empty());
    }

    #[test]
    fn counts_only_new_rows() {
        assert_eq!(net_rows_added("").unwrap(), 0);
        assert_eq!(net_rows_added("3\t0\tresults/sprt.csv").unwrap(), 3);
        // An edited row is one added and one deleted.
        assert_eq!(net_rows_added("1\t1\tresults/sprt.csv").unwrap(), 0);
        assert_eq!(net_rows_added("0\t2\tresults/sprt.csv").unwrap(), 0);
        assert!(net_rows_added("-\t-\tresults/sprt.csv").is_err());
    }

    #[test]
    fn search_and_eval_changes_need_strength_evidence() {
        for path in ["src/search/mod.rs", "src/eval/mod.rs"] {
            let changed = paths(&[path]);
            let base = good_facts(&changed, NOTES);
            assert_eq!(problems(&base).len(), 2, "{path}");
            let sprt_only = Facts {
                sprt_rows_added: 1,
                ..good_facts(&changed, NOTES)
            };
            assert!(problems(&sprt_only)[0].contains("results/ladder.csv"));
            let both = Facts {
                sprt_rows_added: 1,
                ladder_rows_added: 1,
                ..good_facts(&changed, NOTES)
            };
            assert_eq!(problems(&both), Vec::<String>::new());
            let waived = Facts {
                no_strength_test: true,
                ..good_facts(&changed, NOTES)
            };
            assert_eq!(problems(&waived), Vec::<String>::new());
        }
    }
}
