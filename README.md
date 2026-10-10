# ritsu-node

The per-machine daemon for [ritsu](https://github.com/Horizon-Digital-Engineering/ritsu).
It runs on a machine you own, registers with a ritsu server, and gives ritsu's agents
controlled hands on that machine: run a program, read and write files inside approved
workspaces, report inventory, stream a job's output back, and pick up again after a
reconnect.

ritsu stays the brain. The node holds no model credentials and makes no decisions of its
own. It checks every request against its local policy and refuses what that policy
forbids, whatever the server asked for.

## Status

Pre-release. The wire-protocol crate exists; the daemon does not run yet.

## Layout

| Path | What it is |
|---|---|
| `crates/protocol` | The control-channel message types, versioned (`PROTOCOL_VERSION`). |

## Licence

BUSL-1.1, see [LICENSE](LICENSE). The Change Licence is Apache-2.0.
