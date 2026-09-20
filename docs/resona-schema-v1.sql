PRAGMA foreign_keys = ON;
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA busy_timeout = 5000;

BEGIN IMMEDIATE;

CREATE TABLE schema_migrations (
  version INTEGER PRIMARY KEY,
  script_id TEXT NOT NULL,
  applied_at_ms INTEGER NOT NULL CHECK (applied_at_ms >= 0)
);

CREATE TABLE app_meta (
  key TEXT PRIMARY KEY,
  value_json TEXT NOT NULL CHECK (json_valid(value_json))
) WITHOUT ROWID;

CREATE TABLE app_settings (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  settings_version INTEGER NOT NULL CHECK (settings_version > 0),
  revision INTEGER NOT NULL CHECK (revision >= 0),
  launch_at_login INTEGER NOT NULL CHECK (launch_at_login IN (0,1)),
  menu_metric TEXT NOT NULL CHECK (menu_metric IN ('ttft','tps','both')),
  show_provider INTEGER NOT NULL CHECK (show_provider IN (0,1)),
  show_model INTEGER NOT NULL CHECK (show_model IN (0,1)),
  theme TEXT NOT NULL CHECK (theme IN ('system','dark','light')),
  default_range TEXT NOT NULL CHECK (default_range IN ('today','24h','7d')),
  updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0)
);

CREATE TABLE source_roots (
  root_id TEXT PRIMARY KEY,
  provider TEXT NOT NULL CHECK (provider IN ('codex','claude')),
  kind TEXT NOT NULL CHECK (kind IN ('active','archive','projects')),
  path TEXT NOT NULL,
  selected INTEGER NOT NULL CHECK (selected IN (0,1)),
  enabled INTEGER NOT NULL CHECK (enabled IN (0,1) AND enabled <= selected),
  config_revision INTEGER NOT NULL CHECK (config_revision >= 0),
  scan_state TEXT NOT NULL CHECK (scan_state IN
    ('not_started','scanning','ready','missing','paused','error')),
  scan_epoch INTEGER NOT NULL DEFAULT 0 CHECK (scan_epoch >= 0),
  last_scan_at_ms INTEGER,
  last_success_at_ms INTEGER,
  last_error_code TEXT,
  CHECK ((provider = 'codex' AND kind IN ('active','archive'))
      OR (provider = 'claude' AND kind = 'projects')),
  UNIQUE (provider, kind, path)
);

CREATE UNIQUE INDEX idx_roots_selected ON source_roots(provider, kind) WHERE selected = 1;

CREATE TABLE rollouts (
  provider TEXT NOT NULL CHECK (provider IN ('codex','claude')),
  rollout_id TEXT NOT NULL,
  thread_id TEXT,
  root_session_id TEXT,
  identity_status TEXT NOT NULL CHECK (identity_status IN
    ('verified','filename_fallback','unresolved','conflict')),
  thread_kind TEXT NOT NULL CHECK (thread_kind IN ('primary','subagent','unknown')),
  created_at_ms INTEGER,
  cli_version TEXT,
  history_mode TEXT NOT NULL CHECK (history_mode IN ('legacy','paginated','unknown')),
  base_rollout_id TEXT,
  base_end_ordinal INTEGER CHECK (base_end_ordinal IS NULL OR base_end_ordinal >= 0),
  base_end_byte INTEGER CHECK (base_end_byte IS NULL OR base_end_byte >= 0),
  forked_from_thread_id TEXT,
  forked_from_ordinal INTEGER CHECK (forked_from_ordinal IS NULL OR forked_from_ordinal >= 0),
  subagent_start_ordinal INTEGER CHECK
    (subagent_start_ordinal IS NULL OR subagent_start_ordinal >= 0),
  lineage_status TEXT NOT NULL CHECK (lineage_status IN
    ('none','resolved','missing_base','invalid','cycle')),
  parser_version TEXT NOT NULL,
  updated_at_ms INTEGER NOT NULL,
  PRIMARY KEY (provider, rollout_id),
  CHECK ((base_rollout_id IS NULL AND base_end_ordinal IS NULL AND base_end_byte IS NULL)
      OR (base_rollout_id IS NOT NULL AND base_end_ordinal IS NOT NULL AND base_end_byte IS NOT NULL))
) WITHOUT ROWID;

