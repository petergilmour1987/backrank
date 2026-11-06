# Backrank branching, CI/CD and release plan

## Context

When this plan was written (2026-11-06), Backrank committed directly to `main`. Its only workflow was manual, and it had no tags, no license and `publish = false`. The goal is feature branches, gates on PRs, a semantic version and tag on each merge, and GitHub Releases for macOS, Windows and Linux.

**Decisions:** the repo goes public, under GPL-3.0-or-later. **Scope is GitHub Releases only:** nothing in this work publishes to a package manager, and `publish = false` stays. Section 4 only records the research for later.

**Why public first:** on a private repo, GitHub's branch protection and rulesets APIs return 403 ("Upgrade to GitHub Pro or make this repository public").

## 1. The workflow

This is not technically Gitflow, which adds long-lived `develop` and `release/*` branches, which suit products supporting several versions at once and would only slow a solo project. This project uses **GitHub Flow plus a tag on each release merge**: `main` is always releasable, and all work happens on short-lived branches.

A typical engine change would look like this:

```sh
git switch -c feat/transposition-table
# implement, test, commit the code
caffeinate -i cargo xtask sprt                 # compares against main
caffeinate -i cargo xtask ladder --note "TT"
git add results/ && git commit -m "Record TT results"
cargo xtask bump minor                         # updates version + CHANGELOG, commits
git push -u origin HEAD && gh pr create --fill
gh pr merge --auto --merge                     # merges by itself once the gates pass
```

After the merge, GitHub builds, tags and publishes without further input. Branch prefixes are `feat/`, `fix/`, `docs/`, `ci/` and `chore/`. A hotfix follows the same path with a patch bump.

### Three choices that shape everything else

**The bump happens on the branch, as the last commit of the PR.** This is the main design idea. A version bump made *after* the merge has to be a new commit, so either the tag isn't on the merge commit, or the tagged code still reports the old version. Bumping before the merge means the merge commit already contains the right version, so `git checkout v1.2.3 && cargo build` reports `Backrank 1.2.3`. It also means no bot ever pushes to `main`, so `main` can be locked with no exceptions.

| Alternative | Why not |
|---|---|
| Bot commits the bump after merge | The tag lands on the bot's commit, and the bot needs a stored token that bypasses `main`'s protection |
| Release PR tools (release-plz) | A second PR for every release; mainly useful for batching changes, which isn't needed here |
| Version derived from commit messages (semantic-release) | Requires Conventional Commits, and still has the bot-commit problem |

**Merge commits only; squash and rebase are disabled.** The tag belongs on the merge commit, and there is a project-specific reason too: `results/ladder.csv` and `results/sprt.csv` label each run with a commit hash (`xtask/src/engine.rs:52`). Squashing or rebasing would leave those rows pointing at commits that are no longer on `main`. `git log --first-parent main` still gives a clean one-line-per-PR history.

**Everything runs in one workflow, triggered by the merge.** A tag pushed with the default `GITHUB_TOKEN` does not start other workflows, a classic trap for "tag, then release on tag" setups. A single workflow avoids that without a stored token.

### Version rules

Backrank's public interface is its UCI behaviour and command line. The Rust library is internal and is not covered by these rules, which the README will state.

| Bump | When | Examples |
|---|---|---|
| **Major** | A UCI option or command is removed, renamed or changes meaning. Also `1.0.0` itself | Removing `Move Overhead` |
| **Minor** | A change intended to alter play, or a new feature | Transposition table, new eval term, new UCI option |
| **Patch** | A bug fix, speed-up or packaging fix | Crash fix, time-forfeit fix |
| **None** | Nothing shipped changes | Docs, `results/` rows, xtask, CI, tests |

No labels are needed, because the rule is mechanical: **if the PR changes what ships, it must bump; if the version on `main` has no published release, the workflow releases it.** Docs-only merges therefore produce no tag and no release. `check-release` treats a version as released once its tag exists.

Until `v1.0.0`, major is unused and releases are marked as pre-releases.

## 2. PR gates

Everything reports into a single required check called `gates`, which is much simpler to protect than a list of matrix jobs. The goal is under 10 minutes.

