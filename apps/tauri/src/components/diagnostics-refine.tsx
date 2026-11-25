import { CircleAlert, CircleCheck, PlugZap, RefreshCw, Trash2 } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Spinner } from "@/components/ui/spinner";
import type { RefinePlanEstimate, RefineRun, RefineSummary } from "@/lib/refine";
import { REFINE_HISTORY_LIMIT, useRefineHistory } from "@/lib/use-refine-history";

// How many of the loaded runs the table lists.
const ROWS_SHOWN = 12;

const STATUS_LABEL: Record<string, string> = {
  refined: "Refined",
  "skipped-short": "Too short",
  timeout: "Timed out",
  unavailable: "Unavailable",
  failed: "Failed",
  rejected: "Rejected",
  // Asks (TASK-97): rows of the same table with the source "ask".
  answered: "Answered",
  cancelled: "Cancelled",
};

// Diagnostics → Refine: is the model reachable, how fast does it
// answer, and what happened on recent dictations.
//
// Asks go through the same backend and draw on the same plan, so
// they are rows here too, marked "Ask". They stay out of the Refine
// numbers (an ask that timed out is not a dictation that was typed
// as transcribed) and get a line of their own.
//
// Three questions, top to bottom: (1) can Refine run right now
// (connection row), (2) how is it performing (numbers over the last
// runs), (3) what exactly happened (the runs). Nothing here holds
// dictated text: the history stores timings and reasons only.
export function DiagnosticsRefineSection() {
  const {
    history,
    enabled,
    status,
    checking,
    test,
    testing,
    error,
    checkBackend,
    runTest,
    clear,
  } = useRefineHistory();

  const summary = history?.summary;
  const runs = history?.runs ?? [];
  // Older history answers have no ask counters.
  const asks = summary?.asks ?? 0;
  const refineRuns = summary ? summary.total - asks : 0;

  return (
    <section className="ow-diagnostics__section" data-testid="diagnostics-refine">
      <div className="ow-diagnostics__section-header">
        <h2 className="ow-diagnostics__section-title">Refine</h2>
        <span className="ow-diagnostics__section-meta">
          {enabled === null ? "…" : enabled ? "On" : "Off"} · last{" "}
          {REFINE_HISTORY_LIMIT} runs
        </span>
      </div>
      <Card size="sm" className="ow-diagnostics__card">
        <CardContent className="ow-diag-refine">
          <div className="ow-diag-refine__connection" data-testid="diagnostics-refine-connection">
            <span
              className="ow-diag-refine__state"
              data-ready={status ? String(status.ready) : "unknown"}
            >
              {!status ? (
                <Spinner />
              ) : status.ready ? (
                <CircleCheck size={14} aria-hidden="true" />
              ) : (
                <CircleAlert size={14} aria-hidden="true" />
              )}
              <span>{!status ? "Checking" : status.ready ? "Ready" : "Not ready"}</span>
            </span>
            <span className="ow-diag-refine__message">
              {status?.message ?? "Checking whether the model can be reached…"}
            </span>
            <span className="ow-diag-refine__actions">
              <Button
                variant="outline"
                size="sm"
                disabled={checking}
                onClick={() => void checkBackend()}
              >
                <RefreshCw data-icon="inline-start" />
                Check again
              </Button>
              <Button
                variant="outline"
                size="sm"
                disabled={testing}
                onClick={() => void runTest()}
              >
                {testing ? (
                  <Spinner data-icon="inline-start" />
                ) : (
                  <PlugZap data-icon="inline-start" />
                )}
                {testing ? "Testing…" : "Test now"}
              </Button>
            </span>
          </div>

          {test && (
            <p
              className="ow-diag-refine__note"
              data-testid="diagnostics-refine-test"
              data-ok={String(test.ok)}
            >
              {test.ok
                ? `Test answered in ${formatMs(test.elapsed_ms)} (started cold, so this is the slow case).`
                : `Test failed: ${test.message}`}
            </p>
          )}
          {error && (
            <p className="ow-diagnostics__error" role="alert">
              {error}
            </p>
          )}

          {summary && summary.total > 0 ? (
            <>
              {refineRuns > 0 && <SummaryReadouts summary={summary} />}
              {refineRuns > 0 && <StartLine summary={summary} />}
              {asks > 0 && <AskLine summary={summary} />}
              <UsageBlock summary={summary} />
              {summary.last_problem && (
                <p className="ow-diag-refine__note" data-testid="diagnostics-refine-last-problem">
                  Last problem{" "}
                  {summary.last_problem_at_ms
                    ? `(${formatAgo(summary.last_problem_at_ms)})`
                    : ""}
                  : {summary.last_problem}
                </p>
              )}
              <RunsTable runs={runs.slice(0, ROWS_SHOWN)} />
            </>
          ) : (
            <p className="ow-diagnostics__crashes-quiet" data-testid="diagnostics-refine-empty">
              {enabled === false
                ? "Refine is off. Turn it on under Settings → Refine and its runs show up here."
                : "No runs yet. Dictate something, or press Test now."}
            </p>
          )}

          <div className="ow-diag-refine__footer">
            <span className="ow-diag-refine__note">
              Timings and reasons only, never what you said. Also in a
              terminal: <code>openwhisper refine history</code>
            </span>
            {summary && summary.total > 0 && (
              <Button variant="ghost" size="sm" onClick={() => void clear()}>
                <Trash2 data-icon="inline-start" />
                Clear
              </Button>
            )}
          </div>
        </CardContent>
      </Card>
    </section>
  );
}