CREATE INDEX idx_rollouts_thread ON rollouts(provider, thread_id);
CREATE INDEX idx_rollouts_base ON rollouts(provider, base_rollout_id);

CREATE TABLE source_files (
  file_id INTEGER PRIMARY KEY,
  root_id TEXT NOT NULL REFERENCES source_roots(root_id),
  provider TEXT NOT NULL,
  rollout_id TEXT NOT NULL,
  physical_path TEXT NOT NULL UNIQUE,
  encoding TEXT NOT NULL CHECK (encoding IN ('jsonl','zstd')),
  device_id TEXT,
  inode_id TEXT,
  generation INTEGER NOT NULL DEFAULT 1 CHECK (generation > 0),
  size_bytes INTEGER NOT NULL DEFAULT 0 CHECK (size_bytes >= 0),
  mtime_ns TEXT,
  decoded_offset INTEGER NOT NULL DEFAULT 0 CHECK (decoded_offset >= 0),
  last_ordinal INTEGER CHECK (last_ordinal IS NULL OR last_ordinal >= 0),
  prefix_fingerprint TEXT,
  checkpoint_fingerprint TEXT,
  scan_epoch INTEGER NOT NULL DEFAULT 0,
  state TEXT NOT NULL CHECK (state IN ('new','reading','ready','missing','error','conflict')),
  last_success_at_ms INTEGER,
  last_error_code TEXT,
  FOREIGN KEY (provider, rollout_id) REFERENCES rollouts(provider, rollout_id)
);

CREATE INDEX idx_source_files_rollout ON source_files(provider, rollout_id);
CREATE INDEX idx_source_files_scan ON source_files(root_id, scan_epoch, state);

CREATE TABLE event_facts (
  fact_id INTEGER PRIMARY KEY,
  provider TEXT NOT NULL,
  rollout_id TEXT NOT NULL,
  event_key TEXT NOT NULL,
  variant_hash TEXT NOT NULL,
  ordinal INTEGER CHECK (ordinal IS NULL OR ordinal >= 0),
  byte_start INTEGER NOT NULL CHECK (byte_start >= 0),
  byte_end INTEGER NOT NULL CHECK (byte_end > byte_start),
  at_ms INTEGER,
  kind TEXT NOT NULL,
  native_thread_id TEXT,
  native_turn_id TEXT,
  native_item_id TEXT,
  normalized_json TEXT NOT NULL CHECK (json_valid(normalized_json)),
  parser_version TEXT NOT NULL,
  FOREIGN KEY (provider, rollout_id) REFERENCES rollouts(provider, rollout_id),
  UNIQUE (provider, rollout_id, event_key, variant_hash, parser_version)
);

CREATE INDEX idx_facts_stream ON event_facts(provider, rollout_id, byte_start);
CREATE INDEX idx_facts_turn ON event_facts(provider, native_turn_id);
CREATE INDEX idx_facts_conflict ON event_facts(provider, rollout_id, event_key, parser_version);

CREATE TABLE file_fact_occurrences (
  file_id INTEGER NOT NULL REFERENCES source_files(file_id),
  generation INTEGER NOT NULL CHECK (generation > 0),
  byte_start INTEGER NOT NULL CHECK (byte_start >= 0),
  fact_id INTEGER NOT NULL REFERENCES event_facts(fact_id),
  PRIMARY KEY (file_id, generation, byte_start)
) WITHOUT ROWID;

CREATE INDEX idx_occurrences_fact ON file_fact_occurrences(fact_id);

CREATE TABLE rollout_cursors (
  provider TEXT NOT NULL,
  rollout_id TEXT NOT NULL,
  reducer_version TEXT NOT NULL,
  checkpoint_fact_id INTEGER REFERENCES event_facts(fact_id),
  state_json TEXT NOT NULL CHECK (json_valid(state_json)),
  dirty INTEGER NOT NULL CHECK (dirty IN (0,1)),
  PRIMARY KEY (provider, rollout_id),
  FOREIGN KEY (provider, rollout_id) REFERENCES rollouts(provider, rollout_id)
) WITHOUT ROWID;