| Check | Runs on | What it does |
|---|---|---|
| Format and lint | Ubuntu | `cargo fmt --all --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings` |
| Tests | Ubuntu, macOS, Windows | `cargo test --locked --workspace` (`-p backrank` on Windows, since the match tooling is macOS/Linux only) |
| Deep perft | Ubuntu | `cargo test --locked --release --test perft -- --ignored` |
| Minimum Rust version | Ubuntu | `cargo +1.88 check --locked -p backrank`, so the `rust-version` in `Cargo.toml` stays true for anyone building from source |
| UCI behaviour | Ubuntu | `fastchess --compliance`, plus a short self-play run (about 20 games) that fails on any crash, illegal move or time loss |
| Version check | Ubuntu | New `cargo xtask check-release`, described below |
| Strength evidence | Ubuntu | A PR touching `src/search/**` or `src/eval/**` must add rows to `results/sprt.csv` and `results/ladder.csv`, or carry a `no-strength-test` label |
| Release dry run | All release builds | Only when the version or the release workflows change: the real release build and smoke test, so release breakage shows up before the merge |

`check-release` enforces:
- If shipped paths changed, the version must be higher than `main`'s, unless `main`'s version has no tag yet (a release that failed before tagging).
- If the version changed, it must be a single valid step up from `main`'s, and its tag must not already exist.
- If the version changed, `CHANGELOG.md` must have a non-empty section for it.

**SPRT and ladder runs stay on the development Mac.** Timed matches on shared 4-core runners are noisy, and one SPRT has already taken 4,489 games. CI checks that the evidence was committed rather than producing it. After Phase 6 adds `backrank bench`, the path rule above can compare node counts instead: unchanged bench, no strength test needed.

## 3. Release pipeline

`release.yml` runs on every push to `main`, one run at a time. It needs only the default `GITHUB_TOKEN`, so there are no stored secrets.

```
merge commit lands on main
 ├─ plan      read the Cargo.toml version; stop if that release is already published
 ├─ build     native build on each OS, then run the binary (see below)
 ├─ tag       annotated v<version> on the merge commit, only after every build passes
 └─ release   draft, upload archives + SHA256SUMS, attest, publish
```

**If the tag already exists:** on this commit, the run carries on, which makes re-running a failed job safe. On a different commit, `plan` warns and stops, because binaries built from this commit must not be attached to a tag on another one.

**Smoke test:** a new `cargo xtask verify-binary <path>` runs the real archive's binary. It drives `uci`, `isready`, `position startpos` and `go depth 8`, then checks that the best move is legal and that `id name` and `--version` both report exactly the release's version.

**Release settings:** turn on GitHub's immutable releases, which lock the tag and assets once published. That is why the flow goes through a draft. Each release also gets a build attestation, a signed record that GitHub Actions built the file from that commit.

**Release notes:** the `CHANGELOG.md` section for that version, including Elo and ladder results for strength changes, since that's what engine users look for.

### Assets

These follow Stockfish's naming, minus Android and RISC-V. Unversioned names give stable links such as `releases/latest/download/backrank-linux-x86-64.tar.gz` once `v1.0.0` exists (GitHub's `latest` ignores pre-releases).

| Asset | Built on | Build notes |
|---|---|---|
| `backrank-macos-universal.tar.gz` | `macos-15` and `macos-15-intel` | Each slice is built and tested natively, then combined with `lipo` |
| `backrank-linux-x86-64.tar.gz` | `ubuntu-24.04` | Static musl, so it runs on any distro |
| `backrank-linux-arm64.tar.gz` | `ubuntu-24.04-arm` | Static musl |
| `backrank-windows-x86-64.zip` | `windows-2025` | Static CRT, so no Visual C++ runtime install is needed |
| `backrank-windows-arm64.zip` | `windows-11-arm` | Static CRT |
| `SHA256SUMS` | | For command-line checks. GitHub also shows each asset's digest, as on Stockfish's page |

Each archive holds the binary, `LICENSE` and `README.md`. GitHub's automatic source archives help meet the GPL's source requirement.