function SummaryReadouts({ summary }: { summary: RefineSummary }) {
  // Dictations a backend was asked about: not the ones too short to
  // send, and not the asks, which are counted on their own.
  const asked = summary.total - (summary.asks ?? 0) - summary.skipped;
  return (
    <div className="ow-diag-refine__readouts">
      <Stat
        id="median"
        label="Response time"
        value={formatMs(summary.median_ms)}
        sub="median, send to answer"
      />
      <Stat
        id="p90"
        label="Slow case"
        value={formatMs(summary.p90_ms)}
        sub={`9 in 10 are faster · slowest ${formatMs(summary.max_ms)}`}
      />
      <Stat
        id="refined"
        label="Refined"
        value={`${summary.refined} of ${asked}`}
        sub={
          summary.instructions > 0
            ? `${summary.instructions} followed a spoken instruction`
            : `${summary.skipped} more were too short to send`
        }
      />
      <Stat
        id="fallbacks"
        label="Typed as transcribed"
        value={String(summary.fallbacks)}
        sub={fallbackBreakdown(summary)}
        warn={summary.fallbacks > 0}
      />
    </div>
  );
}

function Stat({
  id,
  label,
  value,
  sub,
  warn,
}: {
  id: string;
  label: string;
  value: string;
  sub: string;
  warn?: boolean;
}) {
  return (
    <div className="ow-diag-refine__stat" data-warn={warn ? "true" : undefined}>
      <div className="ow-diagnostics__readout-label">{label}</div>
      <div className="ow-diag-refine__stat-value" data-testid={`diagnostics-refine-${id}`}>
        {value}
      </div>
      <div className="ow-diagnostics__readout-sub">{sub}</div>
    </div>
  );
}

// Whether the model was started while you talked (the fast path) or
// only once the transcript was ready.
function StartLine({ summary }: { summary: RefineSummary }) {
  if (summary.warm_runs + summary.cold_runs === 0) return null;
  return (
    <p className="ow-diag-refine__note" data-testid="diagnostics-refine-start">
      Model started while you talked: {runs(summary.warm_runs)}
      {summary.median_warm_ms !== null && `, median ${formatMs(summary.median_warm_ms)}`}
      {" · "}started only when the text was ready: {runs(summary.cold_runs)}
      {summary.median_cold_ms !== null && `, median ${formatMs(summary.median_cold_ms)}`}
    </p>
  );
}