CREATE TABLE turns (
  provider TEXT NOT NULL CHECK (provider IN ('codex','claude')),
  turn_key TEXT NOT NULL,
  native_turn_id TEXT NOT NULL,
  owner_thread_id TEXT,
  root_session_id TEXT,
  thread_kind TEXT NOT NULL CHECK (thread_kind IN ('primary','subagent','unknown')),
  model TEXT,
  status TEXT NOT NULL CHECK (status IN
    ('running','completed','aborted','failed','incomplete')),
  identity_status TEXT NOT NULL CHECK (identity_status IN
    ('verified','legacy_unverified','unresolved','conflict')),
  record_source TEXT NOT NULL CHECK (record_source IN ('parsed','legacy')),
  started_at_ms INTEGER,
  completed_at_ms INTEGER,
  first_assistant_at_ms INTEGER,
  duration_ms INTEGER CHECK (duration_ms IS NULL OR duration_ms >= 0),
  ttft_ms INTEGER CHECK (ttft_ms IS NULL OR ttft_ms >= 0),
  ttft_source TEXT NOT NULL CHECK (ttft_source IN
    ('native','assistant_event','legacy','unknown')),
  output_tokens INTEGER CHECK (output_tokens IS NULL OR output_tokens >= 0),
  token_source TEXT NOT NULL CHECK (token_source IN
    ('turn_usage','response_sum','counter_delta','claude_message','legacy','unknown')),
  tps REAL CHECK (tps IS NULL OR tps >= 0),
  has_tool INTEGER CHECK (has_tool IS NULL OR has_tool IN (0,1)),
  quality_code TEXT,
  parser_version TEXT NOT NULL,
  metric_version TEXT NOT NULL,
  data_revision INTEGER NOT NULL CHECK (data_revision >= 0),
  updated_at_ms INTEGER NOT NULL,
  PRIMARY KEY (provider, turn_key),
  CHECK (status = 'completed' OR (ttft_ms IS NULL AND tps IS NULL)),
  CHECK (tps IS NULL OR (output_tokens IS NOT NULL AND duration_ms IS NOT NULL
      AND output_tokens > 0 AND duration_ms > 0))
) WITHOUT ROWID;

CREATE INDEX idx_turns_recent ON turns(completed_at_ms DESC, provider, turn_key);
CREATE INDEX idx_turns_filter ON turns(provider, model, status, completed_at_ms);
CREATE INDEX idx_turns_thread ON turns(provider, owner_thread_id, completed_at_ms);
CREATE INDEX idx_turns_ttft ON turns(status, ttft_ms DESC, completed_at_ms DESC);
CREATE INDEX idx_turns_tps ON turns(status, tps, completed_at_ms DESC);

CREATE TABLE turn_evidence (
  provider TEXT NOT NULL,
  turn_key TEXT NOT NULL,
  fact_id INTEGER NOT NULL REFERENCES event_facts(fact_id),
  role TEXT NOT NULL CHECK (role IN
    ('start','finish','model','assistant','token','tool','inherited','conflict')),
  PRIMARY KEY (provider, turn_key, fact_id, role),
  FOREIGN KEY (provider, turn_key) REFERENCES turns(provider, turn_key)
) WITHOUT ROWID;

CREATE TABLE legacy_rows (
  import_id TEXT NOT NULL,
  provider TEXT NOT NULL CHECK (provider IN ('codex','claude')),
  legacy_turn_id TEXT NOT NULL,
  legacy_session_key TEXT,
  normalized_json TEXT NOT NULL CHECK (json_valid(normalized_json)),
  mapped_turn_key TEXT,
  state TEXT NOT NULL CHECK (state IN ('pending','mapped','superseded','rejected')),
  reason_code TEXT,
  PRIMARY KEY (import_id, provider, legacy_turn_id)
) WITHOUT ROWID;

INSERT INTO app_settings VALUES
  (1, 1, 0, 0, 'ttft', 1, 0, 'system', 'today', 0);
INSERT INTO app_meta VALUES ('data_revision', '0');
INSERT INTO app_meta VALUES ('parser_version', '"codex-claude-v1"');
INSERT INTO app_meta VALUES ('metric_version', '"resona-v1"');
INSERT INTO app_meta VALUES ('bootstrap_state', '"not_started"');
INSERT INTO schema_migrations VALUES
  (1, 'resona-schema-v1', CAST(strftime('%s','now') AS INTEGER) * 1000);
PRAGMA user_version = 1;

COMMIT;
