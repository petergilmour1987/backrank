//! The bench node count: the release binary must give the count committed in
//! `results/bench.toml`, on every operating system.
//!
//! Ignored by default, as it wants a release build. Run it with
//! `cargo test --release --test bench -- --ignored`.

use std::process::Command;

/// The `nodes = N` value in `results/bench.toml`.
fn expected_nodes() -> u64 {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/results/bench.toml");
    let text = std::fs::read_to_string(path).expect("results/bench.toml is readable");
    text.lines()
        .filter_map(|line| line.split_once('='))
        .find(|(key, _)| key.trim() == "nodes")
        .and_then(|(_, value)| value.trim().parse().ok())
        .expect("results/bench.toml has a `nodes = <count>` line")
}

#[test]
#[ignore = "slow in debug builds; run with --release"]
fn bench_matches_the_committed_node_count() {
    let output = Command::new(env!("CARGO_BIN_EXE_backrank"))
        .arg("bench")
        .output()
        .expect("failed to run backrank");
    assert!(
        output.status.success(),
        "bench failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let last = stdout.lines().last().expect("bench printed something");
    let nodes: u64 = last
        .split_whitespace()
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or_else(|| panic!("unexpected last line: {last}"));
    assert_eq!(
        nodes,
        expected_nodes(),
        "bench gives {nodes} nodes, but results/bench.toml says otherwise. If the search was \
         meant to change, put {nodes} in results/bench.toml and the commit message; if not, \
         something changed the search by accident, or the count depends on the platform"
    );
}