// The asks among the runs: how many got an answer and how long the
// answered ones took, to the first text and to the end.
function AskLine({ summary }: { summary: RefineSummary }) {
  const times: string[] = [];
  if (summary.ask_median_first_text_ms !== null) {
    times.push(`first text after ${formatMs(summary.ask_median_first_text_ms)}`);
  }
  if (summary.ask_median_ms !== null) {
    times.push(`whole answer after ${formatMs(summary.ask_median_ms)}`);
  }
  return (
    <p className="ow-diag-refine__note" data-testid="diagnostics-refine-asks">
      Asks: {summary.asks_answered} of {summary.asks} answered
      {times.length > 0 && `. Median: ${times.join(", ")}`}
    </p>
  );
}

// What a dictation takes: tokens, and where the Claude plan stands.
//
// Two honest limits shape the wording. The plan's windows count all
// use of the plan (Claude Code, chat, Refine), and they are reported
// in whole percents. So Refine's own share can only be bounded from
// above, and when the window does not move a point we say so instead
// of inventing a number.
function UsageBlock({ summary }: { summary: RefineSummary }) {
  if (summary.usage_runs === 0) return null;
  const plan = summary.plan;
  return (
    <div className="ow-diag-refine__usage" data-testid="diagnostics-refine-usage">
      <div className="ow-diag-refine__readouts">
        <Stat
          id="tokens"
          label="Per dictation"
          value={`${formatCount(summary.avg_tokens_in)} → ${formatCount(summary.avg_tokens_out)}`}
          sub="tokens in → out, average"
        />
        {summary.avg_cost_usd !== null && (
          <Stat
            id="cost"
            label="API list price"
            value={formatUsd(summary.avg_cost_usd)}
            sub={`each · ${formatUsd(summary.total_cost_usd)} for these ${summary.usage_runs}. Not billed on a plan.`}
          />
        )}
        {plan && <PlanStats plan={plan} />}
      </div>
      {plan && <PlanNotes plan={plan} />}
    </div>
  );
}

function PlanStats({ plan }: { plan: RefinePlanEstimate }) {
  const reset = plan.five_hour_resets_at !== null && plan.five_hour_resets_at * 1000 < Date.now();
  return (
    <>
      <Stat
        id="window-5h"
        label="5-hour window"
        value={reset ? "reset" : formatPercent(plan.five_hour_used)}
        sub={
          reset
            ? "a new window has started since the last run"
            : plan.five_hour_resets_at !== null
              ? `used · resets ${formatClock(plan.five_hour_resets_at * 1000)}`
              : "used"
        }
      />
      {plan.seven_day_used !== null && (
        <Stat
          id="window-7d"
          label="7-day window"
          value={formatPercent(plan.seven_day_used)}
          sub={
            plan.seven_day_resets_at !== null
              ? `used · resets ${formatDay(plan.seven_day_resets_at * 1000)}`
              : "used"
          }
        />
      )}
    </>
  );
}

function PlanNotes({ plan }: { plan: RefinePlanEstimate }) {
  return (
    <>
      <p className="ow-diag-refine__note" data-testid="diagnostics-refine-share">
        {plan.max_share_per_run !== null
          ? `One dictation takes at most ${formatShare(plan.max_share_per_run)} of the 5-hour window: it moved ${Math.round((plan.window_moved ?? 0) * 100)} points across ${runs(plan.window_runs)}, and that includes everything else you did with Claude in that time.`
          : `One dictation is too small to see: ${runs(plan.window_runs)} in this window without it moving a single point.`}
      </p>
      {plan.pace_known && (
        <p className="ow-diag-refine__note" data-testid="diagnostics-refine-projection">
          {plan.full_at_ms !== null
            ? `At the pace since the first of those runs, the 5-hour window is full around ${formatClock(plan.full_at_ms)}. That pace is all your Claude use, not only Refine.`
            : "At the pace since the first of those runs, the 5-hour window resets before it fills."}
        </p>
      )}
      <p className="ow-diag-refine__note">
        Window readings are from the last run ({formatAgo(plan.as_of_ms)}) and
        cover your whole Claude plan.
      </p>
    </>
  );
}

