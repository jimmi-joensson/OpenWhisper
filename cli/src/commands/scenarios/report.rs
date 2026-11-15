//! What `openwhisper scenarios` prints: the list, one line per
//! scenario of a run with the details of a failure under it, the
//! totals, and the lines of the offline self-test. `--json` prints
//! one record in place of each.

use anyhow::Result;
use serde_json::{Value, json};

use super::super::ask::one_line;
use super::{Group, Host, Row, Scenario, SelfCheck, Status};

/// `scenarios list`.
pub fn list(scenarios: &[Scenario], json: bool) -> Result<()> {
    // What a run with the Claude backend would skip on this machine.
    let host = Host::default();
    if json {
        let value: Vec<Value> = scenarios
            .iter()
            .map(|s| {
                json!({
                    "name": s.name,
                    "group": s.group.id(),
                    "situation": s.situation,
                    "expect": s.expect,
                    "limit_ms": s.limit_ms,
                    "asks_the_model": s.live_model,
                    "skipped_here": (s.skip)(&host),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(());
    }
    for group in Group::ALL {
        println!("{} ({})", group.title(), group.id());
        for scenario in scenarios.iter().filter(|s| s.group == group) {
            println!("  {}", scenario.name);
            println!("      situation: {}", scenario.situation);
            println!("      expected:  {}", scenario.expect);
            println!(
                "      limit:     {} s{}",
                scenario.limit_ms / 1000,
                if scenario.live_model { "" } else { ", asks no model" }
            );
            if let Some(reason) = (scenario.skip)(&host) {
                println!("      skipped here: {reason}");
            }
        }
        println!();
    }
    println!("{} scenarios. Run them with: openwhisper scenarios run [--only GROUP_OR_PREFIX]", scenarios.len());
    Ok(())
}

/// What a person saw, on one line: the answer, or why there is none.
fn saw(row: &Row) -> String {
    let Some(seen) = &row.seen else {
        return String::new();
    };
    if !seen.answer.is_empty() {
        one_line(&seen.answer)
    } else if !seen.error.is_empty() {
        one_line(&format!("no answer: {}", seen.error))
    } else {
        String::new()
    }
}

/// The line of one scenario in a run.
pub fn line(row: &Row) -> String {
    match row.status {
        Status::Passed | Status::Failed => {
            format!("{}  {:<36} {:>6} ms  {}", row.status.label(), row.name, row.elapsed_ms, saw(row))
        }
        _ => format!("{}  {:<36}            {}", row.status.label(), row.name, one_line(&row.reason)),
    }
}

/// What goes under the line of a scenario that failed: the
/// situation, what was expected, what is wrong, and what was seen.
pub fn details(row: &Row) -> Vec<String> {
    if row.status != Status::Failed {
        return Vec::new();
    }
    let mut out = vec![
        format!("      situation: {}", row.situation),
        format!("      expected:  {}", row.expect),
        format!("      problem:   {}", row.problem),
    ];
    let Some(seen) = &row.seen else {
        return out;
    };
    let indent = "\n                 ";
    let mut add = |label: &str, text: &str| {
        if !text.trim().is_empty() {
            out.push(format!("      {label:<10} {}", text.trim().replace('\n', indent)));
        }
    };
    add("heard:", &seen.transcript);
    add("answer:", &seen.answer);
    add("error:", &seen.error);
    add("notice:", &seen.notice);
    add("hint:", &seen.suggestion);
    add("screen:", &format!("{} {}", seen.screen, seen.screen_notice));
    add("refused:", &seen.refused.join(", "));
    add("steps:", &seen.steps.join("; "));
    add("calls:", &seen.calls.join(", "));
    add("looked at:", &seen.context_source);
    if seen.captures > 0 {
        add("captures:", &seen.captures.to_string());
    }
    add("mode:", &format!("{} {} {}", seen.mode, seen.mode_reason, seen.status));
    out
}

/// The totals of a run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Summary {
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    /// Stopped at a limit, or not run because the run stopped.
    pub not_run: usize,
    pub elapsed_ms: u64,
}

impl Summary {
    pub fn of(rows: &[Row], elapsed_ms: u64) -> Self {
        let count = |status: Status| rows.iter().filter(|r| r.status == status).count();
        Self {
            passed: count(Status::Passed),
            failed: count(Status::Failed),
            skipped: count(Status::Skipped),
            not_run: count(Status::Stopped) + count(Status::NotRun),
            elapsed_ms,
        }
    }

    /// The last line of the report.
    pub fn line(&self) -> String {
        let mut parts = vec![format!("{} passed", self.passed), format!("{} failed", self.failed)];
        if self.skipped > 0 {
            parts.push(format!("{} skipped", self.skipped));
        }
        if self.not_run > 0 {
            parts.push(format!("{} not run", self.not_run));
        }
        format!("{}, in {:.1} s", parts.join(", "), self.elapsed_ms as f64 / 1000.0)
    }
}

/// A run that ended early: the backend refused a scenario because a
/// spend or rate limit of the account is reached. That scenario is
/// not a failed one, and nothing was sent after it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stopped {
    pub scenario: String,
    /// What the backend said.
    pub reason: String,
    /// The refused scenario and every one after it.
    pub not_run: usize,
}

impl Stopped {
    pub fn summary(&self) -> String {
        format!(
            "stopped: the backend reported a spend or rate limit at \"{}\" ({}). {} not run, nothing more was sent. \
The scenarios above are the ones that were judged; the run is not a result for the whole set.",
            self.scenario,
            one_line(&self.reason),
            if self.not_run == 1 { "1 scenario was".to_string() } else { format!("{} scenarios were", self.not_run) }
        )
    }

    pub fn to_json(&self) -> Value {
        json!({ "scenario": self.scenario, "reason": self.reason, "not_run": self.not_run })
    }
}

/// `scenarios run --offline`: print the checks of the self-test and
/// exit non-zero when one of them does not behave.
pub fn self_test(checks: &[SelfCheck], json: bool) -> Result<()> {
    let wrong = checks.iter().filter(|c| !c.passed).count();
    if json {
        let value = json!({
            "mode": "offline",
            "passed": checks.len() - wrong,
            "total": checks.len(),
            "checks": checks
                .iter()
                .map(|c| json!({ "scenario": c.scenario, "what": c.what, "passed": c.passed, "problem": c.problem }))
                .collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        for check in checks {
            println!(
                "{}  {:<36} {}{}",
                if check.passed { "ok   " } else { "WRONG" },
                check.scenario,
                check.what,
                if check.problem.is_empty() { String::new() } else { format!(": {}", check.problem) }
            );
        }
        println!("{} of {} checks behave (no model was asked)", checks.len() - wrong, checks.len());
    }
    if wrong > 0 {
        std::process::exit(1);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::Seen;
    use super::*;

    fn row(status: Status) -> Row {
        Row {
            name: "everyday-what-day".into(),
            group: Group::Everyday,
            status,
            elapsed_ms: 2_345,
            limit_ms: 20_000,
            situation: "They ask what day it is.".into(),
            expect: "Today's weekday.".into(),
            problem: String::new(),
            reason: String::new(),
            seen: Some(Seen { answer: "Today is\nThursday.".into(), ..Seen::default() }),
        }
    }

    #[test]
    fn a_line_has_the_verdict_the_name_the_time_and_what_was_seen() {
        let passed = line(&row(Status::Passed));
        assert!(passed.starts_with("PASS  everyday-what-day"), "{passed}");
        assert!(passed.ends_with("2345 ms  Today is Thursday."), "{passed}");
        assert!(details(&row(Status::Passed)).is_empty());

        let skipped = Row { reason: "needs the macOS system voice".into(), seen: None, ..row(Status::Skipped) };
        let skipped_line = line(&skipped);
        assert!(skipped_line.starts_with("SKIP  everyday-what-day"), "{skipped_line}");
        assert!(skipped_line.ends_with("needs the macOS system voice"), "{skipped_line}");
        assert!(!skipped_line.contains(" ms"), "{skipped_line}");

        let no_answer = Row {
            seen: Some(Seen { error: "no answer in time".into(), ..Seen::default() }),
            ..row(Status::Failed)
        };
        assert!(line(&no_answer).ends_with("no answer: no answer in time"));
    }

    #[test]
    fn a_failure_says_what_was_expected_what_is_wrong_and_what_was_seen() {
        let failed = Row {
            problem: "the answer does not say today's weekday".into(),
            seen: Some(Seen {
                answer: "Today is\nFriday.".into(),
                screen: "none".into(),
                context_source: "none".into(),
                calls: vec!["read_thread".into()],
                ..Seen::default()
            }),
            ..row(Status::Failed)
        };
        let text = details(&failed).join("\n");
        assert!(text.contains("situation: They ask what day it is."), "{text}");
        assert!(text.contains("expected:  Today's weekday."), "{text}");
        assert!(text.contains("problem:   the answer does not say today's weekday"), "{text}");
        assert!(text.contains("answer:    Today is\n                 Friday."), "{text}");
        assert!(text.contains("calls:     read_thread"), "{text}");
        // What there is nothing of is left out.
        assert!(!text.contains("heard:") && !text.contains("hint:") && !text.contains("captures:"), "{text}");
    }

    #[test]
    fn the_totals_count_every_kind_of_ending() {
        let rows = vec![
            row(Status::Passed),
            row(Status::Passed),
            row(Status::Failed),
            row(Status::Skipped),
            row(Status::Stopped),
            row(Status::NotRun),
        ];
        let summary = Summary::of(&rows, 61_300);
        assert_eq!(summary, Summary { passed: 2, failed: 1, skipped: 1, not_run: 2, elapsed_ms: 61_300 });
        assert_eq!(summary.line(), "2 passed, 1 failed, 1 skipped, 2 not run, in 61.3 s");
        assert_eq!(Summary::of(&rows[..2], 4_000).line(), "2 passed, 0 failed, in 4.0 s");
    }

    #[test]
    fn a_stopped_run_says_where_why_and_how_much_was_left() {
        let stop = Stopped {
            scenario: "everyday-what-day".into(),
            reason: "Claude Code: API Error: 429\nrate_limit_error".into(),
            not_run: 20,
        };
        let summary = stop.summary();
        assert!(summary.contains("a spend or rate limit at \"everyday-what-day\""), "{summary}");
        assert!(summary.contains("20 scenarios were not run, nothing more was sent."), "{summary}");
        assert!(!summary.contains('\n'));
        assert!(Stopped { not_run: 1, ..stop.clone() }.summary().contains("1 scenario was not run"));
        assert_eq!(stop.to_json()["not_run"], 20);
    }
}
