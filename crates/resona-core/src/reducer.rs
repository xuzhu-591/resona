//! Deterministic reduction of sanitized evidence. File discovery order is irrelevant.
use crate::model::{Fact, Rollout, Turn};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone)]
struct Positioned {
    fact: Fact,
    owner: String,
    kind: String,
    uncertain: bool,
}
#[derive(Default)]
struct Pending {
    turn: Turn,
    tokens: i64,
    seen_counter: bool,
    invalid_counter: bool,
    native_total: Option<i64>,
    invalid_native: bool,
    native_responses: BTreeMap<String, i64>,
    responses: BTreeMap<String, i64>,
}
fn expand(
    id: &str,
    rolls: &BTreeMap<String, Rollout>,
    stack: &mut BTreeSet<String>,
) -> (Vec<Positioned>, bool) {
    if stack.len() >= 128 || !stack.insert(id.into()) {
        return (vec![], true);
    }
    let Some(r) = rolls.get(id) else {
        stack.remove(id);
        return (vec![], true);
    };
    let mut bad = r.conflict;
    let mut events = vec![];
    if let Some(base) = &r.meta.base {
        let (mut prefix, missing) = expand(base, rolls, stack);
        bad |= missing;
        if let (Some(byte), Some(ordinal)) = (r.meta.base_byte, r.meta.base_ordinal) {
            let valid = rolls.get(base).is_some_and(|parent| {
                byte == 0
                    || parent
                        .facts
                        .iter()
                        .any(|f| f.end == byte && f.ordinal.is_none_or(|n| n < ordinal))
            });
            if !valid {
                bad = true;
            } else {
                // Only the directly referenced file is bounded by its physical byte offset.
                let inherited_len = rolls.get(base).map_or(0, |parent| parent.facts.len());
                let split = prefix.len().saturating_sub(inherited_len);
                let mut own = prefix.split_off(split);
                own.retain(|p| p.fact.end <= byte && p.fact.ordinal.is_none_or(|n| n < ordinal));
                prefix.extend(own);
                events = prefix;
            }
        } else {
            bad = true;
        }
    }
    // Old copied forks have no ordinal boundary. Only an exact ordered prefix of
    // sanitized parent evidence can establish the boundary; absence stays unknown.
    let legacy_prefix =
        if r.meta.fork.is_some() && r.meta.fork_ordinal.is_none() && r.meta.base.is_none() {
            let own: Vec<_> = r.facts.iter().filter(|f| f.kind != "meta").collect();
            rolls
                .values()
                .filter(|p| Some(&p.meta.thread) == r.meta.fork.as_ref() && p.id != r.id)
                .map(|p| {
                    own.iter()
                        .zip(p.facts.iter().filter(|f| f.kind != "meta"))
                        .take_while(|(a, b)| same_evidence(a, b))
                        .count()
                })
                .filter(|n| *n > 0)
                .max()
        } else {
            None
        };
    let mut evidence_index = 0;
    for f in &r.facts {
        let copied = f.kind != "meta" && legacy_prefix.is_some_and(|n| evidence_index < n);
        if f.kind != "meta" {
            evidence_index += 1;
        }
        let inherited = copied
            || r.meta
                .fork_ordinal
                .or(r.meta.subagent_start)
                .zip(f.ordinal)
                .is_some_and(|(bound, n)| n < bound);
        let owner = f
            .owner
            .clone()
            .or_else(|| {
                if inherited {
                    r.meta.fork.clone()
                } else {
                    Some(r.meta.thread.clone())
                }
            })
            .unwrap_or_default();
        let uncertain = r.meta.fork.is_some()
            && f.owner.is_none()
            && r.meta.fork_ordinal.is_none()
            && r.meta.subagent_start.is_none()
            && legacy_prefix.is_none();
        let owner_kind = if owner == r.meta.thread {
            r.meta.kind.clone()
        } else {
            rolls
                .values()
                .find(|p| p.meta.thread == owner)
                .map(|p| p.meta.kind.clone())
                .unwrap_or("unknown".into())
        };
        events.push(Positioned {
            fact: f.clone(),
            owner,
            kind: owner_kind,
            uncertain,
        });
    }
    stack.remove(id);
    (events, bad)
}
fn fresh(provider: &str, id: &str, p: &Positioned, model: Option<String>) -> Pending {
    Pending {
        turn: Turn {
            provider: provider.into(),
            turn_key: if provider == "codex" {
                format!("turn:{id}")
            } else {
                id.into()
            },
            native_turn_id: id.into(),
            owner_thread_id: if p.owner.is_empty() {
                None
            } else {
                Some(p.owner.clone())
            },
            model,
            status: "running".into(),
            thread_kind: p.kind.clone(),
            identity_status: if p.uncertain || p.owner.is_empty() {
                "unresolved"
            } else {
                "verified"
            }
            .into(),
            record_source: "parsed".into(),
            started_at_ms: p.fact.at,
            ttft_source: "unknown".into(),
            token_source: "unknown".into(),
            ..Turn::default()
        },
        ..Pending::default()
    }
}
fn finalize(p: &mut Pending) {
    if p.turn.status != "completed" {
        p.turn.ttft_ms = None;
        p.turn.tps = None;
        return;
    }
    if p.turn.duration_ms.is_none() {
        p.turn.duration_ms = p
            .turn
            .completed_at_ms
            .zip(p.turn.started_at_ms)
            .and_then(|(e, s)| e.checked_sub(s))
            .filter(|n| *n >= 0);
    }
    if p.turn.ttft_ms.is_none() {
        p.turn.ttft_ms = p
            .turn
            .first_assistant_at_ms
            .zip(p.turn.started_at_ms)
            .and_then(|(e, s)| e.checked_sub(s))
            .filter(|n| *n >= 0);
        if p.turn.ttft_ms.is_some() {
            p.turn.ttft_source = "assistant_event".into();
        }
    }
    if p.invalid_native {
        p.turn.quality_code = Some("TOKEN_COUNTER_REGRESSION".into());
    } else if let Some(total) = p.native_total {
        p.turn.output_tokens = Some(total);
        p.turn.token_source = "turn_usage".into();
    } else if p.turn.provider == "claude" && !p.responses.is_empty() {
        p.turn.output_tokens = Some(p.responses.values().sum());
        p.turn.token_source = "claude_message".into();
    } else if p.seen_counter && !p.invalid_counter {
        p.turn.output_tokens = Some(p.tokens);
        p.turn.token_source = "counter_delta".into();
    }
    p.turn.tps = p
        .turn
        .output_tokens
        .zip(p.turn.duration_ms)
        .filter(|(n, d)| *n > 0 && *d > 0)
        .map(|(n, d)| n as f64 * 1000.0 / d as f64);
    if p.turn.output_tokens.is_none() {
        p.turn
            .quality_code
            .get_or_insert("TOKEN_EVIDENCE_INCOMPLETE".into());
    }
}
pub fn reduce(rollouts: Vec<Rollout>) -> Vec<Turn> {
    let mut result: BTreeMap<(String, String), Turn> = BTreeMap::new();
    for provider in ["codex", "claude"] {
        let rolls: BTreeMap<_, _> = rollouts
            .iter()
            .filter(|r| r.provider == provider)
            .map(|r| (r.id.clone(), r.clone()))
            .collect();
        for (id, r) in &rolls {
            let (events, bad) = expand(id, &rolls, &mut BTreeSet::new());
            let mut states: BTreeMap<String, Pending> = BTreeMap::new();
            let mut open = BTreeSet::new();
            let mut model = None;
            let mut previous = if bad || (r.meta.fork.is_some() && r.meta.base.is_none()) {
                None
            } else {
                Some(0)
            };
            for p in events {
                let f = &p.fact;
                let turn = f.turn.clone().or_else(|| {
                    if open.len() == 1 {
                        open.first().cloned()
                    } else {
                        None
                    }
                });
                if f.kind == "start" {
                    if let Some(id) = &f.turn {
                        // A new real user message ends an unclosed Claude attempt as incomplete.
                        if provider == "claude" {
                            for old in &open {
                                if let Some(s) = states.get_mut(old) {
                                    s.turn.status = "incomplete".into();
                                }
                            }
                            open.clear();
                        }
                        open.insert(id.clone());
                        states
                            .entry(id.clone())
                            .or_insert_with(|| fresh(provider, id, &p, model.clone()));
                    }
                    continue;
                }
                if f.kind == "model" {
                    model = f.model.clone();
                    if let Some(s) = turn.as_ref().and_then(|id| states.get_mut(id)) {
                        s.turn.model = model.clone();
                    }
                    continue;
                }
                if f.kind == "counter" {
                    if let Some(total) = f.cumulative {
                        if let Some(s) = turn.as_ref().and_then(|id| states.get_mut(id)) {
                            s.seen_counter = true;
                            if let Some(prev) = previous.filter(|n| *n <= total) {
                                s.tokens += total - prev;
                            } else {
                                s.invalid_counter = true;
                            }
                        }
                        previous = Some(total);
                    }
                    continue;
                }
                let Some(id) = turn else {
                    continue;
                };
                if ["finish", "abort", "usage"].contains(&f.kind.as_str())
                    && !states.contains_key(&id)
                {
                    let mut s = fresh(provider, &id, &p, model.clone());
                    s.turn.started_at_ms = None;
                    states.insert(id.clone(), s);
                }
                let Some(s) = states.get_mut(&id) else {
                    continue;
                };
                if f.owner
                    .as_ref()
                    .is_some_and(|owner| s.turn.owner_thread_id.as_ref() != Some(owner))
                {
                    s.turn.identity_status = "conflict".into();
                    s.turn.quality_code = Some("IDENTITY_CONFLICT".into());
                }
                s.turn.has_tool |= f.tool;
                if f.model.is_some() {
                    s.turn.model = f.model.clone();
                }
                match f.kind.as_str() {
                    "assistant" | "claude_assistant" | "claude_finish" => {
                        if s.turn.first_assistant_at_ms.is_none() {
                            s.turn.first_assistant_at_ms = f.at;
                        }
                        if let (Some(response), Some(n)) = (&f.response, f.output) {
                            s.responses
                                .entry(response.clone())
                                .and_modify(|v| *v = (*v).max(n))
                                .or_insert(n);
                        }
                    }
                    "usage" => {
                        if let Some(n) = f.cumulative {
                            if let Some(response) = &f.response {
                                if let Some(old) = s.native_responses.get(response) {
                                    s.invalid_native |= *old != n;
                                    continue;
                                }
                                s.native_responses.insert(response.clone(), n);
                            }
                            s.invalid_native |= s.native_total.is_some_and(|v| n < v);
                            s.native_total = Some(n);
                        }
                    }
                    _ => {}
                }
                if ["finish", "abort", "claude_finish"].contains(&f.kind.as_str()) {
                    s.turn.status = if f.kind == "abort" {
                        "aborted"
                    } else if f.failed {
                        "failed"
                    } else {
                        "completed"
                    }
                    .into();
                    s.turn.completed_at_ms = f.at;
                    s.turn.duration_ms = f.duration;
                    s.turn.ttft_ms = f.ttft;
                    if f.ttft.is_some() {
                        s.turn.ttft_source = "native".into();
                    }
                    open.remove(&id);
                }
            }
            for (_, mut s) in states {
                if bad {
                    s.invalid_counter = true;
                    s.turn.quality_code = Some("LINEAGE_INCOMPLETE".into());
                }
                if r.conflict {
                    s.turn.identity_status = "conflict".into();
                    s.turn.quality_code = Some("IDENTITY_CONFLICT".into());
                }
                finalize(&mut s);
                if s.turn.identity_status == "conflict" {
                    s.turn.ttft_ms = None;
                    s.turn.tps = None;
                }
                let t = s.turn;
                let key = (t.provider.clone(), t.turn_key.clone());
                if let Some(old) = result.get_mut(&key) {
                    let conflict = old.owner_thread_id != t.owner_thread_id
                        && old.identity_status == "verified"
                        && t.identity_status == "verified";
                    let metrics_conflict = old.status == "completed"
                        && t.status == "completed"
                        && (old.ttft_ms.zip(t.ttft_ms).is_some_and(|(a, b)| a != b)
                            || old
                                .output_tokens
                                .zip(t.output_tokens)
                                .is_some_and(|(a, b)| a != b));
                    if conflict
                        || metrics_conflict
                        || old.identity_status == "conflict"
                        || t.identity_status == "conflict"
                    {
                        old.identity_status = "conflict".into();
                        old.quality_code = Some("IDENTITY_CONFLICT".into());
                        old.ttft_ms = None;
                        old.tps = None;
                    } else if (old.status != "completed" && t.status == "completed")
                        || (old.status == t.status
                            && old.output_tokens.is_none()
                            && t.output_tokens.is_some())
                        || (old.identity_status != "verified" && t.identity_status == "verified")
                    {
                        *old = t;
                    }
                } else {
                    result.insert(key, t);
                }
            }
        }
    }
    result.into_values().collect()
}

fn same_evidence(a: &Fact, b: &Fact) -> bool {
    let mut a = a.clone();
    let mut b = b.clone();
    a.start = 0;
    a.end = 0;
    a.ordinal = None;
    b.start = 0;
    b.end = 0;
    b.ordinal = None;
    serde_json::to_value(a).ok() == serde_json::to_value(b).ok()
}