**Measure x86-64 speed before shipping.** Development so far has been on ARM, where popcount is a fast instruction. The default x86-64 target has no `POPCNT`, so `Bitboard::count` (`src/types/bitboard.rs:41`) uses a slower fallback. The plan is to build with `-C target-cpu=x86-64-v2`, which any CPU from roughly the last 15 years supports. Stage 3 compares baseline, v2 and v3 speed, and musl against glibc, and only adds extra builds if the numbers justify them.

### Tooling: a hand-written workflow

| Option | Assessment |
|---|---|
| **Hand-written workflow + small xtask commands** (recommended) | Native build and run on every OS, Stockfish-style asset names, and it fits the existing xtask approach |
| `dist` (cargo-dist) | Actively maintained, and the closest alternative. Worth another look if the workflow becomes a burden, or when package managers come back |
| GoReleaser | It cross-compiles rather than running on each OS, and its docs warn that Cargo workspaces "might not work" |

### Signing: deferred, so the README must cover it

GitHub Releases are now the main way to install, so unsigned-binary warnings will affect real users.

- **macOS:** a browser-downloaded unsigned binary gets blocked by Gatekeeper, and a GUI launching it may fail confusingly. The fix is `xattr -d com.apple.quarantine backrank`, while downloads made with `curl` aren't affected. Signing properly needs an Apple Developer account (about $99 a year).
- **Windows:** browser downloads can trigger SmartScreen or antivirus warnings. SignPath Foundation offers free signing for open source projects, subject to approval.

Stage 3 adds a README install section with these workarounds. Signing is only worth paying for if users complain.

## 4. Package managers (deferred, not part of this work)

Nothing in this section gets built now: no tap, no crates.io publish, no manifests. It records the research for later. As of this check, the name `backrank` is free on crates.io, Homebrew and the AUR, and winget has no `Backrank` publisher.

Stockfish's packaging, from Repology and winget-pkgs, shows the split. Windows and macOS channels are ones a project can publish to itself, while most Linux distro packages (Debian, Fedora and so on) are normally maintained by distro volunteers once a project is popular. So the plan is to target the channels the project controls and let the distros come later.

| Tier | Channel | Install command | How it works | Effort |
|---|---|---|---|---|
| **1: first** | Homebrew tap | `brew install petergilmour1987/tap/backrank` | Your own `homebrew-tap` repo; CI writes a formula pointing at the release assets | Low |
| | crates.io | `cargo install backrank` (`cargo binstall` for prebuilt) | First publish by hand claims the name, then Trusted Publishing with no stored token | Low |
| | winget | `winget install Backrank.Backrank` | Built into Windows. Zip with a portable binary, the format Stockfish and ripgrep both use. First PR by hand, then CI uses Microsoft's `wingetcreate`. Reviewed, so not instant | Medium |
| **2: next** | Scoop | `scoop install backrank/backrank` | Your own bucket repo, one JSON file | Low |
| | AUR | `yay -S backrank-bin` | Arch Linux. Needs an account and SSH key; not pre-reviewed | Low |
| | nixpkgs | `nix-env -iA nixpkgs.backrank` | One PR adding `pkgs/by-name/ba/backrank/package.nix`, as Stockfish does. Builds from source | Low |
| **3: when triggered** | homebrew-core | `brew install backrank` | Needs a stable release and, for self-submission, **225 stars, 90 forks or 90 watchers** (75/30/30 if someone else submits). Builds from source | Low |
| | `.deb` / `.rpm` | Download from Releases | `cargo-deb` assets, added if asked for; the static tarball already covers these users | Low |

**Skip:**
- **Chocolatey:** it has a moderation queue, and winget and Scoop already cover Windows.
- **Snap and Flatpak:** sandboxing gets in the way of a GUI launching an engine by path.
- **Docker, npm, conda:** wrong audience.
- **Your own apt repo:** ongoing upkeep for little gain.

**Beyond package managers:** chess engine users mostly find engines elsewhere, so after `v1.0.0` these probably matter more than Tier 2: submitting to the CCRL and CEGT rating lists, a chessprogramming.org page, and a Lichess bot through lichess-bot. The rating lists are also an independent check on the Stockfish-ladder estimate.

**Still to confirm when this resumes:** whether winget accepts an unsigned zip, as no official statement was found. Publishing would also need credentials: a fine-grained token for your own tap and bucket repos, a classic `public_repo` token for winget, and an SSH key for the AUR, all held in a GitHub environment limited to `main`.

