use std::num::{NonZeroU32, NonZeroU64};

use ritsu_node_protocol::*;
use serde_json::{Value, json};

fn id(value: &str) -> JobId {
    JobId::new(value).unwrap()
}

fn seq(n: u64) -> NonZeroU64 {
    NonZeroU64::new(n).unwrap()
}

fn chunk(text: &str) -> OutputChunk {
    OutputChunk::new(text).unwrap()
}

fn roundtrip(message: &Message) -> Value {
    let text = serde_json::to_string(message).unwrap();
    let back: Message = serde_json::from_str(&text).unwrap();
    assert_eq!(&back, message);
    serde_json::from_str(&text).unwrap()
}

#[test]
fn hello_matches_the_design_document() {
    let text = r#"{
      "type": "node.hello", "protocol": 1, "node_id": "win-01", "hostname": "win-01",
      "os": "windows", "arch": "x86_64", "version": "0.1.0",
      "capabilities": ["process.exec", "shell.pwsh", "fs.read", "fs.write", "fs.list", "fs.stat", "inventory.snapshot"],
      "workspaces": [{ "id": "projects", "permissions": ["read", "write", "exec"] }],
      "resume": [{ "job_id": "job_01K", "state": "running", "first_seq": 1, "last_seq": 184 }]
    }"#;
    let message: Message = serde_json::from_str(text).unwrap();
    let Message::NodeHello(hello) = &message else {
        panic!("expected node.hello");
    };
    assert_eq!(hello.protocol, PROTOCOL_VERSION);
    assert_eq!(hello.node_id.as_str(), "win-01");
    assert_eq!(hello.os, Os::Windows);
    assert_eq!(hello.arch, Arch::X86_64);
    assert_eq!(
        hello.workspaces[0].permissions,
        vec![Permission::Read, Permission::Write, Permission::Exec]
    );
    assert_eq!(hello.resume[0].state, JobState::Running);
    assert_eq!(hello.resume[0].first_seq, 1);
    assert_eq!(hello.resume[0].last_seq, 184);
    let json = roundtrip(&message);
    assert_eq!(json["type"], "node.hello");
    assert_eq!(json["workspaces"][0]["permissions"][2], "exec");
}

#[test]
fn offer_matches_the_design_document() {
    let text = r#"{
      "type": "job.offer", "job_id": "job_01K", "agent_id": "windows-operator",
      "action": { "kind": "process.exec", "program": "pwsh.exe",
                  "args": ["-NoProfile", "-NonInteractive", "-Command", "Get-PhysicalDisk | ConvertTo-Json"] },
      "workspace": "projects", "cwd": "tools", "env": {}, "timeout_s": 300,
      "max_output_bytes": 1048576, "approval_id": 42
    }"#;
    let message: Message = serde_json::from_str(text).unwrap();
    let Message::JobOffer(offer) = &message else {
        panic!("expected job.offer");
    };
    assert_eq!(offer.workspace.as_ref().unwrap().as_str(), "projects");
    assert_eq!(offer.cwd.as_deref(), Some("tools"));
    assert!(offer.env.is_empty());
    assert_eq!(offer.timeout_s.get(), 300);
    assert_eq!(offer.max_output_bytes, 1_048_576);
    assert_eq!(offer.approval_id, Some(42));
    assert_eq!(offer.action.required_capability(), "process.exec");
    assert!(offer.action.needs_workspace());
    let json = roundtrip(&message);
    assert_eq!(json["action"]["kind"], "process.exec");
    assert!(json.get("env").is_none());
}

