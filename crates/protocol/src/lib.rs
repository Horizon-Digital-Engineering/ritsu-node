#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fmt;
use std::num::{NonZeroU32, NonZeroU64};
use std::ops::RangeInclusive;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use zeroize::Zeroizing;

pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_OUTPUT_FRAME_BYTES: usize = 64 * 1024;
pub const HEARTBEAT_RANGE_S: RangeInclusive<u32> = 1..=300;
pub const WELCOME_TIMEOUT_S: u32 = 10;
pub const MISSED_HEARTBEATS_BEFORE_OFFLINE: u32 = 3;
pub const SERVER_SILENCE_INTERVALS_BEFORE_RECONNECT: u32 = 3;

pub mod capability {
    pub const PROCESS_EXEC: &str = "process.exec";
    pub const SHELL_PREFIX: &str = "shell.";
    pub const FS_READ: &str = "fs.read";
    pub const FS_WRITE: &str = "fs.write";
    pub const FS_LIST: &str = "fs.list";
    pub const FS_STAT: &str = "fs.stat";
    pub const INVENTORY_SNAPSHOT: &str = "inventory.snapshot";
}

fn validate_id(what: &str, value: &str) -> Result<(), String> {
    let charset_ok = value
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    if value.is_empty() || value.len() > 64 || !charset_ok {
        return Err(format!(
            "invalid {what} {value:?}: expected 1 to 64 characters from [A-Za-z0-9_-]"
        ));
    }
    Ok(())
}