## 5. GitHub settings

**Rule on `main`, with no bypass (including admins):**
- Require a PR, with 0 approvals, since GitHub doesn't allow approving your own PR.
- Require the `gates` check, with the branch up to date, so the version check always sees the latest `main`.
- Block force pushes and deletion.

**Repo settings:** merge commits only, merge commit title and body taken from the PR, auto-merge on, and branches deleted on merge.

**Being public changes the threat model:**
- Keep the first-time contributor approval requirement for fork PRs, and never use `pull_request_target`.
- Default all workflows to read-only permissions, and pin actions to commit hashes.
- Enable Dependabot for actions and xtask's crates, plus secret scanning with push protection.
- Only `release.yml` gets write permission, and only for the jobs that create the tag and release.

## 6. When a release goes wrong

| Failure | Result | Recovery |
|---|---|---|
| Build or smoke test fails after merge | No tag, nothing public | Merge a fix PR with no new bump (`check-release` allows this); it releases the same version on the fix's merge commit |
| Upload fails | The tag and a draft exist, but nothing is published | Re-run the failed job. If that's no longer possible, ship the next patch version, because `plan` won't release from another commit |
| A bad release is published | Immutable, so it can't be replaced | Ship a patch release and note the problem on the old release (notes stay editable) |

## 7. Rollout

Each stage is its own PR or set of PRs, and each must pass its exit check before the next starts.

**Stage 0: go public.**
- Add `LICENSE` (GPL-3.0 text), `license = "GPL-3.0-or-later"` and `repository` in `Cargo.toml` (`publish = false` stays), plus a README license section.
- Scan history for secrets, then make the repo public.
- Turn on secret scanning with push protection, and set workflows to read-only by default.
- Copy this plan to `docs/plan-release.md`, and tag the merged `main` as `v0.1.0` as the baseline. This tag gets no GitHub Release: Stage 2 moves `main` to 0.2.0 before `release.yml` exists, so nothing tries to release 0.1.0.
- *Exit:* the branch protection API no longer returns 403.

**Stage 1: branches and gates.**
- Replace `ci.yml` with the gates, apart from the version and strength checks (Stage 2) and the release dry run (Stage 3).
- Add `cargo xtask setup --skip-stockfish`, so CI can install fastchess and the book for the self-play gate. Add Dependabot.
- Pin `rust-toolchain.toml` to an exact version (currently the floating `stable`), so a new Rust release can't break unrelated PRs or change binaries.
- Fix whatever the first Windows run shows. It has never been built there. No platform-specific code turned up, but the short timing windows in `tests/uci.rs:185` and `:204` may be tight on slow runners.
- Turn on the `main` rule and the repo merge settings, and update `CLAUDE.md`, `docs/plan-v1.md` §8 and the README.
- *Exit:* a deliberately broken PR cannot merge, and a good one can.

**Stage 2: version tooling.**
- Add `cargo xtask bump`, `check-release` and `verify-binary`.
- Add `backrank --version` (it currently returns a usage error, `src/main.rs`).
- Add `CHANGELOG.md` and a PR template with the version rules.
- Remove `version = "0.1.0"` from the backrank dependency in `xtask/Cargo.toml`, or the first minor bump breaks the build.
- Add the version and strength checks to `gates`.
- Bump to 0.2.0 in the same PR, since `--version` changes what ships.
- Change `sprt`'s default `--base` from `HEAD` to the merge-base with `main` (`xtask/src/sprt.rs:18`). On a branch with committed code, `HEAD` would test the engine against itself.
- Add `results/*.csv merge=union` to `.gitattributes`, so branches adding rows don't conflict in local merges.
- Make the version in the README's sample output (`README.md:39`) generic.
- *Exit:* a PR that changes `src/` without a bump fails `gates`.

**Stage 3: GitHub Releases.**
- Add the reusable build workflow and `release.yml`, run the x86-64 speed comparison, and turn on immutable releases.
- Add a README install section: where to download, verifying checksums, and the macOS and Windows warnings.
- Add the release dry run to `gates`, and merge. With `main` already at 0.2.0 and no tag, this merge ships `v0.2.0` through the full flow.
- *Exit:* all the Verification checks below for that release pass.

