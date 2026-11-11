//! A record of what Refine did on each dictation, for diagnosing how
//! it performs: was the backend reachable, how long did it take, did
//! the answer get used, and if not, why.
//!
//! Stored in the app's SQLite file (table `refine_runs`) so the
//! desktop app's Diagnostics pane and `openwhisper refine history`
//! read the same data, and so it survives a restart.
//!
//! Asks (TASK-97) are rows of the same table, with the source
//! [`SOURCE_ASK`]: they go through the same backend and draw on the
//! same plan, so the plan estimate has to count them. The numbers
//! about Refine itself (how often it fell back, how fast it answers)
//! leave them out; see [`summarize`].
//!
//! **No dictated text is stored**, not even the spoken instruction:
//! only lengths, timings, the backend's settings, the name of the mode
//! when it was one the user made, and the reason for a fallback. The
//! newest [`MAX_ROWS`] runs are kept.
//!
//! Each run also keeps what the call used (tokens, the API list-price
//! cost) and, for the Claude CLI on a subscription, how full the
//! plan's 5-hour and 7-day windows were right after it. Those windows
//! count *all* use of the plan, so the estimates built on them (see
//! [`RefineSummary`]) are upper bounds for what Refine itself takes.

use rusqlite::params;
use serde::Serialize;

use super::{RefineOutcome, RefineStatus};
use crate::store::{Store, StoreError};

/// Rows kept; older ones are deleted on insert.
pub const MAX_ROWS: i64 = 500;

/// A run made by a real dictation.
pub const SOURCE_DICTATION: &str = "dictation";
/// A run made by the connection test.
pub const SOURCE_TEST: &str = "test";
/// An ask (see `crate::ask`), not a Refine run: a question answered
/// by the same backend. Its status is `answered`, `timeout`,
/// `unavailable`, `failed` or `cancelled`.
pub const SOURCE_ASK: &str = "ask";

/// One Refine run.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct RefineRun {
    /// Unix epoch milliseconds.
    pub at_ms: i64,
    /// [`SOURCE_DICTATION`], [`SOURCE_TEST`] or [`SOURCE_ASK`].
    pub source: String,
    /// [`RefineStatus`] id; for an ask, see [`SOURCE_ASK`].
    pub status: String,
    pub backend: String,
    pub model: String,
    /// `""` when the backend sent no effort.
    pub effort: String,
    /// Send-to-answer time. 0 when no call was made.
    pub elapsed_ms: i64,
    /// The backend had been started at record start; this is how many
    /// ms before the request. `None` = it started cold.
    pub warm_lead_ms: Option<i64>,
    pub chars_in: i64,
    pub chars_out: i64,
    /// A spoken instruction was carried out.
    pub instruction: bool,
    /// Why the answer was not used; empty when it was.
    pub detail: String,
    /// All input tokens of the call, cached ones included.
    pub tokens_in: Option<i64>,
    /// The part of `tokens_in` served from the prompt cache.
    pub tokens_cached: Option<i64>,
    pub tokens_out: Option<i64>,
    /// API list price of the call. A yardstick, not a charge.
    pub cost_usd: Option<f64>,
    /// Share of the plan's 5-hour window used after the call (0 to 1).
    pub window_5h_used: Option<f64>,
    /// Unix epoch seconds.
    pub window_5h_resets_at: Option<i64>,
    /// Share of the plan's 7-day window used after the call (0 to 1).
    pub window_7d_used: Option<f64>,
    pub window_7d_resets_at: Option<i64>,
    /// Name of the mode the run was made in, when it was a mode the
    /// user made. `None` for the built-in Dictation mode and for rows
    /// written before modes existed.
    pub mode: Option<String>,
    /// An ask: from sending the question to the first text of the
    /// answer. `None` for Refine runs (they do not stream), for asks
    /// that got no text, and for rows written before asks existed.
    pub first_text_ms: Option<i64>,
    /// An ask: whether a screenshot of the screen was taken for it
    /// (TASK-98). Written for every ask, also one that failed or was
    /// cancelled after the capture, so that a screenshot never leaves
    /// the machine without a record. `None` for Refine runs and for
    /// rows written before asks could include the screen.
    pub screen: Option<bool>,
}