macro_rules! id_type {
    ($name:ident, $what:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, String> {
                let value = value.into();
                validate_id($what, &value)?;
                Ok($name(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<String> for $name {
            type Error = String;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                $name::new(value)
            }
        }

        impl From<$name> for String {
            fn from(id: $name) -> String {
                id.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

id_type!(NodeId, "node id");
id_type!(JobId, "job id");
id_type!(WorkspaceId, "workspace id");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "u32", into = "u32")]
pub struct HeartbeatSeconds(u32);

impl HeartbeatSeconds {
    pub fn new(seconds: u32) -> Result<Self, String> {
        if HEARTBEAT_RANGE_S.contains(&seconds) {
            Ok(HeartbeatSeconds(seconds))
        } else {
            Err(format!(
                "heartbeat_s {seconds} outside {}..={}",
                HEARTBEAT_RANGE_S.start(),
                HEARTBEAT_RANGE_S.end()
            ))
        }
    }

    pub fn get(self) -> u32 {
        self.0
    }
}

impl TryFrom<u32> for HeartbeatSeconds {
    type Error = String;

    fn try_from(seconds: u32) -> Result<Self, Self::Error> {
        HeartbeatSeconds::new(seconds)
    }
}

impl From<HeartbeatSeconds> for u32 {
    fn from(value: HeartbeatSeconds) -> u32 {
        value.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "f32", into = "f32")]
pub struct Fraction(f32);

impl Fraction {
    pub fn new(value: f32) -> Option<Self> {
        (value.is_finite() && (0.0..=1.0).contains(&value)).then_some(Fraction(value))
    }

    pub fn get(self) -> f32 {
        self.0
    }
}

impl TryFrom<f32> for Fraction {
    type Error = String;

    fn try_from(value: f32) -> Result<Self, Self::Error> {
        Fraction::new(value).ok_or_else(|| format!("fraction {value} outside 0..=1"))
    }
}

impl From<Fraction> for f32 {
    fn from(value: Fraction) -> f32 {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct OutputChunk(String);

impl OutputChunk {
    pub fn new(text: impl Into<String>) -> Result<Self, String> {
        let text = text.into();
        if text.len() > MAX_OUTPUT_FRAME_BYTES {
            return Err(format!(
                "output frame of {} bytes exceeds {MAX_OUTPUT_FRAME_BYTES}",
                text.len()
            ));
        }
        Ok(OutputChunk(text))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for OutputChunk {
    type Error = String;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        OutputChunk::new(text)
    }
}

impl From<OutputChunk> for String {
    fn from(chunk: OutputChunk) -> String {
        chunk.0
    }
}

// Debug prints a placeholder so a token or credential never lands in a log line by accident.
#[derive(Clone)]
pub struct Secret(Zeroizing<String>);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Secret(Zeroizing::new(value.into()))
    }

    pub fn expose(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

impl Serialize for Secret {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.0.as_str())
    }
}

impl<'de> Deserialize<'de> for Secret {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Secret::new(String::deserialize(deserializer)?))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Os {
    Linux,
    Macos,
    Windows,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Arch {
    X86_64,
    Aarch64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Permission {
    Read,
    Write,
    Exec,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceAdvert {
    pub id: WorkspaceId,
    pub permissions: Vec<Permission>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Offered,
    Accepted,
    Running,
    Succeeded,
    Failed,
    TimedOut,
    Cancelled,
    Refused,
    Lost,
}

impl JobState {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            JobState::Succeeded
                | JobState::Failed
                | JobState::TimedOut
                | JobState::Cancelled
                | JobState::Refused
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Succeeded,
    Failed,
    TimedOut,
    Cancelled,
}

impl From<Outcome> for JobState {
    fn from(outcome: Outcome) -> JobState {
        match outcome {
            Outcome::Succeeded => JobState::Succeeded,
            Outcome::Failed => JobState::Failed,
            Outcome::TimedOut => JobState::TimedOut,
            Outcome::Cancelled => JobState::Cancelled,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResumeEntry {
    pub job_id: JobId,
    pub state: JobState,
    pub first_seq: u64,
    pub last_seq: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Load {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu: Option<Fraction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mem: Option<Fraction>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShellKind {
    Sh,
    Bash,
    Zsh,
    Pwsh,
    Powershell,
    Cmd,
}

impl ShellKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ShellKind::Sh => "sh",
            ShellKind::Bash => "bash",
            ShellKind::Zsh => "zsh",
            ShellKind::Pwsh => "pwsh",
            ShellKind::Powershell => "powershell",
            ShellKind::Cmd => "cmd",
        }
    }

    pub fn capability(self) -> String {
        format!("{}{}", capability::SHELL_PREFIX, self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Stream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Action {
    #[serde(rename = "process.exec")]
    ProcessExec {
        program: String,
        #[serde(default)]
        args: Vec<String>,
    },
    #[serde(rename = "shell.run")]
    ShellRun { shell: ShellKind, script: String },
    #[serde(rename = "fs.read")]
    FsRead { path: String },
    #[serde(rename = "fs.write")]
    FsWrite {
        path: String,
        #[serde(with = "base64_bytes")]
        bytes: Vec<u8>,
    },
    #[serde(rename = "fs.list")]
    FsList { path: String },
    #[serde(rename = "fs.stat")]
    FsStat { path: String },
    #[serde(rename = "inventory.snapshot")]
    InventorySnapshot,
}

impl Action {
    pub fn kind(&self) -> &'static str {
        match self {
            Action::ProcessExec { .. } => capability::PROCESS_EXEC,
            Action::ShellRun { .. } => "shell.run",
            Action::FsRead { .. } => capability::FS_READ,
            Action::FsWrite { .. } => capability::FS_WRITE,
            Action::FsList { .. } => capability::FS_LIST,
            Action::FsStat { .. } => capability::FS_STAT,
            Action::InventorySnapshot => capability::INVENTORY_SNAPSHOT,
        }
    }

    pub fn required_capability(&self) -> String {
        match self {
            Action::ShellRun { shell, .. } => shell.capability(),
            other => other.kind().to_owned(),
        }
    }

    pub fn needs_workspace(&self) -> bool {
        !matches!(self, Action::InventorySnapshot)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryKind {
    File,
    Dir,
    Symlink,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirEntry {
    pub name: String,
    pub kind: EntryKind,
    pub size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified_unix_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum ActionResult {
    #[serde(rename = "fs.read")]
    FsRead {
        #[serde(with = "base64_bytes")]
        bytes: Vec<u8>,
        size: u64,
        truncated: bool,
    },
    #[serde(rename = "fs.list")]
    FsList { entries: Vec<DirEntry> },
    #[serde(rename = "fs.stat")]
    FsStat { entry: DirEntry },
    #[serde(rename = "inventory.snapshot")]
    InventorySnapshot { value: serde_json::Value },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeHello {
    pub protocol: u32,
    pub node_id: NodeId,
    pub hostname: String,
    pub os: Os,
    pub arch: Arch,
    pub version: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub workspaces: Vec<WorkspaceAdvert>,
    #[serde(default)]
    pub resume: Vec<ResumeEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeWelcome {
    pub protocol: u32,
    pub heartbeat_s: HeartbeatSeconds,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeRejected {
    pub reason: String,
    pub retry: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeHeartbeat {
    pub node_id: NodeId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub load: Option<Load>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobOffer {
    pub job_id: JobId,
    pub agent_id: String,
    pub action: Action,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<WorkspaceId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    pub timeout_s: NonZeroU32,
    pub max_output_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_id: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobAccepted {
    pub job_id: JobId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobRefused {
    pub job_id: JobId,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobOutput {
    pub job_id: JobId,
    pub seq: NonZeroU64,
    pub stream: Stream,
    pub data: OutputChunk,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobResult {
    pub job_id: JobId,
    pub seq: NonZeroU64,
    pub result: ActionResult,
}

fn is_zero(n: &u64) -> bool {
    *n == 0
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobFinished {
    pub job_id: JobId,
    pub seq: NonZeroU64,
    pub outcome: Outcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signal: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub truncated_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobCancel {
    pub job_id: JobId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobReplay {
    pub job_id: JobId,
    pub after_seq: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobAck {
    pub job_id: JobId,
}

// Read before the typed body so a job.* message whose body fails to decode can still be refused by id.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Envelope {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub job_id: Option<JobId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Message {
    #[serde(rename = "node.hello")]
    NodeHello(NodeHello),
    #[serde(rename = "node.welcome")]
    NodeWelcome(NodeWelcome),
    #[serde(rename = "node.rejected")]
    NodeRejected(NodeRejected),
    #[serde(rename = "node.heartbeat")]
    NodeHeartbeat(NodeHeartbeat),
    #[serde(rename = "job.offer")]
    JobOffer(JobOffer),
    #[serde(rename = "job.accepted")]
    JobAccepted(JobAccepted),
    #[serde(rename = "job.refused")]
    JobRefused(JobRefused),
    #[serde(rename = "job.output")]
    JobOutput(JobOutput),
    #[serde(rename = "job.result")]
    JobResult(JobResult),
    #[serde(rename = "job.finished")]
    JobFinished(JobFinished),
    #[serde(rename = "job.cancel")]
    JobCancel(JobCancel),
    #[serde(rename = "job.replay")]
    JobReplay(JobReplay),
    #[serde(rename = "job.ack")]
    JobAck(JobAck),
}

impl Message {
    pub fn job_id(&self) -> Option<&JobId> {
        match self {
            Message::NodeHello(_)
            | Message::NodeWelcome(_)
            | Message::NodeRejected(_)
            | Message::NodeHeartbeat(_) => None,
            Message::JobOffer(m) => Some(&m.job_id),
            Message::JobAccepted(m) => Some(&m.job_id),
            Message::JobRefused(m) => Some(&m.job_id),
            Message::JobOutput(m) => Some(&m.job_id),
            Message::JobResult(m) => Some(&m.job_id),
            Message::JobFinished(m) => Some(&m.job_id),
            Message::JobCancel(m) => Some(&m.job_id),
            Message::JobReplay(m) => Some(&m.job_id),
            Message::JobAck(m) => Some(&m.job_id),
        }
    }

    pub fn from_node(&self) -> bool {
        matches!(
            self,
            Message::NodeHello(_)
                | Message::NodeHeartbeat(_)
                | Message::JobAccepted(_)
                | Message::JobRefused(_)
                | Message::JobOutput(_)
                | Message::JobResult(_)
                | Message::JobFinished(_)
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrollRequest {
    pub protocol: u32,
    pub token: Secret,
    pub node_id: NodeId,
    #[serde(with = "base64_bytes")]
    pub public_key: Vec<u8>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    pub hostname: String,
    pub os: Os,
    pub arch: Arch,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrollResponse {
    pub protocol: u32,
    pub node_id: NodeId,
    pub credential: Secret,
    pub control_url: String,
}

mod base64_bytes {
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        let text = String::deserialize(deserializer)?;
        STANDARD.decode(text).map_err(serde::de::Error::custom)
    }
}