**Phase 6 then proceeds under this flow:** bench, stress run, GUI check and final ladder climb, ending in `v1.0.0`. Package managers (section 4) can be picked up later.

### Manual steps

- **Windows GUI check:** try a release in a GUI on a Windows machine at Phase 6.

## 8. Files

| Status | Files |
|---|---|
| **New** | `LICENSE`, `CHANGELOG.md`, `.gitattributes`, `.github/workflows/release.yml`, `.github/workflows/build.yml` (reusable), `.github/pull_request_template.md`, `.github/dependabot.yml`, `docs/plan-release.md`, `xtask/src/release.rs` (`bump`, `check-release`, `verify-binary`). No other repos are created |
| **Changed** | `.github/workflows/ci.yml`, `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `xtask/Cargo.toml`, `xtask/src/main.rs`, `xtask/src/sprt.rs`, `xtask/src/setup.rs` (an install without Stockfish, for CI), `src/main.rs`, `README.md`, `CLAUDE.md`, `docs/plan-v1.md` |
| **Reused** | `ENGINE_VERSION` (`src/lib.rs:22`) already feeds `id name`, and `find_legal_move` (`src/movegen/`) can check the smoke test's best move. `xtask/src/setup.rs` already pins and checks the fastchess download |

## 9. Verification

Run these against `v0.2.0` in Stage 3.

1. **Gates block:** a PR with a formatting error can't merge, and neither can one changing `src/` without a bump.
2. **No false releases:** a docs-only PR merges, and `release.yml` stops at `plan` with no tag.
3. **Tag is on the merge commit:** `git rev-parse 'v0.2.0^{commit}'` equals the merge commit, and `git cat-file -t v0.2.0` prints `tag`.
4. **Versions agree:** the binary from each archive reports `0.2.0` through `--version` and UCI `id name`, which `verify-binary` checks in CI.
5. **Downloads are intact:** `shasum -a 256 -c SHA256SUMS` and `gh attestation verify` both pass.
6. **Portability:** the macOS binary contains both architectures (`lipo -info`), the Linux binary is static (`file`) and runs in Alpine and Ubuntu containers, and the Windows zip runs on a clean runner.
7. **Idempotence:** re-running `release.yml` for an already published version stops at `plan` and changes nothing.
8. **Local checks:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` pass for every PR, per `CLAUDE.md`.

## Sources

The Homebrew, crates.io, winget and Repology links support the deferred section 4.

- GitHub: [GITHUB_TOKEN behaviour](https://docs.github.com/en/actions/concepts/security/github_token), [Actions billing](https://docs.github.com/en/billing/concepts/product-billing/github-actions), [hosted runners](https://docs.github.com/en/actions/reference/runners/github-hosted-runners), [protected branches](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-protected-branches/about-protected-branches), [immutable releases](https://docs.github.com/en/code-security/concepts/supply-chain-security/immutable-releases), [artifact attestations](https://docs.github.com/en/actions/concepts/security/artifact-attestations)
- Homebrew: [Package Acceptance Policy](https://docs.brew.sh/Package-Acceptance-Policy), [Acceptable Formulae](https://docs.brew.sh/Acceptable-Formulae)
- crates.io: [Trusted Publishing RFC](https://rust-lang.github.io/rfcs/3691-trusted-publishing-cratesio.html), [Linux Foundation write-up](https://alpha-omega.dev/blog/trusted-publishing-secure-rust-package-deployment-without-secrets/)
- winget: [winget-create](https://github.com/microsoft/winget-create), [winget-releaser](https://github.com/vedantmgoyal2009/winget-releaser), [winget-pkgs](https://github.com/microsoft/winget-pkgs)
- Tooling: [cargo-dist](https://github.com/axodotdev/cargo-dist), [GoReleaser Rust builds](https://goreleaser.com/customization/builds/rust/), [release-plz](https://github.com/release-plz/release-plz), [Rust platform support](https://doc.rust-lang.org/rustc/platform-support.html)
- Packaging data: [Repology: Stockfish](https://repology.org/project/stockfish/versions)
