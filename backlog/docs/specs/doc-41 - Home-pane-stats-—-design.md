---
id: doc-41
title: Home-pane stats — design
type: specification
created_date: '2026-05-06 06:10'
---

**Backlog parent:** TASK-88
**Depends on:** TASK-87 (persistence foundation — `dictations` table)
**Mocks:** Home-pane mockup with 4-card stats strip (Words Today / Words This Week / Words All-Time / Time Saved); Time Saved card with `vs. typing at 40 wpm` and the wpm rendered as an underlined link.

## Problem

The Home pane currently shows an empty-state hero ("Ready when you are") and a single most-recent transcript row. There is no surface that tells the user how much they have used dictation. Without that, OW feels like a tool the user has to remember to use — instead of one that visibly compounds value over time. Stats also become the on-ramp to a future history surface (separate task) and to a future "share my stats" social moment, both deferred.

## Goal

Add a 4-card stats strip at the top of the Home pane that shows:

| Card | Value | Subcaption |
|---|---|---|
| Words Today | sum of `word_count` where `started_at` is today (local time) | "across this Mac" / "across this PC" platform-aware |
| Words This Week | sum of `word_count` over the last 7 days (rolling window) | "last 7 days" |
| Words All-Time | sum of all `word_count` | "since first launch" |
| Time Saved | `max(0, words_total / wpm − seconds_total / 60)` formatted as e.g. `2 min`, `1.4 hrs`, `42 sec` | `vs. typing at <wpm> wpm` where `<wpm>` is a clickable link |

Empty state (zero rows in `dictations`): every card shows `0` — the Time Saved value collapses to a long em-dash `—` instead of a number, with subcaption `vs. typing` (no wpm clause yet).

## Non-goals

- **No history list, no recent-dictations rendering.** That's a separate future feature; this spec only adds counters + the stats UI on top of the existing `dictations` table.
- **No transcript text in stats.** The stats writer never touches the `transcript` column. Only `started_at`, `duration_ms`, `word_count` are written.
- **No per-app breakdown.** Mock shows destination apps in the recent-dictations list, but that's part of the future history feature. Stats are aggregate-only.
- **No charts, no graphs, no time-series.** Four numbers, no Recharts dependency.
- **No timezone settings.** "Today" is the user's local timezone; "this week" is the last 7 days regardless of week boundary.
- **No built-in typing test.** WPM is user-input only in v1; in-app test is a future enhancement.
- **No share-my-stats / social.** Counters are local-only, never sent anywhere.

## Behavior model

### Write path

In `core/src/dictation.rs::dictation_deliver_transcript`, after `INJECTOR.inject(text)` succeeds, the function calls a new `core::stats::record_dictation(&store, text, elapsed_ms)`:

```rust
pub fn record_dictation(store: &Store, text: &str, started_at_ms: i64, duration_ms: i64) {
    let word_count = text.split_whitespace().count() as i64;
    if word_count == 0 { return; }   // empty transcripts don't count
    let result = store.with_conn(|c| {
        c.execute(
            "INSERT INTO dictations (started_at, duration_ms, word_count) VALUES (?1, ?2, ?3)",
            (started_at_ms, duration_ms, word_count),
        )?;
        Ok(())
    });
    if let Err(e) = result {
        tracing::warn!("stats record_dictation failed: {e:?}");
    }
}
```

Three properties locked here:

1. **Insertion happens after injection success** — so a failed paste does NOT count. Cancel and empty-transcript paths never reach this site.
2. **Empty transcript = no row** — `word_count == 0` early-returns. Prevents inflating session counts with no-content dictations.
3. **Failure is logged, not propagated** — disk-full / locked DB warns and returns; the dictation flow does not enter PHASE_ERROR.