function RunsTable({ runs }: { runs: RefineRun[] }) {
  return (
    <table className="ow-diag-refine__table" aria-label="Recent Refine runs">
      <thead>
        <tr>
          <th scope="col">When</th>
          <th scope="col">Result</th>
          <th scope="col" className="ow-diag-refine__num">
            Time
          </th>
          <th scope="col">Start</th>
          <th scope="col">Model</th>
          <th scope="col" className="ow-diag-refine__num">
            Tokens
          </th>
          <th scope="col">Note</th>
        </tr>
      </thead>
      <tbody>
        {runs.map((run, i) => (
          <tr key={`${run.at_ms}-${i}`} data-status={run.status} data-source={run.source}>
            <td>{formatAgo(run.at_ms)}</td>
            <td>{STATUS_LABEL[run.status] ?? run.status}</td>
            <td className="ow-diag-refine__num">
              {run.status === "skipped-short" ? "–" : formatMs(run.elapsed_ms)}
            </td>
            <td>
              {run.status === "skipped-short"
                ? "–"
                : run.warm_lead_ms !== null
                  ? `${formatMs(run.warm_lead_ms)} ahead`
                  : "cold"}
            </td>
            <td>
              {run.model}
              {run.effort ? ` · ${run.effort}` : ""}
            </td>
            <td className="ow-diag-refine__num">
              {run.tokens_in !== null && run.tokens_out !== null
                ? `${run.tokens_in} → ${run.tokens_out}`
                : "–"}
            </td>
            <td>{runNote(run)}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function runNote(run: RefineRun): string {
  const parts: string[] = [];
  // An ask, not a Refine run, with how long the first text took.
  if (run.source === "ask") {
    parts.push("Ask");
    // Rows from before asks existed have no such field.
    if (run.first_text_ms != null) {
      parts.push(`first text after ${formatMs(run.first_text_ms)}`);
    }
  }
  // Set when the run was made in one of the user's own modes.
  if (run.mode) parts.push(`Mode: ${run.mode}`);
  if (run.detail) {
    parts.push(run.detail);
  } else {
    if (run.source === "test") parts.push("connection test");
    if (run.instruction) parts.push("followed a spoken instruction");
  }
  return parts.join(" · ");
}

function runs(n: number): string {
  return n === 1 ? "1 run" : `${n} runs`;
}

function fallbackBreakdown(s: RefineSummary): string {
  const parts: string[] = [];
  if (s.timeouts) parts.push(`${s.timeouts} timed out`);
  if (s.unavailable) parts.push(`${s.unavailable} unavailable`);
  if (s.failed) parts.push(`${s.failed} failed`);
  if (s.rejected) parts.push(`${s.rejected} rejected`);
  return parts.length ? parts.join(" · ") : "no problems";
}

function formatMs(ms: number | null): string {
  if (ms === null) return "–";
  if (ms < 1000) return `${ms} ms`;
  return `${(ms / 1000).toFixed(ms < 10_000 ? 2 : 1)} s`;
}

function formatCount(n: number | null): string {
  return n === null ? "–" : n.toLocaleString("en-US");
}

// List prices per dictation are fractions of a cent.
function formatUsd(usd: number | null): string {
  if (usd === null) return "–";
  return `$${usd.toFixed(usd < 0.1 ? 4 : 2)}`;
}

function formatPercent(share: number): string {
  return `${Math.round(share * 100)}%`;
}

// A share of a window, small enough to need decimals.
function formatShare(share: number): string {
  const pct = share * 100;
  return `${pct < 1 ? pct.toFixed(2) : pct.toFixed(1)}%`;
}

function formatClock(atMs: number): string {
  return new Date(atMs).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

function formatDay(atMs: number): string {
  return new Date(atMs).toLocaleDateString([], { weekday: "short", day: "numeric", month: "short" });
}

function formatAgo(atMs: number): string {
  const seconds = Math.max(0, Math.floor((Date.now() - atMs) / 1000));
  if (seconds < 60) return "just now";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} h ago`;
  return new Date(atMs).toLocaleDateString();
}
