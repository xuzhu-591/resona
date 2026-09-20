use crate::model::*;
use crate::Store;
use chrono::{Days, Local, NaiveDate, TimeZone};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub fn quantile(values: &[f64], q: f64) -> Option<f64> {
    let mut v: Vec<_> = values.iter().copied().filter(|n| n.is_finite()).collect();
    if v.is_empty() {
        return None;
    }
    v.sort_by(f64::total_cmp);
    let pos = (v.len() - 1) as f64 * q;
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    Some(v[lo] + (v[hi] - v[lo]) * (pos - lo as f64))
}
pub fn summarize(turns: &[Turn]) -> Summary {
    let good: Vec<_> = turns.iter().filter(|t| t.trusted()).collect();
    let ttft: Vec<_> = good
        .iter()
        .filter_map(|t| t.ttft_ms.map(|n| n as f64 / 1000.0))
        .collect();
    let tps: Vec<_> = good.iter().filter_map(|t| t.tps).collect();
    Summary {
        completed_count: good.len(),
        ttft_valid_count: ttft.len(),
        tps_valid_count: tps.len(),
        excluded_count: turns.len() - good.len(),
        ttft_p50: quantile(&ttft, 0.5),
        ttft_p95: quantile(&ttft, 0.95),
        tps_p50: quantile(&tps, 0.5),
        tps_p5: quantile(&tps, 0.05),
        codex_count: good.iter().filter(|t| t.provider == "codex").count(),
        claude_count: good.iter().filter(|t| t.provider == "claude").count(),
    }
}
fn midnight(d: NaiveDate) -> Result<i64> {
    d.and_hms_opt(0, 0, 0)
        .and_then(|t| Local.from_local_datetime(&t).earliest())
        .map(|t| t.timestamp_millis())
        .ok_or_else(|| Error::Invalid("INVALID_LOCAL_DATE".into()))
}
pub fn bounds(f: &Filters, now: i64) -> Result<(Option<i64>, i64)> {
    let end = now + 1;
    Ok(match f.range.as_str() {
        "today" => (
            Some(midnight(
                Local
                    .timestamp_millis_opt(now)
                    .single()
                    .ok_or_else(|| Error::Invalid("INVALID_TIME".into()))?
                    .date_naive(),
            )?),
            end,
        ),
        "24h" => (Some(now - 86_400_000), end),
        "7d" => (Some(now - 7 * 86_400_000), end),
        "all" => (None, end),
        "custom" => {
            let parse = |v: &Option<String>| {
                v.as_deref()
                    .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
                    .ok_or_else(|| Error::Invalid("INVALID_DATE_RANGE".into()))
            };
            let a = parse(&f.start_date)?;
            let b = parse(&f.end_date)?;
            if a > b {
                return Err(Error::Invalid("INVALID_DATE_RANGE".into()));
            }
            (
                Some(midnight(a)?),
                midnight(
                    b.checked_add_days(Days::new(1))
                        .ok_or_else(|| Error::Invalid("INVALID_DATE_RANGE".into()))?,
                )?
                .min(end),
            )
        }
        _ => return Err(Error::Invalid("INVALID_RANGE".into())),
    })
}
const TRUSTED: &str = "status='completed' AND thread_kind='primary' AND identity_status='verified' AND record_source='parsed' AND parser_version='codex-claude-v2' AND metric_version='resona-v1'";
fn filtered(
    f: &Filters,
    start: Option<i64>,
    end: i64,
) -> Result<(String, Vec<rusqlite::types::Value>)> {
    if f.providers.iter().any(|p| p != "codex" && p != "claude") {
        return Err(Error::Invalid("INVALID_ARGUMENT".into()));
    }
    Ok(("COALESCE(completed_at_ms,started_at_ms)>=?1 AND COALESCE(completed_at_ms,started_at_ms)<?2 AND (?3='[]' OR provider IN (SELECT value FROM json_each(?3))) AND (?4 IS NULL OR model=?4)".into(),vec![start.unwrap_or(0).into(),end.into(),serde_json::to_string(&f.providers)?.into(),f.model.clone().map_or(rusqlite::types::Value::Null,Into::into)]))
}
// Exactly 32 bytes per sample, with NaN representing missing numeric evidence.
// Strings are interned once per model group, never cloned into every sample.
#[derive(Clone, Copy)]
struct Metric {
    at: i64,
    ttft: f64,
    tps: f64,
    group: u32,
    trusted: bool,
    codex: bool,
}
fn metrics_summary<'a>(samples: impl Iterator<Item = &'a Metric>) -> Summary {
    let mut ttft = vec![];
    let mut tps = vec![];
    let (mut completed, mut excluded, mut codex, mut claude) = (0, 0, 0, 0);
    for s in samples {
        if !s.trusted {
            excluded += 1;
            continue;
        }
        completed += 1;
        if s.codex {
            codex += 1;
        } else {
            claude += 1;
        }
        if s.ttft.is_finite() {
            ttft.push(s.ttft);
        }
        if s.tps.is_finite() {
            tps.push(s.tps);
        }
    }
    ttft.sort_unstable_by(f64::total_cmp);
    tps.sort_unstable_by(f64::total_cmp);
    let percentile = |v: &[f64], q: f64| {
        if v.is_empty() {
            None
        } else {
            let rank = (v.len() - 1) as f64 * q;
            let lo = rank.floor() as usize;
            let hi = rank.ceil() as usize;
            Some(v[lo] + (v[hi] - v[lo]) * (rank - lo as f64))
        }
    };
    Summary {
        completed_count: completed,
        excluded_count: excluded,
        ttft_valid_count: ttft.len(),
        tps_valid_count: tps.len(),
        ttft_p50: percentile(&ttft, 0.5),
        ttft_p95: percentile(&ttft, 0.95),
        tps_p50: percentile(&tps, 0.5),
        tps_p5: percentile(&tps, 0.05),
        codex_count: codex,
        claude_count: claude,
    }
}
fn metric_bins(samples: &[Metric], ttft: bool, n: usize) -> Vec<DistributionBin> {
    if n <= 600 {
        return vec![];
    }
    let value = |s: &Metric| if ttft { s.ttft } else { s.tps };
    let maximum = samples
        .iter()
        .filter(|s| s.trusted)
        .map(value)
        .filter(|v| v.is_finite())
        .fold(0.0, f64::max);
    let width = if maximum > 0.0 { maximum / 48.0 } else { 1.0 };
    let mut bins: Vec<_> = (0..48)
        .map(|i| DistributionBin {
            lower: i as f64 * width,
            upper: (i + 1) as f64 * width,
            codex_count: 0,
            claude_count: 0,
        })
        .collect();
    for s in samples.iter().filter(|s| s.trusted && value(s).is_finite()) {
        let b = &mut bins[((value(s) / width).floor() as usize).min(47)];
        if s.codex {
            b.codex_count += 1;
        } else {
            b.claude_count += 1;
        }
    }
    bins
}
impl Store {
    pub fn dashboard(&self, f: &Filters) -> Result<Dashboard> {
        if f.range == "all" {
            return Err(Error::Invalid("INVALID_RANGE".into()));
        }
        let now = now_ms();
        let (start, end) = bounds(f, now)?;
        let (predicate, params) = filtered(f, start, end)?;
        let mut groups: BTreeMap<(String, Option<String>), u32> = BTreeMap::new();
        let mut query=self.conn.prepare(&format!("SELECT provider,model,COALESCE(completed_at_ms,started_at_ms),ttft_ms,tps,({TRUSTED}) FROM turns WHERE {predicate}"))?;
        let mut rows = query.query(rusqlite::params_from_iter(&params))?;
        let mut samples = Vec::new();
        while let Some(row) = rows.next()? {
            let provider: String = row.get(0)?;
            let model: Option<String> = row.get(1)?;
            let codex = provider == "codex";
            let next = groups.len() as u32;
            let group = *groups.entry((provider, model)).or_insert(next);
            samples.push(Metric {
                at: row.get(2)?,
                ttft: row
                    .get::<_, Option<i64>>(3)?
                    .map_or(f64::NAN, |n| n as f64 / 1000.0),
                tps: row.get::<_, Option<f64>>(4)?.unwrap_or(f64::NAN),
                group,
                trusted: row.get(5)?,
                codex,
            });
        }
        let summary = metrics_summary(samples.iter());
        let total_points = summary.completed_count;
        let models = groups
            .iter()
            .map(|((provider, model), id)| ModelSummary {
                provider: provider.clone(),
                model: model.clone(),
                summary: metrics_summary(samples.iter().filter(|s| s.group == *id)),
            })
            .collect();
        let ttft_bins = metric_bins(&samples, true, summary.ttft_valid_count);
        let tps_bins = metric_bins(&samples, false, summary.tps_valid_count);
        let mut trend_buckets = vec![];
        if total_points > 1500 {
            let min = samples
                .iter()
                .filter(|s| s.trusted)
                .map(|s| s.at)
                .min()
                .unwrap_or(0);
            let max = samples
                .iter()
                .filter(|s| s.trusted)
                .map(|s| s.at)
                .max()
                .unwrap_or(min);
            let width = ((max - min + 1) as f64 / 240.0).ceil() as i64;
            let mut buckets: BTreeMap<(i64, bool), Vec<&Metric>> = BTreeMap::new();
            for s in samples.iter().filter(|s| s.trusted) {
                buckets
                    .entry(((s.at - min) / width, s.codex))
                    .or_default()
                    .push(s);
            }
            for ((i, codex), items) in buckets {
                trend_buckets.push(TrendBucket {
                    start_at_ms: min + i * width,
                    end_exclusive_ms: min + (i + 1) * width,
                    provider: if codex { "codex" } else { "claude" }.into(),
                    summary: metrics_summary(items.into_iter()),
                });
            }
        }
        let point_filter = if total_points <= 1500 {
            "1".to_string()
        } else {
            format!(
                "(({} AND ttft_ms IS NOT NULL) OR ({} AND tps IS NOT NULL))",
                summary.ttft_valid_count <= 600,
                summary.tps_valid_count <= 600
            )
        };
        let points = self.query_turns_page(
            &format!("{predicate} AND {TRUSTED} AND {point_filter}"),
            rusqlite::params_from_iter(&params),
            "completed_at_ms,provider,turn_key",
            Some(1500),
            0,
        )?;
        Ok(Dashboard {
            data_revision: self.revision()?,
            as_of_ms: now,
            start_at_ms: start,
            end_exclusive_ms: end,
            summary,
            models,
            points,
            total_points,
            ttft_bins,
            tps_bins,
            trend_buckets,
            sources: self.sources.clone(),
            scanning: self.scanning,
        })
    }
    pub fn list_turns(&self, req: &ListRequest) -> Result<TurnPage> {
        if req.page_size == 0
            || req.page_size > 100
            || !["recent", "ttft", "tps"].contains(&req.sort.as_str())
        {
            return Err(Error::Invalid("INVALID_ARGUMENT".into()));
        }
        let revision = self.revision()?;
        let signature = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&(
                &req.filters,
                &req.status,
                &req.search,
                &req.sort,
                req.page_size
            ))?)
        );
        let cursor: Option<(u64, String, i64, usize)> = req
            .cursor
            .as_ref()
            .map(|s| serde_json::from_str(s).map_err(|_| Error::Invalid("INVALID_CURSOR".into())))
            .transpose()?;
        if let Some((rev, hash, _, _)) = &cursor {
            if *rev != revision {
                return Err(Error::Invalid("STALE_CURSOR".into()));
            }
            if hash != &signature {
                return Err(Error::Invalid("INVALID_CURSOR".into()));
            }
        }
        let as_of = cursor.as_ref().map_or_else(now_ms, |c| c.2);
        let offset = cursor.as_ref().map_or(0, |c| c.3);
        let (start, end) = bounds(&req.filters, as_of)?;
        let (mut predicate, mut params) = filtered(&req.filters, start, end)?;
        predicate.push_str(" AND (?5 IS NULL OR ?5='all' OR status=?5) AND (?6 IS NULL OR instr(native_turn_id,?6)>0 OR instr(COALESCE(owner_thread_id,''),?6)>0)");
        params.push(
            req.status
                .clone()
                .map_or(rusqlite::types::Value::Null, Into::into),
        );
        params.push(
            req.search
                .clone()
                .map_or(rusqlite::types::Value::Null, Into::into),
        );
        let total: i64 = self.conn.query_row(
            &format!("SELECT COUNT(*) FROM turns WHERE {predicate}"),
            rusqlite::params_from_iter(&params),
            |r| r.get(0),
        )?;
        if offset > total as usize {
            return Err(Error::Invalid("INVALID_CURSOR".into()));
        }
        let order = match req.sort.as_str() {
            "ttft" => "ttft_ms DESC,provider,turn_key",
            "tps" => "tps IS NULL,tps,provider,turn_key",
            _ => "COALESCE(completed_at_ms,started_at_ms) DESC,provider,turn_key",
        };
        let items = self.query_turns_page(
            &predicate,
            rusqlite::params_from_iter(&params),
            order,
            Some(req.page_size),
            offset,
        )?;
        let next_cursor = if offset + items.len() < total as usize {
            Some(serde_json::to_string(&(
                revision,
                signature,
                as_of,
                offset + items.len(),
            ))?)
        } else {
            None
        };
        Ok(TurnPage {
            data_revision: revision,
            total: total as usize,
            items,
            next_cursor,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quantiles_are_exact() {
        assert_eq!(quantile(&[1., 2., 3., 4.], 0.5), Some(2.5));
        assert_eq!(quantile(&[], 0.5), None);
    }
    #[test]
    fn range_is_rolling() {
        let f = Filters {
            range: "24h".into(),
            ..Filters::default()
        };
        assert_eq!(
            bounds(&f, 100_000_000).unwrap(),
            (Some(13_600_000), 100_000_001)
        );
    }
}