impl RefineRun {
    /// Build the record for an outcome. `None` for
    /// [`RefineStatus::Disabled`]: nothing ran.
    pub fn from_outcome(source: &str, outcome: &RefineOutcome, at_ms: i64) -> Option<Self> {
        if outcome.status == RefineStatus::Disabled {
            return None;
        }
        let usage = outcome.usage.as_ref();
        let plan = usage.and_then(|u| u.plan.as_ref());
        Some(Self {
            at_ms,
            source: source.to_string(),
            status: outcome.status.as_str().to_string(),
            backend: outcome.backend.clone(),
            model: outcome.model.clone(),
            effort: outcome.effort.clone(),
            elapsed_ms: outcome.elapsed_ms as i64,
            warm_lead_ms: outcome.warm_lead_ms.map(|ms| ms as i64),
            chars_in: outcome.raw.trim().chars().count() as i64,
            chars_out: outcome.text.trim().chars().count() as i64,
            instruction: !outcome.instruction.is_empty(),
            detail: outcome.detail.clone(),
            tokens_in: usage.map(|u| u.tokens_in as i64),
            tokens_cached: usage.map(|u| u.tokens_cached as i64),
            tokens_out: usage.map(|u| u.tokens_out as i64),
            cost_usd: usage.and_then(|u| u.cost_usd),
            window_5h_used: plan.and_then(|p| p.five_hour_used),
            window_5h_resets_at: plan.and_then(|p| p.five_hour_resets_at),
            window_7d_used: plan.and_then(|p| p.seven_day_used),
            window_7d_resets_at: plan.and_then(|p| p.seven_day_resets_at),
            mode: (!outcome.mode.is_builtin()).then(|| outcome.mode.name.clone()),
            first_text_ms: None,
            screen: None,
        })
    }

    /// The row is an ask, not a Refine run.
    pub fn is_ask(&self) -> bool {
        self.source == SOURCE_ASK
    }

    /// A backend was asked (so the timing means something).
    pub fn called_backend(&self) -> bool {
        self.status != RefineStatus::SkippedShort.as_str()
    }

    /// The answer was not used and the unrefined text was inserted.
    pub fn fell_back(&self) -> bool {
        matches!(
            self.status.as_str(),
            "timeout" | "unavailable" | "failed" | "rejected"
        )
    }
}

/// Store one run and trim the table. Never panics and never fails the
/// caller: diagnostics must not be able to break a dictation.
pub fn record(store: &Store, run: &RefineRun) {
    let result = store.with_conn(|c| {
        c.execute(
            "INSERT INTO refine_runs \
             (at_ms, source, status, backend, model, effort, elapsed_ms, warm_lead_ms, \
              chars_in, chars_out, instruction, detail, tokens_in, tokens_cached, tokens_out, \
              cost_usd, window_5h_used, window_5h_resets_at, window_7d_used, \
              window_7d_resets_at, mode, first_text_ms, screen) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                run.at_ms,
                run.source,
                run.status,
                run.backend,
                run.model,
                run.effort,
                run.elapsed_ms,
                run.warm_lead_ms,
                run.chars_in,
                run.chars_out,
                run.instruction,
                run.detail,
                run.tokens_in,
                run.tokens_cached,
                run.tokens_out,
                run.cost_usd,
                run.window_5h_used,
                run.window_5h_resets_at,
                run.window_7d_used,
                run.window_7d_resets_at,
                run.mode,
                run.first_text_ms,
                run.screen,
            ],
        )?;
        c.execute(
            "DELETE FROM refine_runs WHERE id NOT IN \
             (SELECT id FROM refine_runs ORDER BY id DESC LIMIT ?)",
            params![MAX_ROWS],
        )?;
        Ok(())
    });
    if let Err(e) = result {
        eprintln!("[refine] history write failed: {e}");
    }
}