#[test]
fn every_message_kind_round_trips() {
    let welcome = Message::NodeWelcome(NodeWelcome {
        protocol: PROTOCOL_VERSION,
        heartbeat_s: HeartbeatSeconds::new(15).unwrap(),
    });
    let rejected = Message::NodeRejected(NodeRejected {
        reason: "credential revoked".into(),
        retry: false,
    });
    let heartbeat = Message::NodeHeartbeat(NodeHeartbeat {
        node_id: NodeId::new("mac-01").unwrap(),
        load: Some(Load {
            cpu: Fraction::new(0.07),
            mem: Fraction::new(0.42),
        }),
    });
    for (message, kind) in [
        (&welcome, "node.welcome"),
        (&rejected, "node.rejected"),
        (&heartbeat, "node.heartbeat"),
    ] {
        assert_eq!(roundtrip(message)["type"], kind);
        assert_eq!(message.job_id(), None);
    }
    assert!(heartbeat.from_node());
    assert!(!welcome.from_node());
    assert!(!rejected.from_node());

    let job = id("job_01K");
    let messages = [
        Message::JobAccepted(JobAccepted {
            job_id: job.clone(),
        }),
        Message::JobRefused(JobRefused {
            job_id: job.clone(),
            reason: "agent not allowed on workspace".into(),
        }),
        Message::JobOutput(JobOutput {
            job_id: job.clone(),
            seq: seq(185),
            stream: Stream::Stderr,
            data: chunk("warning: slow disk"),
        }),
        Message::JobResult(JobResult {
            job_id: job.clone(),
            seq: seq(186),
            result: ActionResult::FsRead {
                bytes: vec![255, 254, 104, 0],
                size: 4,
                truncated: false,
            },
        }),
        Message::JobFinished(JobFinished {
            job_id: job.clone(),
            seq: seq(190),
            outcome: Outcome::Failed,
            exit_code: None,
            signal: Some(9),
            reason: Some("daemon restarted".into()),
            truncated_bytes: 4096,
        }),
        Message::JobCancel(JobCancel {
            job_id: job.clone(),
        }),
        Message::JobReplay(JobReplay {
            job_id: job.clone(),
            after_seq: 184,
        }),
        Message::JobAck(JobAck {
            job_id: job.clone(),
        }),
    ];
    for message in &messages {
        let json = roundtrip(message);
        assert_eq!(message.job_id(), Some(&job));
        assert!(json["type"].as_str().unwrap().starts_with("job."));
    }
    let from_node: Vec<bool> = messages.iter().map(Message::from_node).collect();
    assert_eq!(
        from_node,
        [true, true, true, true, true, false, false, false]
    );
}

#[test]
fn finished_omits_what_it_does_not_know() {
    let text = serde_json::to_string(&Message::JobFinished(JobFinished {
        job_id: id("job_01K"),
        seq: seq(3),
        outcome: Outcome::Succeeded,
        exit_code: Some(0),
        signal: None,
        reason: None,
        truncated_bytes: 0,
    }))
    .unwrap();
    let json: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(json["outcome"], "succeeded");
    assert_eq!(json["exit_code"], 0);
    for absent in ["signal", "reason", "truncated_bytes"] {
        assert!(json.get(absent).is_none(), "{absent} should be omitted");
    }
    assert_eq!(JobState::from(Outcome::TimedOut), JobState::TimedOut);
}

#[test]
fn every_action_kind_round_trips_with_its_capability() {
    let actions = [
        Action::ProcessExec {
            program: "uname".into(),
            args: vec!["-a".into()],
        },
        Action::ShellRun {
            shell: ShellKind::Pwsh,
            script: "Get-Date".into(),
        },
        Action::FsRead {
            path: "notes/todo.md".into(),
        },
        Action::FsWrite {
            path: "out/bin.dat".into(),
            bytes: vec![0, 159, 146, 150, 255],
        },
        Action::FsList { path: ".".into() },
        Action::FsStat {
            path: "Cargo.toml".into(),
        },
        Action::InventorySnapshot,
    ];
    let kinds = [
        "process.exec",
        "shell.run",
        "fs.read",
        "fs.write",
        "fs.list",
        "fs.stat",
        "inventory.snapshot",
    ];
    let capabilities = [
        "process.exec",
        "shell.pwsh",
        "fs.read",
        "fs.write",
        "fs.list",
        "fs.stat",
        "inventory.snapshot",
    ];
    for ((action, kind), capability) in actions.iter().zip(kinds).zip(capabilities) {
        let text = serde_json::to_string(action).unwrap();
        let json: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(json["kind"], kind);
        assert_eq!(action.kind(), kind);
        assert_eq!(action.required_capability(), capability);
        let back: Action = serde_json::from_str(&text).unwrap();
        assert_eq!(&back, action);
    }
    assert!(!Action::InventorySnapshot.needs_workspace());
}

