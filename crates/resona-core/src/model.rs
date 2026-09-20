use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use ts_rs::TS;

pub const PARSER_VERSION: &str = "codex-claude-v2";
pub const REDUCER_VERSION: &str = "resona-reducer-v1";
pub const METRIC_VERSION: &str = "resona-v1";
pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("Filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid data: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Invalid(String),
}

#[derive(Clone, Debug, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Settings {
    #[ts(type = "number")]
    pub revision: u64,
    pub launch_at_login: bool,
    pub menu_metric: String,
    pub show_provider: bool,
    pub show_model: bool,
    pub theme: String,
    pub default_range: String,
    pub codex_home: PathBuf,
    pub claude_projects: PathBuf,
    pub codex_enabled: bool,
    pub claude_enabled: bool,
}
impl Settings {
    pub fn for_home(home: &std::path::Path) -> Self {
        Self {
            revision: 0,
            launch_at_login: false,
            menu_metric: "ttft".into(),
            show_provider: true,
            show_model: false,
            theme: "system".into(),
            default_range: "today".into(),
            codex_home: home.join(".codex"),
            claude_projects: home.join(".claude/projects"),
            codex_enabled: true,
            claude_enabled: true,
        }
    }
    pub fn validate(&self) -> Result<()> {
        if !["ttft", "tps", "both"].contains(&self.menu_metric.as_str())
            || !["system", "dark", "light"].contains(&self.theme.as_str())
            || !["today", "24h", "7d"].contains(&self.default_range.as_str())
            || !self.codex_home.is_absolute()
            || !self.claude_projects.is_absolute()
        {
            return Err(Error::Invalid("INVALID_ARGUMENT".into()));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Turn {
    pub provider: String,
    pub turn_key: String,
    pub native_turn_id: String,
    pub owner_thread_id: Option<String>,
    pub model: Option<String>,
    pub status: String,
    pub thread_kind: String,
    pub identity_status: String,
    pub record_source: String,
    #[ts(type = "number | null")]
    pub started_at_ms: Option<i64>,
    #[ts(type = "number | null")]
    pub completed_at_ms: Option<i64>,
    #[ts(type = "number | null")]
    pub first_assistant_at_ms: Option<i64>,
    #[ts(type = "number | null")]
    pub duration_ms: Option<i64>,
    #[ts(type = "number | null")]
    pub ttft_ms: Option<i64>,
    pub ttft_source: String,
    #[ts(type = "number | null")]
    pub output_tokens: Option<i64>,
    pub token_source: String,
    pub tps: Option<f64>,
    pub has_tool: bool,
    pub quality_code: Option<String>,
}
impl Turn {
    pub fn trusted(&self) -> bool {
        self.status == "completed"
            && self.thread_kind == "primary"
            && self.identity_status == "verified"
            && self.record_source == "parsed"
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SourceStatus {
    pub provider: String,
    pub kind: String,
    pub path: String,
    pub enabled: bool,
    pub state: String,
    pub files: usize,
    pub error_files: usize,
    #[ts(type = "number | null")]
    pub last_success_at_ms: Option<i64>,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Bootstrap {
    pub settings: Settings,
    pub storage_directory: String,
    pub sources: Vec<SourceStatus>,
    #[ts(type = "number")]
    pub data_revision: u64,
    pub scanning: bool,
    pub recent: Vec<Turn>,
    pub active: Vec<Turn>,
    pub version: String,
    pub error: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Filters {
    pub range: String,
    #[serde(default)]
    pub providers: Vec<String>,
    #[ts(optional = nullable)]
    #[serde(default)]
    pub model: Option<String>,
    #[ts(optional = nullable)]
    #[serde(default)]
    pub start_date: Option<String>,
    #[ts(optional = nullable)]
    #[serde(default)]
    pub end_date: Option<String>,
}
impl Default for Filters {
    fn default() -> Self {
        Self {
            range: "today".into(),
            providers: vec![],
            model: None,
            start_date: None,
            end_date: None,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub completed_count: usize,
    pub ttft_valid_count: usize,
    pub tps_valid_count: usize,
    pub excluded_count: usize,
    pub ttft_p50: Option<f64>,
    pub ttft_p95: Option<f64>,
    pub tps_p50: Option<f64>,
    pub tps_p5: Option<f64>,
    pub codex_count: usize,
    pub claude_count: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ModelSummary {
    pub provider: String,
    pub model: Option<String>,
    pub summary: Summary,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DistributionBin {
    pub lower: f64,
    pub upper: f64,
    pub codex_count: usize,
    pub claude_count: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TrendBucket {
    #[ts(type = "number")]
    pub start_at_ms: i64,
    #[ts(type = "number")]
    pub end_exclusive_ms: i64,
    pub provider: String,
    pub summary: Summary,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Dashboard {
    #[ts(type = "number")]
    pub data_revision: u64,
    #[ts(type = "number")]
    pub as_of_ms: i64,
    #[ts(type = "number | null")]
    pub start_at_ms: Option<i64>,
    #[ts(type = "number")]
    pub end_exclusive_ms: i64,
    pub summary: Summary,
    pub models: Vec<ModelSummary>,
    pub points: Vec<Turn>,
    pub total_points: usize,
    pub ttft_bins: Vec<DistributionBin>,
    pub tps_bins: Vec<DistributionBin>,
    pub trend_buckets: Vec<TrendBucket>,
    pub sources: Vec<SourceStatus>,
    pub scanning: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListRequest {
    pub filters: Filters,
    pub status: Option<String>,
    pub search: Option<String>,
    pub sort: String,
    pub page_size: usize,
    pub cursor: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TurnPage {
    #[ts(type = "number")]
    pub data_revision: u64,
    pub total: usize,
    pub items: Vec<Turn>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Fact {
    pub kind: String,
    pub at: Option<i64>,
    pub turn: Option<String>,
    pub owner: Option<String>,
    pub model: Option<String>,
    pub ordinal: Option<u64>,
    pub start: u64,
    pub end: u64,
    pub duration: Option<i64>,
    pub ttft: Option<i64>,
    pub output: Option<i64>,
    pub cumulative: Option<i64>,
    pub response: Option<String>,
    pub tool: bool,
    pub failed: bool,
    pub meta: Option<Meta>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Meta {
    pub thread: String,
    pub root: Option<String>,
    pub kind: String,
    pub base: Option<String>,
    pub base_ordinal: Option<u64>,
    pub base_byte: Option<u64>,
    pub fork: Option<String>,
    pub fork_ordinal: Option<u64>,
    pub subagent_start: Option<u64>,
    pub history_mode: String,
    pub version: Option<String>,
}
#[derive(Clone, Debug)]
pub struct Rollout {
    pub provider: String,
    pub id: String,
    pub meta: Meta,
    pub facts: Vec<Fact>,
    pub conflict: bool,
}
pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