/// Record `outcome` in the registered store, if there is one (the
/// desktop app registers it at boot; the CLI does not, so one-off CLI
/// runs leave no trace).
pub fn record_outcome(source: &str, outcome: &RefineOutcome) {
    let Some(store) = crate::stats::store() else {
        return;
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    if let Some(run) = RefineRun::from_outcome(source, outcome, now) {
        record(store, &run);
    }
}

/// Record a ready-made row in the registered store, if there is one.
/// What an ask calls: it has no [`RefineOutcome`].
pub fn record_run(run: &RefineRun) {
    if let Some(store) = crate::stats::store() {
        record(store, run);
    }
}

/// The newest `limit` runs, newest first.
pub fn recent(store: &Store, limit: u32) -> Result<Vec<RefineRun>, StoreError> {
    store.with_conn(|c| {
        let mut stmt = c.prepare(
            "SELECT at_ms, source, status, backend, model, effort, elapsed_ms, warm_lead_ms, \
                    chars_in, chars_out, instruction, detail, tokens_in, tokens_cached, \
                    tokens_out, cost_usd, window_5h_used, window_5h_resets_at, window_7d_used, \
                    window_7d_resets_at, mode, first_text_ms, screen \
             FROM refine_runs ORDER BY id DESC LIMIT ?",
        )?;
        let rows = stmt.query_map(params![limit], |r| {
            Ok(RefineRun {
                at_ms: r.get(0)?,
                source: r.get(1)?,
                status: r.get(2)?,
                backend: r.get(3)?,
                model: r.get(4)?,
                effort: r.get(5)?,
                elapsed_ms: r.get(6)?,
                warm_lead_ms: r.get(7)?,
                chars_in: r.get(8)?,
                chars_out: r.get(9)?,
                instruction: r.get(10)?,
                detail: r.get(11)?,
                tokens_in: r.get(12)?,
                tokens_cached: r.get(13)?,
                tokens_out: r.get(14)?,
                cost_usd: r.get(15)?,
                window_5h_used: r.get(16)?,
                window_5h_resets_at: r.get(17)?,
                window_7d_used: r.get(18)?,
                window_7d_resets_at: r.get(19)?,
                mode: r.get(20)?,
                first_text_ms: r.get(21)?,
                screen: r.get(22)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    })
}

/// Delete every stored run.
pub fn clear(store: &Store) -> Result<(), StoreError> {
    store.with_conn(|c| {
        c.execute("DELETE FROM refine_runs", [])?;
        Ok(())
    })
}

/// Numbers over a set of runs.
///
/// Everything from `refined` to `total_cost_usd` is about Refine and
/// leaves asks out: an ask that timed out is not a dictation that
/// fell back, and an answer of several hundred tokens would skew the
/// tokens "per dictation". Asks are counted on their own, and they
/// are part of `plan`, which is about the plan as a whole.
#[derive(Serialize, Clone, Debug, PartialEq, Default)]
pub struct RefineSummary {
    /// Rows looked at, asks included.
    pub total: i64,
    /// Asks among them.
    pub asks: i64,
    /// Asks that got an answer.
    pub asks_answered: i64,
    /// Over the answered asks: median time to the first text of the
    /// answer, and to its end.
    pub ask_median_first_text_ms: Option<i64>,
    pub ask_median_ms: Option<i64>,
    /// Asks a screenshot was taken for, whether or not they got an
    /// answer.
    pub asks_with_screen: i64,
    /// The model's text was used.
    pub refined: i64,
    /// Too short to send.
    pub skipped: i64,
    /// A backend was asked and its answer was not used.
    pub fallbacks: i64,
    pub timeouts: i64,
    pub unavailable: i64,
    pub failed: i64,
    pub rejected: i64,
    /// A spoken instruction was carried out.
    pub instructions: i64,
    /// Send-to-answer time over the runs that got an answer in time
    /// (refined or rejected). `None` when there are none.
    pub median_ms: Option<i64>,
    pub p90_ms: Option<i64>,
    pub max_ms: Option<i64>,
    /// The same median, split by whether the backend had a head start.
    pub median_warm_ms: Option<i64>,
    pub median_cold_ms: Option<i64>,
    pub warm_runs: i64,
    pub cold_runs: i64,
    /// Newest fallback: when and why. Empty when there is none.
    pub last_problem_at_ms: Option<i64>,
    pub last_problem: String,
    /// Runs that reported token counts, and their averages.
    pub usage_runs: i64,
    pub avg_tokens_in: Option<i64>,
    pub avg_tokens_out: Option<i64>,
    /// API list price: average per run and the sum over these runs.
    /// A yardstick for size; a subscription is not billed this.
    pub avg_cost_usd: Option<f64>,
    pub total_cost_usd: Option<f64>,
    /// The plan's usage windows as of the newest run that reported
    /// them, and what the runs in the current 5-hour window say about
    /// Refine's share. `None` without a subscription backend.
    pub plan: Option<PlanEstimate>,
}

/// Where the subscription's usage windows stand, and rough estimates
/// from the runs recorded in the current 5-hour window.
///
/// The windows count everything on the plan (Claude Code sessions,
/// chat, Refine). So the per-run share is an **upper bound** for what
/// a dictation takes, and the projection is for the plan as a whole.
#[derive(Serialize, Clone, Debug, PartialEq, Default)]
pub struct PlanEstimate {
    /// When the newest reading was taken (epoch ms).
    pub as_of_ms: i64,
    /// Share of the 5-hour window used, 0 to 1.
    pub five_hour_used: f64,
    /// Epoch seconds.
    pub five_hour_resets_at: Option<i64>,
    pub seven_day_used: Option<f64>,
    pub seven_day_resets_at: Option<i64>,
    /// Runs (Refine and asks) recorded in the current 5-hour window.
    pub window_runs: i64,
    /// How far the 5-hour window moved between the first and the
    /// newest of those runs (all plan use in that time).
    pub window_moved: Option<f64>,
    /// `window_moved / window_runs`: the most one dictation can have
    /// taken of the 5-hour window. `None` when the window did not
    /// visibly move (the readings have 1% resolution).
    pub max_share_per_run: Option<f64>,
    /// At the pace between those runs, when the 5-hour window would
    /// be full (epoch ms). `None` when the pace is unknown (readings
    /// under [`MIN_PACE_SPAN_MS`] apart, or no movement) or when the
    /// window resets before that.
    pub full_at_ms: Option<i64>,
    /// A pace could be worked out (so `full_at_ms == None` means
    /// "resets first", not "unknown").
    pub pace_known: bool,
}

/// Readings closer together than this say nothing about pace.
pub const MIN_PACE_SPAN_MS: i64 = 10 * 60 * 1000;

/// Summarize `runs` (as returned by [`recent`], newest first).
pub fn summarize(all_runs: &[RefineRun]) -> RefineSummary {
    // The Refine numbers are over Refine runs only (see
    // [`RefineSummary`]).
    let refine_runs: Vec<RefineRun> = all_runs.iter().filter(|r| !r.is_ask()).cloned().collect();
    let runs = &refine_runs[..];
    let answered_asks: Vec<&RefineRun> = all_runs.iter().filter(|r| r.is_ask() && r.status == "answered").collect();
    let sorted = |mut v: Vec<i64>| {
        v.sort_unstable();
        v
    };
    let ask_first_text = sorted(answered_asks.iter().filter_map(|r| r.first_text_ms).collect());
    let ask_times = sorted(answered_asks.iter().map(|r| r.elapsed_ms).collect());
    let count = |status: &str| runs.iter().filter(|r| r.status == status).count() as i64;
    // Only answers that arrived say something about response time; a
    // timeout would just report the timeout setting.
    let answered: Vec<&RefineRun> = runs
        .iter()
        .filter(|r| r.status == "refined" || r.status == "rejected")
        .collect();
    let times = |pick: &dyn Fn(&RefineRun) -> bool| -> Vec<i64> {
        let mut v: Vec<i64> = answered
            .iter()
            .filter(|r| pick(r))
            .map(|r| r.elapsed_ms)
            .collect();
        v.sort_unstable();
        v
    };
    let all = times(&|_| true);
    let warm = times(&|r| r.warm_lead_ms.is_some());
    let cold = times(&|r| r.warm_lead_ms.is_none());
    let last_problem = runs.iter().find(|r| r.fell_back());
    let with_tokens: Vec<&RefineRun> = runs.iter().filter(|r| r.tokens_in.is_some()).collect();
    let costs: Vec<f64> = runs.iter().filter_map(|r| r.cost_usd).collect();

    RefineSummary {
        total: all_runs.len() as i64,
        asks: all_runs.iter().filter(|r| r.is_ask()).count() as i64,
        asks_answered: answered_asks.len() as i64,
        ask_median_first_text_ms: percentile(&ask_first_text, 50),
        ask_median_ms: percentile(&ask_times, 50),
        asks_with_screen: all_runs.iter().filter(|r| r.is_ask() && r.screen == Some(true)).count() as i64,
        refined: count("refined"),
        skipped: count("skipped-short"),
        fallbacks: runs.iter().filter(|r| r.fell_back()).count() as i64,
        timeouts: count("timeout"),
        unavailable: count("unavailable"),
        failed: count("failed"),
        rejected: count("rejected"),
        instructions: runs.iter().filter(|r| r.instruction).count() as i64,
        median_ms: percentile(&all, 50),
        p90_ms: percentile(&all, 90),
        max_ms: all.last().copied(),
        median_warm_ms: percentile(&warm, 50),
        median_cold_ms: percentile(&cold, 50),
        warm_runs: warm.len() as i64,
        cold_runs: cold.len() as i64,
        last_problem_at_ms: last_problem.map(|r| r.at_ms),
        last_problem: last_problem.map(|r| r.detail.clone()).unwrap_or_default(),
        usage_runs: with_tokens.len() as i64,
        avg_tokens_in: average(with_tokens.iter().filter_map(|r| r.tokens_in)),
        avg_tokens_out: average(with_tokens.iter().filter_map(|r| r.tokens_out)),
        avg_cost_usd: if costs.is_empty() {
            None
        } else {
            Some(costs.iter().sum::<f64>() / costs.len() as f64)
        },
        total_cost_usd: if costs.is_empty() {
            None
        } else {
            Some(costs.iter().sum())
        },
        // The plan is used by asks as much as by dictations.
        plan: plan_estimate(all_runs),
    }
}

fn average(values: impl Iterator<Item = i64>) -> Option<i64> {
    let v: Vec<i64> = values.collect();
    if v.is_empty() {
        None
    } else {
        Some(v.iter().sum::<i64>() / v.len() as i64)
    }
}

/// Plan standing and estimates from `runs` (newest first).
fn plan_estimate(runs: &[RefineRun]) -> Option<PlanEstimate> {
    let newest = runs.iter().find(|r| r.window_5h_used.is_some())?;
    let used = newest.window_5h_used?;
    // Runs in the same 5-hour window as the newest reading.
    let in_window: Vec<&RefineRun> = runs
        .iter()
        .filter(|r| {
            r.window_5h_used.is_some() && r.window_5h_resets_at == newest.window_5h_resets_at
        })
        .collect();
    let oldest = in_window.last().copied().unwrap_or(newest);
    let moved = used - oldest.window_5h_used.unwrap_or(used);
    let span_ms = newest.at_ms - oldest.at_ms;
    let window_runs = in_window.len() as i64;

    let visible = moved > 0.0 && window_runs >= 2;
    let pace_known = visible && span_ms >= MIN_PACE_SPAN_MS;
    let full_at_ms = if pace_known {
        let per_ms = moved / span_ms as f64;
        let at = newest.at_ms + ((1.0 - used).max(0.0) / per_ms) as i64;
        match newest.window_5h_resets_at {
            Some(resets) if at >= resets * 1000 => None,
            _ => Some(at),
        }
    } else {
        None
    };

    Some(PlanEstimate {
        as_of_ms: newest.at_ms,
        five_hour_used: used,
        five_hour_resets_at: newest.window_5h_resets_at,
        seven_day_used: newest.window_7d_used,
        seven_day_resets_at: newest.window_7d_resets_at,
        window_runs,
        window_moved: visible.then_some(moved),
        max_share_per_run: visible.then(|| moved / window_runs as f64),
        full_at_ms,
        pace_known,
    })
}

/// Nearest-rank percentile of a sorted slice.
fn percentile(sorted: &[i64], pct: usize) -> Option<i64> {
    if sorted.is_empty() {
        return None;
    }
    let rank = (pct * sorted.len()).div_ceil(100).max(1);
    sorted.get(rank - 1).copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn fresh_store() -> (tempfile::TempDir, Store) {
        let dir = tempdir().unwrap();
        let store = Store::open_or_init(&dir.path().join("test.db")).unwrap();
        (dir, store)
    }

    fn run(status: &str, elapsed_ms: i64, warm: Option<i64>) -> RefineRun {
        RefineRun {
            at_ms: 1_000,
            source: SOURCE_DICTATION.into(),
            status: status.into(),
            backend: "claude-cli".into(),
            model: "haiku".into(),
            effort: "medium".into(),
            elapsed_ms,
            warm_lead_ms: warm,
            chars_in: 80,
            chars_out: 82,
            instruction: false,
            detail: if status == "refined" { String::new() } else { format!("{status} reason") },
            tokens_in: None,
            tokens_cached: None,
            tokens_out: None,
            cost_usd: None,
            window_5h_used: None,
            window_5h_resets_at: None,
            window_7d_used: None,
            window_7d_resets_at: None,
            mode: None,
            first_text_ms: None,
            screen: None,
        }
    }

    /// An ask that was answered: first text after `first_ms`, done
    /// after `elapsed_ms`.
    fn ask(status: &str, first_ms: Option<i64>, elapsed_ms: i64) -> RefineRun {
        RefineRun {
            source: SOURCE_ASK.into(),
            model: "sonnet".into(),
            effort: "low".into(),
            chars_in: 30,
            chars_out: 400,
            first_text_ms: first_ms,
            screen: Some(false),
            detail: if status == "answered" { String::new() } else { format!("{status} reason") },
            ..run(status, elapsed_ms, Some(2000))
        }
    }

    /// A run with usage and a 5-hour reading, `minutes` after t0.
    fn metered(minutes: i64, used: f64, resets_at: i64) -> RefineRun {
        RefineRun {
            at_ms: minutes * 60_000,
            tokens_in: Some(600),
            tokens_cached: Some(560),
            tokens_out: Some(40),
            cost_usd: Some(0.0005),
            window_5h_used: Some(used),
            window_5h_resets_at: Some(resets_at),
            window_7d_used: Some(0.21),
            window_7d_resets_at: Some(900_000),
            ..run("refined", 1400, Some(3000))
        }
    }

    #[test]
    fn round_trips_newest_first() {
        let (_dir, store) = fresh_store();
        let mut first = run("refined", 800, Some(2500));
        first.at_ms = 1;
        let mut second = run("timeout", 5000, None);
        second.at_ms = 2;
        second.instruction = true;
        record(&store, &first);
        record(&store, &second);
        let got = recent(&store, 10).unwrap();
        assert_eq!(got, vec![second, first]);
        assert_eq!(recent(&store, 1).unwrap().len(), 1);
    }

    #[test]
    fn keeps_only_the_newest_rows() {
        let (_dir, store) = fresh_store();
        for i in 0..(MAX_ROWS + 20) {
            let mut r = run("refined", 700, None);
            r.at_ms = i;
            record(&store, &r);
        }
        let got = recent(&store, 10_000).unwrap();
        assert_eq!(got.len() as i64, MAX_ROWS);
        assert_eq!(got[0].at_ms, MAX_ROWS + 19);
        assert_eq!(got.last().unwrap().at_ms, 20);
    }

    #[test]
    fn clear_empties_the_table() {
        let (_dir, store) = fresh_store();
        record(&store, &run("refined", 700, None));
        clear(&store).unwrap();
        assert!(recent(&store, 10).unwrap().is_empty());
    }

    #[test]
    fn outcome_becomes_a_run_without_any_text() {
        let outcome = RefineOutcome {
            text: "Hi Emanoil, we're moving the meeting. ".into(),
            raw: "hej emanoil vi rykker mødet skriv det på engelsk ".into(),
            status: RefineStatus::Refined,
            detail: String::new(),
            instruction: "skriv det på engelsk".into(),
            backend: "claude-cli".into(),
            model: "sonnet".into(),
            effort: "low".into(),
            elapsed_ms: 912,
            warm_lead_ms: Some(3100),
            usage: Some(crate::refine::CallUsage {
                tokens_in: 640,
                tokens_cached: 571,
                tokens_out: 22,
                cost_usd: Some(0.00028),
                plan: Some(crate::refine::PlanWindows {
                    five_hour_used: Some(0.09),
                    five_hour_resets_at: Some(1794325200),
                    seven_day_used: Some(0.21),
                    seven_day_resets_at: Some(1794646800),
                }),
            }),
            mode: crate::refine::ModeUse::default(),
        };
        let r = RefineRun::from_outcome(SOURCE_DICTATION, &outcome, 42).unwrap();
        assert_eq!(r.status, "refined");
        assert_eq!(r.model, "sonnet");
        assert_eq!(r.effort, "low");
        assert_eq!(r.elapsed_ms, 912);
        assert_eq!(r.warm_lead_ms, Some(3100));
        assert!(r.instruction);
        assert_eq!(r.chars_in, 48);
        assert_eq!((r.tokens_in, r.tokens_cached, r.tokens_out), (Some(640), Some(571), Some(22)));
        assert_eq!(r.cost_usd, Some(0.00028));
        assert_eq!(r.window_5h_used, Some(0.09));
        assert_eq!(r.window_7d_resets_at, Some(1794646800));
        // Nothing of what was said is in the record.
        let json = serde_json::to_string(&r).unwrap();
        assert!(!json.contains("emanoil") && !json.contains("Emanoil"), "{json}");
        assert!(!json.contains("engelsk"), "{json}");

        // The built-in mode leaves the mode column empty.
        assert_eq!(r.mode, None);

        assert!(RefineRun::from_outcome(SOURCE_DICTATION, &RefineOutcome::disabled("x"), 1).is_none());
    }

    #[test]
    fn a_custom_mode_is_recorded_by_name_and_round_trips() {
        let (_dir, store) = fresh_store();
        let mut outcome = RefineOutcome::disabled("hej anna vi ses i morgen");
        outcome.status = RefineStatus::Refined;
        outcome.text = "Hi Anna, see you tomorrow. ".into();
        outcome.mode = crate::refine::ModeUse {
            id: "write-in-english".into(),
            name: "Write in English".into(),
            reason: "app-default".into(),
            app_name: "Mail".into(),
        };
        let with_mode = RefineRun::from_outcome(SOURCE_DICTATION, &outcome, 7).unwrap();
        assert_eq!(with_mode.mode.as_deref(), Some("Write in English"));

        record(&store, &run("refined", 700, None));
        record(&store, &with_mode);
        let got = recent(&store, 10).unwrap();
        assert_eq!(got[0], with_mode);
        assert_eq!(got[1].mode, None);
    }

    #[test]
    fn rows_written_before_the_mode_column_read_as_no_mode() {
        // The table as migrations 3 and 4 left it, with one row, then
        // the mode column added the way migration 5 adds it.
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE refine_runs (
                id INTEGER PRIMARY KEY AUTOINCREMENT, at_ms INTEGER NOT NULL, source TEXT NOT NULL,
                status TEXT NOT NULL, backend TEXT NOT NULL, model TEXT NOT NULL, effort TEXT NOT NULL,
                elapsed_ms INTEGER NOT NULL, warm_lead_ms INTEGER, chars_in INTEGER NOT NULL,
                chars_out INTEGER NOT NULL, instruction INTEGER NOT NULL, detail TEXT NOT NULL,
                tokens_in INTEGER, tokens_cached INTEGER, tokens_out INTEGER, cost_usd REAL,
                window_5h_used REAL, window_5h_resets_at INTEGER, window_7d_used REAL,
                window_7d_resets_at INTEGER);
             INSERT INTO refine_runs (at_ms, source, status, backend, model, effort, elapsed_ms,
                chars_in, chars_out, instruction, detail)
             VALUES (5, 'dictation', 'refined', 'claude-cli', 'sonnet', 'low', 900, 40, 42, 0, '');
             ALTER TABLE refine_runs ADD COLUMN mode TEXT;
             ALTER TABLE refine_runs ADD COLUMN first_text_ms INTEGER;
             ALTER TABLE refine_runs ADD COLUMN screen INTEGER;",
        )
        .unwrap();
        let store = Store::from_connection_for_test(conn);
        let got = recent(&store, 10).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].mode, None);
        assert_eq!(got[0].elapsed_ms, 900);
        // The same goes for the columns asks brought (migrations 6
        // and 7).
        assert_eq!(got[0].first_text_ms, None);
        assert_eq!(got[0].screen, None);
    }

    #[test]
    fn an_ask_records_whether_a_screenshot_was_taken_and_old_rows_read_as_unknown() {
        // The table as migration 6 left it, with an ask in it, then
        // the screen column added the way migration 7 adds it.
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE refine_runs (
                id INTEGER PRIMARY KEY AUTOINCREMENT, at_ms INTEGER NOT NULL, source TEXT NOT NULL,
                status TEXT NOT NULL, backend TEXT NOT NULL, model TEXT NOT NULL, effort TEXT NOT NULL,
                elapsed_ms INTEGER NOT NULL, warm_lead_ms INTEGER, chars_in INTEGER NOT NULL,
                chars_out INTEGER NOT NULL, instruction INTEGER NOT NULL, detail TEXT NOT NULL,
                tokens_in INTEGER, tokens_cached INTEGER, tokens_out INTEGER, cost_usd REAL,
                window_5h_used REAL, window_5h_resets_at INTEGER, window_7d_used REAL,
                window_7d_resets_at INTEGER, mode TEXT, first_text_ms INTEGER);
             INSERT INTO refine_runs (at_ms, source, status, backend, model, effort, elapsed_ms,
                chars_in, chars_out, instruction, detail, first_text_ms)
             VALUES (5, 'ask', 'answered', 'claude-cli', 'sonnet', 'low', 2400, 30, 400, 0, '', 620);
             ALTER TABLE refine_runs ADD COLUMN screen INTEGER;",
        )
        .unwrap();
        let store = Store::from_connection_for_test(conn);
        // An ask from before the column: not known.
        assert_eq!(recent(&store, 10).unwrap()[0].screen, None);

        // From here on every ask says it, whatever became of it.
        let failed_with_screen = RefineRun { screen: Some(true), ..ask("failed", None, 900) };
        let cancelled_with_screen = RefineRun { screen: Some(true), ..ask("cancelled", None, 300) };
        let answered_without = ask("answered", Some(500), 2000);
        record(&store, &failed_with_screen);
        record(&store, &cancelled_with_screen);
        record(&store, &answered_without);
        record(&store, &run("refined", 700, None));
        let got = recent(&store, 10).unwrap();
        assert_eq!(got[0].screen, None, "a Refine run has no screen");
        assert_eq!(got[1], answered_without);
        assert_eq!(got[2], cancelled_with_screen);
        assert_eq!(got[3], failed_with_screen);
        assert_eq!(got[1].screen, Some(false));
        // Counted over every ask, answered or not.
        let s = summarize(&got);
        assert_eq!((s.asks, s.asks_answered, s.asks_with_screen), (4, 2, 2));
        // And the row says it as a word a program can read.
        let json = serde_json::to_value(&got[3]).unwrap();
        assert_eq!(json["screen"], true);
        assert_eq!(serde_json::to_value(&got[0]).unwrap()["screen"], serde_json::Value::Null);
    }

    #[test]
    fn an_ask_round_trips_with_its_time_to_first_text() {
        let (_dir, store) = fresh_store();
        let mut answered = ask("answered", Some(620), 2400);
        answered.tokens_in = Some(900);
        answered.tokens_out = Some(120);
        record(&store, &run("refined", 700, None));
        record(&store, &answered);
        let got = recent(&store, 10).unwrap();
        assert_eq!(got[0], answered);
        assert!(got[0].is_ask());
        assert_eq!(got[0].first_text_ms, Some(620));
        // A Refine run in the same table has none.
        assert_eq!(got[1].first_text_ms, None);
        assert!(!got[1].is_ask());
    }

    #[test]
    fn asks_are_counted_on_their_own_and_stay_out_of_the_refine_numbers() {
        // Newest first.
        let runs = vec![
            ask("timeout", None, 60_000),
            ask("answered", Some(700), 3000),
            ask("answered", Some(500), 2000),
            ask("cancelled", None, 900),
            run("refined", 800, Some(2500)),
            run("timeout", 5000, None),
        ];
        let s = summarize(&runs);
        assert_eq!(s.total, 6);
        assert_eq!((s.asks, s.asks_answered), (4, 2));
        assert_eq!(s.ask_median_first_text_ms, Some(500));
        assert_eq!(s.ask_median_ms, Some(2000));
        // Refine: one refined, one timeout. The ask that timed out is
        // not a dictation that fell back, and its minute is not a
        // Refine response time.
        assert_eq!((s.refined, s.fallbacks, s.timeouts), (1, 1, 1));
        assert_eq!((s.median_ms, s.max_ms), (Some(800), Some(800)));
        assert_eq!(s.last_problem, "timeout reason");
        assert_eq!(s.last_problem_at_ms, Some(1_000));

        // No asks: the ask numbers are empty, the rest as before.
        let s = summarize(&[run("refined", 800, None)]);
        assert_eq!((s.asks, s.asks_answered), (0, 0));
        assert_eq!((s.ask_median_first_text_ms, s.ask_median_ms), (None, None));
    }

    #[test]
    fn the_plan_estimate_counts_asks_too() {
        let mut asked = metered(60, 0.30, 18_000);
        asked.source = SOURCE_ASK.into();
        asked.status = "answered".into();
        let plan = summarize(&[asked, metered(0, 0.10, 18_000)]).plan.unwrap();
        assert_eq!(plan.window_runs, 2);
        assert_eq!(plan.five_hour_used, 0.30);
        assert!((plan.window_moved.unwrap() - 0.20).abs() < 1e-9);
    }

    #[test]
    fn summary_counts_and_times() {
        // Newest first, as `recent` returns them.
        let runs = vec![
            run("timeout", 5000, Some(900)),
            run("refined", 800, Some(2500)),
            run("refined", 900, Some(3000)),
            run("refined", 1300, None),
            run("rejected", 1000, Some(2000)),
            run("skipped-short", 0, None),
            run("unavailable", 3, None),
            run("failed", 40, None),
        ];
        let s = summarize(&runs);
        assert_eq!(s.total, 8);
        assert_eq!(s.refined, 3);
        assert_eq!(s.skipped, 1);
        assert_eq!(s.fallbacks, 4);
        assert_eq!((s.timeouts, s.unavailable, s.failed, s.rejected), (1, 1, 1, 1));
        // Times over answered runs only: 800, 900, 1000, 1300.
        assert_eq!(s.median_ms, Some(900));
        assert_eq!(s.p90_ms, Some(1300));
        assert_eq!(s.max_ms, Some(1300));
        assert_eq!(s.median_warm_ms, Some(900));
        assert_eq!(s.median_cold_ms, Some(1300));
        assert_eq!((s.warm_runs, s.cold_runs), (3, 1));
        assert_eq!(s.last_problem, "timeout reason");
    }

    #[test]
    fn usage_round_trips_and_old_rows_have_none() {
        let (_dir, store) = fresh_store();
        record(&store, &run("refined", 700, None));
        let with = metered(5, 0.12, 18_000);
        record(&store, &with);
        let got = recent(&store, 10).unwrap();
        assert_eq!(got[0], with);
        assert_eq!(got[1].tokens_in, None);
        assert_eq!(got[1].window_5h_used, None);
    }

    #[test]
    fn summary_averages_tokens_and_cost_over_runs_that_report_them() {
        let mut big = metered(1, 0.10, 18_000);
        big.tokens_in = Some(1000);
        big.tokens_out = Some(80);
        big.cost_usd = Some(0.0025);
        let runs = vec![metered(2, 0.10, 18_000), big, run("refined", 900, None)];
        let s = summarize(&runs);
        assert_eq!(s.usage_runs, 2);
        assert_eq!(s.avg_tokens_in, Some(800));
        assert_eq!(s.avg_tokens_out, Some(60));
        assert!((s.avg_cost_usd.unwrap() - 0.0015).abs() < 1e-9);
        assert!((s.total_cost_usd.unwrap() - 0.003).abs() < 1e-9);
    }

    #[test]
    fn plan_estimate_gives_an_upper_bound_per_run_and_a_projection() {
        // Window resets at minute 300 (18 000 s). Over 60 minutes and
        // 4 Refine runs it went from 10% to 30%.
        let runs = vec![
            metered(60, 0.30, 18_000),
            metered(40, 0.22, 18_000),
            metered(20, 0.15, 18_000),
            metered(0, 0.10, 18_000),
        ];
        let plan = summarize(&runs).plan.unwrap();
        assert_eq!(plan.five_hour_used, 0.30);
        assert_eq!(plan.window_runs, 4);
        assert!((plan.window_moved.unwrap() - 0.20).abs() < 1e-9);
        // At most 5% of the window per dictation (it includes all
        // other use of the plan in that hour).
        assert!((plan.max_share_per_run.unwrap() - 0.05).abs() < 1e-9);
        // 20 points an hour, 70 to go: full 210 minutes after the
        // newest reading, at minute 270, before the reset at 300.
        assert!(plan.pace_known);
        assert_eq!(plan.full_at_ms, Some(270 * 60_000));
    }

    #[test]
    fn plan_estimate_says_when_the_window_resets_first() {
        // Same pace, but the window resets at minute 120.
        let runs = vec![metered(60, 0.30, 7_200), metered(0, 0.10, 7_200)];
        let plan = summarize(&runs).plan.unwrap();
        assert!(plan.pace_known);
        assert_eq!(plan.full_at_ms, None);
    }

    #[test]
    fn plan_estimate_makes_no_claims_it_cannot_back() {
        // Readings only 2 minutes apart: no pace.
        let plan = summarize(&[metered(2, 0.11, 18_000), metered(0, 0.10, 18_000)])
            .plan
            .unwrap();
        assert!(!plan.pace_known);
        assert_eq!(plan.full_at_ms, None);
        assert!(plan.max_share_per_run.is_some());

        // No visible movement at 1% resolution: no share, no pace.
        let plan = summarize(&[metered(30, 0.09, 18_000), metered(0, 0.09, 18_000)])
            .plan
            .unwrap();
        assert_eq!(plan.window_moved, None);
        assert_eq!(plan.max_share_per_run, None);
        assert!(!plan.pace_known);

        // A reading from an earlier window does not count.
        let plan = summarize(&[metered(400, 0.02, 36_000), metered(0, 0.90, 18_000)])
            .plan
            .unwrap();
        assert_eq!(plan.window_runs, 1);
        assert_eq!(plan.five_hour_used, 0.02);
        assert_eq!(plan.window_moved, None);

        // Backends that report no windows: no plan block at all.
        assert_eq!(summarize(&[run("refined", 700, None)]).plan, None);
    }

    #[test]
    fn empty_summary_has_no_times() {
        let s = summarize(&[]);
        assert_eq!(s, RefineSummary::default());
        assert_eq!(s.median_ms, None);
    }

    #[test]
    fn record_on_a_store_without_the_table_does_not_panic() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let store = Store::from_connection_for_test(conn);
        record(&store, &run("refined", 700, None));
        assert!(recent(&store, 10).is_err());
    }
}