`started_at_ms` comes from `record_start.elapsed()` math at the call site (we already track `record_start: Option<Instant>` in the dictation state). `duration_ms` is the elapsed wall time from record-start to delivery. Word count uses `split_whitespace` to match the user-visible word count (mockup's "16 words" badge would use the same definition).

### Read path

One Tauri command serves all four cards:

```rust
#[tauri::command]
fn stats_get_summary(store: State<Store>) -> Result<StatsSummary, String> {
    // Single connection borrow, four aggregates.
}

struct StatsSummary {
    words_today: i64,
    words_week: i64,
    words_all_time: i64,
    seconds_total: f64,
}
```

Implementation runs four queries inside one `with_conn` borrow (one mutex acquisition):

```sql
-- words_today
SELECT COALESCE(SUM(word_count), 0) FROM dictations WHERE started_at >= ?1 AND started_at < ?2;
-- words_week
SELECT COALESCE(SUM(word_count), 0) FROM dictations WHERE started_at >= ?3;
-- words_all_time + seconds_total
SELECT COALESCE(SUM(word_count), 0), COALESCE(SUM(duration_ms) / 1000.0, 0) FROM dictations;
```

Day boundaries are computed in Rust against the system's local timezone (`chrono::Local::now().date_naive()`). The 7-day boundary is "now minus 7×24h" (rolling, not calendar week). Computing in Rust keeps SQL portable should we ever change locale logic.

A second Tauri command handles reset:

```rust
#[tauri::command]
fn stats_reset(store: State<Store>) -> Result<(), String> {
    store.with_conn(|c| { c.execute("DELETE FROM dictations", [])?; Ok(()) })
}
```

`DELETE FROM dictations` is the simplest correct thing — `TRUNCATE` doesn't exist in SQLite. Auto-increment counter intentionally NOT reset; if a future history feature ever surfaces row IDs, a gap from a reset is fine.

### React surface

`StatsStrip` (new component, `apps/tauri/src/components/stats-strip.tsx`) renders 4 cards using shadcn `Card` composition. Each card has a small uppercase label, a large number, and a subcaption. Layout is `grid grid-cols-4 gap-3` on the desktop sizes OW supports (no responsive collapse — main window is fixed-ish min width).

Data comes from a new hook `useStatsSummary` (`apps/tauri/src/lib/use-stats-summary.ts`):

- On mount, invokes `stats_get_summary`.
- Subscribes to a new `stats_changed` event emitted by Rust after each successful `record_dictation` insert and after `stats_reset`. Hook re-fetches on each event.
- Returns `{ summary: StatsSummary | null, refresh: () => void }`.

Emit-on-change pattern (event-driven, not polling) costs nothing when idle and updates instantly after a dictation completes. Pattern matches the existing `useDictation`/`useLastTranscription` hooks.

**Time Saved formula** lives in React (display-only computation, not domain logic):

```ts
function timeSavedSeconds(words: number, dictationSeconds: number, wpm: number): number {
  return Math.max(0, words / wpm * 60 - dictationSeconds);
}

function formatTimeSaved(secs: number): string {
  if (secs < 1) return "—";
  if (secs < 60) return `${Math.round(secs)} sec`;
  const mins = secs / 60;
  if (mins < 60) return mins < 10 ? `${mins.toFixed(1)} min` : `${Math.round(mins)} min`;
  const hrs = mins / 60;
  return hrs < 10 ? `${hrs.toFixed(1)} hrs` : `${Math.round(hrs)} hrs`;
}
```

Negative time-saved (user dictates slower than they type — improbable but possible if WPM is set very high) clamps to 0 → renders `—`. Defensive, no error message needed.

### WPM setting

New field `user_wpm: u32` in the existing JSON settings store (NOT in SQLite — settings stay in JSON per the architectural decision):

- Default: `40` (matches mockup; widely cited average adult typist baseline).
- Validation: clamp to `[10, 300]` on save. Out-of-range input shows helper text "10–300 wpm" but the input is silently clamped on blur (no modal error). Rationale: typing speed is a personal calibration, not a security boundary; clamp + helper is friendlier than red error states.
- Surface: input field on the new Stats settings pane (see below). Number input with `min="10" max="300" step="1"`.
- Read by React via the existing settings get/set hooks. Stats strip subscribes to settings changes so the Time Saved card updates immediately when the user edits the value.

### Stats settings pane

New entry in `SETTINGS_PANES` (`apps/tauri/src/lib/settings-panes.ts`):

```ts
export const SETTINGS_PANES = [
  { id: "general", label: "General" },
  { id: "audio", label: "Audio" },
  { id: "models", label: "Models" },
  { id: "stats", label: "Stats" },         // NEW
  { id: "shortcuts", label: "Shortcuts" },
] as const;
```

Position: between Models and Shortcuts. The pane's icon (used in the sidebar nav) is lucide `BarChart3` — sticks to the existing icon vocabulary used by the other panes.

Pane content (`apps/tauri/src/components/stats-pane.tsx`, new):

- **Section: Typing speed**
  - Label: "Your typing speed"
  - shadcn `Input` (numeric, type="number", min/max/step as above)
  - Suffix unit: "wpm"
  - Helper text below: "Used for the Time Saved estimate on Home. The default 40 wpm is an average adult baseline. If unsure, take a free online typing test and enter the result."
- **Section: Danger zone**
  - shadcn `Card` with destructive-tinted styling: `border-destructive/40 bg-destructive/5` on the outer card.
  - `CardTitle` "Danger zone" rendered with `text-destructive`.
  - `CardDescription` "These actions are irreversible."
  - `CardContent` holds `Button variant="destructive"` "Reset all stats…"
  - Confirmation: shadcn `AlertDialog` — "This will permanently delete every recorded dictation count. The action cannot be undone." Confirm/Cancel.
  - On confirm: `invoke("stats_reset")`, then `refresh()` on the stats hook.

Both sections sit under the same pane because they both touch the same data domain. Putting Reset Stats in Diagnostics (the original idea) would split the cognitive load. The Danger zone treatment follows the GitHub-popularized convention: persistent, visually distinct, non-modal — the user always knows where the destructive lever lives without it being in their face. Future destructive actions (wipe-all-settings, reset-hotkeys) will get their own Danger zone Card at the bottom of their respective panes; the pattern scales without inflating the sidebar.

### In-line wpm link

The Time Saved card's subcaption is `vs. typing at <wpm> wpm`. The `<wpm> wpm` portion is a shadcn `Button variant="link"` (already installed in `apps/tauri/src/components/ui/button.tsx:21` — `text-primary underline-offset-4 hover:underline`):

```tsx
<span className="text-muted-foreground text-xs">
  vs. typing at{" "}
  <Button
    variant="link"
    size="sm"
    className="h-auto p-0 text-xs font-normal"
    onClick={() => onNavigateToStatsSetting()}
  >
    {wpm} wpm
    <Settings data-icon="inline-end" />
  </Button>
</span>
```

`h-auto p-0` overrides the button's default block sizing so it sits inline with the surrounding text. The lucide `Settings` (gear) icon at the canonical inline-end position reads "go change this in Settings" — more honest than `Pencil` (which would imply inline editing) or `ArrowUpRight` (which conventionally signals external navigation). `Settings` is already used elsewhere in the lucide vocabulary; consistency wins.

`onNavigateToStatsSetting` is a prop passed from `App.tsx` that flips `setRoute("settings")` AND `setSettingsPane("stats")` — same pattern the `⌘,` shortcut uses (App.tsx:98–108).

When the stats are empty (no recorded dictations), the Time Saved value is `—` and the subcaption simplifies to plain text `vs. typing` — no link, no number. Showing a clickable link to set WPM before any data exists is a footgun: the user has nothing to attach a "time saved" framing to yet.

### Empty state

First launch, no rows in `dictations`:

- Words Today / Week / All-Time → `0` (numeric zero, not em-dash — zero is a real value).
- Time Saved → `—` (no math possible without data).
- Subcaptions render as in the mockup, except the wpm link is suppressed (see above).

After the first successful dictation, all numbers reflect the new row immediately via the `stats_changed` event.

## Why these choices

**Stats live in the same `dictations` table that history will eventually use.** Single source of truth. The day history opt-in lands, the only change is "writer also fills `transcript`"; readers don't change. This is the entire reason for choosing SQLite over JSON in TASK-87.

**WPM in JSON settings, not SQLite.** Per the architectural decision: settings = preferences (different shape, different lifecycle). WPM is a per-user calibration that survives independently of stats data; if a user resets their stats they don't lose their WPM.

**Clamp on out-of-range, don't error.** WPM is a personal number, not a security boundary. The user typing 9 wpm because their kid grabbed the keyboard should not block them with a red error state — silently clamp to 10 and move on.

**Time-saved math in React.** It's pure display. Putting it in Rust would mean another Tauri command for the same data plus the WPM round-trip. Skip the layer.

**`stats_changed` event over polling.** Idle apps stay idle. The pill HUD already uses the same emit-after-state-change pattern.

**lucide `Settings` over `Pencil`.** Click navigates to a settings pane. `Pencil` reads "edit inline," which is a lie. `Settings` (gear) reads "go change this elsewhere," which is what happens.

**No charts.** Four numbers tell the story. A spark line costs Recharts (~80 KB gzipped) and a design decision (window? bin size?). Defer until there's user demand.

## Risks

- **Word-count drift.** `split_whitespace` after injection counts what was injected, but if the future LLM-cleanup feature (TASK-17 reopen) ever reshapes the injected text, the count must update at the same site. This is a single-site invariant; document it in the function doc-comment.
- **Day-boundary off-by-one across DST.** "Today" is `[start_of_today_local, start_of_tomorrow_local)`. On DST spring-forward, start_of_today shifts an hour. `chrono::Local` handles this correctly; just don't reimplement the math.
- **Reset during a recording.** If the user clicks Reset Stats mid-dictation, the in-flight `record_dictation` insert may land after the DELETE. Acceptable: the AlertDialog asks "delete every recorded dictation"; the new row is technically post-confirmation. v1 ships this behavior; if it bothers anyone, defer the DELETE until idle.
- **WPM = 0 if user bypasses validation somehow.** The Time Saved formula divides by wpm. Defensive: cap minimum at 10 in the formula too, so even a tampered settings file can't NaN/Infinity the display.
