# Security

ritsu-node is a daemon that executes commands and touches files on the machine it runs
on, at the request of a ritsu server. That makes its posture the whole point. The
defaults assume a single operator and a private network boundary between the node and
its server (a mesh VPN or LAN); nothing here is meant to face the open internet.

## Design posture (pre-release)

- **Local policy is authoritative.** The node's own configuration decides which
  workspaces exist, which agents may use them, which executors are enabled, and
  whether elevation is ever possible. A request the policy forbids is refused even
  when the server approved it.
- **No model, no model credentials.** The node never holds an LLM key and never reasons
  about a request; it validates and executes.
- **The node dials out.** The control channel is an outbound, authenticated connection
  to the server. The only listener is the MCP server, bound to loopback by default.
- **Program + arguments over shell strings.** Shell execution is a separate, opt-in
  capability per node.
- **Per-node credential.** Each node enrols once with a short-lived token and holds its
  own credential; revoking a node is one action on the server.

## What runs in CI

- **CI** (`.github/workflows/ci.yml`): `cargo fmt --check`, `cargo clippy -D warnings`,
  `cargo test` on Linux, macOS and Windows, release build.
- **Security** (`.github/workflows/security.yml`): `actionlint`; `gitleaks` over the
  full history on every run; `trufflehog --only-verified` over each push or pull-request
  range and over the full history weekly; `actions/dependency-review` on pull requests;
  `cargo deny check` (licences, sources, advisories, bans) per `deny.toml`; `semgrep
  --config auto --error`.
- **CodeQL** (`.github/workflows/codeql.yml`): the `security-and-quality` suite for Rust
  on pushes to `main`, pull requests and weekly.
- **OpenSSF Scorecard** (`.github/workflows/scorecard.yml`): on pushes to `main`, weekly
  and on branch-rule changes; publishes the score and uploads SARIF to Code Scanning.
- **SBOM** (`.github/workflows/sbom.yml`): CycloneDX and SPDX SBOMs, signed keyless with
  cosign, with SLSA provenance, attached to a release. Runs on dispatch until the release
  workflow lands with the first binary.
- **Dependabot**: monthly grouped cargo and github-actions updates with a 7-day cooldown.

All GitHub Actions are pinned to commit SHAs.

## Reporting

Email `security@horizondigital.dev`. Please do not open a public issue for a
vulnerability. No bounty programme; credit if you want it.