#[test]
fn every_result_kind_round_trips() {
    let entry = DirEntry {
        name: "Cargo.toml".into(),
        kind: EntryKind::File,
        size: 311,
        modified_unix_ms: Some(1_760_000_000_000),
    };
    let results = [
        ActionResult::FsRead {
            bytes: b"hello".to_vec(),
            size: 5,
            truncated: false,
        },
        ActionResult::FsList {
            entries: vec![
                entry.clone(),
                DirEntry {
                    name: "src".into(),
                    kind: EntryKind::Dir,
                    size: 0,
                    modified_unix_ms: None,
                },
            ],
        },
        ActionResult::FsStat {
            entry: entry.clone(),
        },
        ActionResult::InventorySnapshot {
            value: json!({ "cpu": { "model": "example", "cores": 8 }, "disks": [] }),
        },
    ];
    let kinds = ["fs.read", "fs.list", "fs.stat", "inventory.snapshot"];
    for (result, kind) in results.iter().zip(kinds) {
        let text = serde_json::to_string(result).unwrap();
        let json: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(json["kind"], kind);
        let back: ActionResult = serde_json::from_str(&text).unwrap();
        assert_eq!(&back, result);
    }
    let listed = serde_json::to_value(&results[1]).unwrap();
    assert!(listed["entries"][1].get("modified_unix_ms").is_none());
}

#[test]
fn file_bytes_travel_as_base64() {
    let text = serde_json::to_string(&Action::FsWrite {
        path: "x".into(),
        bytes: vec![0, 159, 146, 150, 255],
    })
    .unwrap();
    let json: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(json["bytes"], "AJ+Slv8=");
}

#[test]
fn unknown_fields_are_ignored() {
    let text = json!({
        "type": "job.cancel", "job_id": "job_01K", "added_in_protocol_2": true
    })
    .to_string();
    let message: Message = serde_json::from_str(&text).unwrap();
    assert_eq!(
        message,
        Message::JobCancel(JobCancel {
            job_id: id("job_01K")
        })
    );
}

#[test]
fn unknown_type_is_rejected() {
    let text = json!({ "type": "node.selfdestruct", "node_id": "win-01" }).to_string();
    assert!(serde_json::from_str::<Message>(&text).is_err());
}

