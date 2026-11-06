
## Installing

Download the archive for your system, unpack it, and add the `backrank` binary to your chess
GUI as a UCI engine. x86-64 builds need a CPU with POPCNT and SSE4.2, which nearly every PC
from the last 15 years has.

The binaries aren't code signed yet:

- **macOS:** a browser download is blocked until you run
  `xattr -d com.apple.quarantine backrank`. Downloads made with `curl` aren't affected.
- **Windows:** SmartScreen or your antivirus may warn about an unknown publisher.

To check a download, run `sha256sum -c SHA256SUMS --ignore-missing` (on macOS,
`shasum -a 256 -c SHA256SUMS --ignore-missing`). To check that GitHub Actions built it from
this repository, run `gh attestation verify <file> --repo petergilmour1987/backrank`.