#[test]
fn envelope_recovers_the_job_id_when_the_body_cannot_be_decoded() {
    let text = json!({
        "type": "job.offer", "job_id": "job_01K", "agent_id": "a",
        "action": { "kind": "service.list" }, "timeout_s": 30
    })
    .to_string();
    assert!(serde_json::from_str::<Message>(&text).is_err());
    let envelope: Envelope = serde_json::from_str(&text).unwrap();
    assert_eq!(envelope.kind, "job.offer");
    assert_eq!(envelope.job_id, Some(id("job_01K")));

    let hello: Envelope = serde_json::from_str(r#"{"type":"node.hello","protocol":1}"#).unwrap();
    assert_eq!(hello.job_id, None);
}

#[test]
fn ids_are_validated_on_the_wire_and_in_code() {
    assert!(JobId::new("job_01K").is_ok());
    assert!(NodeId::new("mac-01").is_ok());
    assert!(WorkspaceId::new("projects").is_ok());
    for bad in ["", "a/b", "../x", "with space", "x\0", &"a".repeat(65)] {
        assert!(JobId::new(bad).is_err(), "{bad:?} should be rejected");
    }
    let text = json!({ "type": "job.cancel", "job_id": "../etc" }).to_string();
    assert!(serde_json::from_str::<Message>(&text).is_err());
    assert_eq!(String::from(id("job_01K")), "job_01K");
    assert_eq!(id("job_01K").to_string(), "job_01K");
}

#[test]
fn lost_is_not_terminal_and_refused_is() {
    assert!(!JobState::Lost.is_terminal());
    assert!(!JobState::Running.is_terminal());
    assert!(JobState::Refused.is_terminal());
    assert!(JobState::Cancelled.is_terminal());
    assert!(JobState::from(Outcome::Succeeded).is_terminal());
}

#[test]
fn enrollment_round_trips_and_never_prints_secrets() {
    let request = EnrollRequest {
        protocol: PROTOCOL_VERSION,
        token: Secret::new("join_123456"),
        node_id: NodeId::new("win-01").unwrap(),
        public_key: vec![1, 2, 3, 4],
        capabilities: vec!["process.exec".into()],
        hostname: "win-01".into(),
        os: Os::Windows,
        arch: Arch::X86_64,
        version: "0.1.0".into(),
    };
    let text = serde_json::to_string(&request).unwrap();
    let json: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(json["token"], "join_123456");
    assert_eq!(json["public_key"], "AQIDBA==");
    let back: EnrollRequest = serde_json::from_str(&text).unwrap();
    assert_eq!(back.token.expose(), "join_123456");
    assert_eq!(back.node_id, request.node_id);
    assert!(!format!("{request:?}").contains("join_123456"));

    let response = EnrollResponse {
        protocol: PROTOCOL_VERSION,
        node_id: NodeId::new("win-01").unwrap(),
        credential: Secret::new("rnd_abcdef"),
        control_url: "wss://example.invalid/node".into(),
    };
    let text = serde_json::to_string(&response).unwrap();
    let back: EnrollResponse = serde_json::from_str(&text).unwrap();
    assert_eq!(back.credential.expose(), "rnd_abcdef");
    assert!(!format!("{response:?}").contains("rnd_abcdef"));
}

#[test]
fn load_omits_unknown_values_and_shells_cover_windows() {
    let text = serde_json::to_string(&NodeHeartbeat {
        node_id: NodeId::new("mac-01").unwrap(),
        load: Some(Load {
            cpu: None,
            mem: Fraction::new(0.5),
        }),
    })
    .unwrap();
    let json: Value = serde_json::from_str(&text).unwrap();
    assert!(json["load"].get("cpu").is_none());
    assert_eq!(json["load"]["mem"], 0.5);

    for (shell, name) in [
        (ShellKind::Cmd, "cmd"),
        (ShellKind::Powershell, "powershell"),
        (ShellKind::Pwsh, "pwsh"),
        (ShellKind::Bash, "bash"),
    ] {
        let text = serde_json::to_string(&shell).unwrap();
        assert_eq!(text, format!("\"{name}\""));
        assert_eq!(shell.capability(), format!("shell.{name}"));
    }
    assert!(HEARTBEAT_RANGE_S.contains(&15));
    assert!(!HEARTBEAT_RANGE_S.contains(&0));
    assert_eq!(MAX_OUTPUT_FRAME_BYTES, 65_536);
}

#[test]
fn the_wire_rejects_what_the_rules_forbid() {
    let zero_seq = json!({ "type": "job.output", "job_id": "job_01K", "seq": 0, "stream": "stdout", "data": "x" }).to_string();
    assert!(serde_json::from_str::<Message>(&zero_seq).is_err());

    let zero_timeout = json!({
        "type": "job.offer", "job_id": "job_01K", "agent_id": "a",
        "action": { "kind": "inventory.snapshot" }, "timeout_s": 0, "max_output_bytes": 1024
    })
    .to_string();
    assert!(serde_json::from_str::<Message>(&zero_timeout).is_err());
    let envelope: Envelope = serde_json::from_str(&zero_timeout).unwrap();
    assert_eq!(envelope.job_id, Some(id("job_01K")));

    for bad in [0, 301] {
        let text = json!({ "type": "node.welcome", "protocol": 1, "heartbeat_s": bad }).to_string();
        assert!(
            serde_json::from_str::<Message>(&text).is_err(),
            "heartbeat_s {bad} should be rejected"
        );
    }
    assert!(HeartbeatSeconds::new(300).is_ok());

    let over = "x".repeat(MAX_OUTPUT_FRAME_BYTES + 1);
    assert!(OutputChunk::new(over.clone()).is_err());
    assert!(OutputChunk::new("x".repeat(MAX_OUTPUT_FRAME_BYTES)).is_ok());
    let text = json!({ "type": "job.output", "job_id": "job_01K", "seq": 1, "stream": "stdout", "data": over }).to_string();
    assert!(serde_json::from_str::<Message>(&text).is_err());

    let text = json!({ "type": "node.heartbeat", "node_id": "mac-01", "load": { "cpu": 7.5 } })
        .to_string();
    assert!(serde_json::from_str::<Message>(&text).is_err());
    assert!(Fraction::new(f32::NAN).is_none());
    assert!(Fraction::new(-0.1).is_none());
    assert_eq!(Fraction::new(1.0).map(Fraction::get), Some(1.0));
    assert_eq!(NonZeroU32::new(0), None);
    assert_eq!(
        SERVER_SILENCE_INTERVALS_BEFORE_RECONNECT,
        MISSED_HEARTBEATS_BEFORE_OFFLINE
    );
}
