import {
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Ban, Check, CircleAlert, Copy, Hand } from "lucide-react";

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
} from "@/components/ui/card";
import { Spinner } from "@/components/ui/spinner";
import {
  Trace,
  TraceDescription,
  TraceIcon,
  TraceLine,
  TraceTitle,
} from "@/components/ui/trace";
import {
  ASK_TICK_EVENT,
  askIsBusy,
  type AskConfirm,
  type AskSnapshot,
  type AskStep,
  type AskStepState,
} from "@/lib/ask";
import { CLAUDE_MODELS } from "@/lib/refine";
import { useStoredTheme } from "@/lib/use-theme";

// The design tokens, Tailwind and the shadcn component styles all
// live in App.css, so this window loads it too. What App.css says
// about the page itself (an opaque background) is taken back by
// AnswerPanel.css, which must stay the later import. Both arrive
// only with this module, which main.tsx imports for the window
// labelled "answer" alone.
import "./App.css";
import "./AnswerPanel.css";

// How long "Cancelled" stays before the panel closes itself.
const CANCELLED_SHOWN_MS = 900;
// How long the Copy button says "Copied".
const COPIED_SHOWN_MS = 1200;
// The view counts as "at the newest text" within this many pixels of
// the end, so a line that is still growing does not unpin it.
const FOLLOW_SLACK_PX = 24;
// How long a confirm has to be on show in this window before its
// Allow button is active (`CONFIRM_ARM_MS` in core, which refuses a
// yes that comes sooner).
const CONFIRM_ARM_MS = 600;

// The answer window of Ask (TASK-97): what the person asked, where
// the ask stands, and the answer with Copy, Insert and Dismiss. With
// Vision (TASK-98) it also says what became of the screen: that it
// is being looked at, that it was included, or why it was not. Only
// ever in words: the picture itself never comes to this window.
//
// With Agent (TASK-99) it shows every tool call of the ask as it
// happens, and the one moment an ask waits for the person: a tool
// that sends, posts, deletes or changes something runs only after
// they pressed Allow on exactly that call, with its whole input in
// front of them. What a tool was given and what it returned is text
// from outside and is only ever shown as text.
//
// Everything shown comes from core's ask snapshot: `ask_snapshot`
// once on mount, so a window that loads late shows the current ask,
// then every `ask_tick`. The buttons are commands; the shell hides
// the window when the ask is back to idle.
//
// The window never takes keyboard focus (so "Insert" pastes into the
// app the person was in). Hence no inputs, no shortcuts and no focus
// calls in here, and every control is a visible button: hover states
// are not something a window that is never active can count on.
export function AnswerPanel() {
  const [ask, setAsk] = useState<AskSnapshot | null>(null);
  const applyTheme = useStoredTheme();
  // The ask on screen, to tell when a new one begins.
  const shownId = useRef(0);

  useEffect(() => {
    let alive = true;
    // An older version never replaces a newer one: the snapshot read
    // on mount can arrive after the first ticks.
    const take = (next: AskSnapshot | null) => {
      if (!alive || !next) return;
      if (next.id !== shownId.current) {
        shownId.current = next.id;
        // The window stays loaded between asks. Should a change of
        // theme in the main window not have reached it, pick it up
        // before the next answer shows.
        applyTheme();
      }
      setAsk((prev) => (prev && prev.version > next.version ? prev : next));
    };
    void invoke<AskSnapshot | null>("ask_snapshot").then(take).catch(() => {});
    const unlisten = listen<AskSnapshot>(ASK_TICK_EVENT, (event) => take(event.payload));
    return () => {
      alive = false;
      void unlisten.then((fn) => fn());
    };
  }, [applyTheme]);

  const phase = ask?.phase ?? "idle";
  return (
    <div className="answer-root" data-testid="answer-panel" data-phase={phase}>
      {ask && phase !== "idle" && <AnswerCard ask={ask} />}
    </div>
  );
}

function AnswerCard({ ask }: { ask: AskSnapshot }) {
  const { id, phase, answer, searching } = ask;
  const hasAnswer = answer.trim() !== "";
  const [copied, setCopied] = useState(false);
  // Agent (TASK-99). A snapshot from a shell that does not know
  // about tools has neither field.
  const steps = ask.steps ?? [];
  // The confirm that was just answered from here, so the card closes
  // with the click and not a tick later.
  const [answered, setAnswered] = useState<string | null>(null);
  const waiting = phase === "confirming" ? (ask.confirm ?? null) : null;
  const confirm = waiting && waiting.request_id !== answered ? waiting : null;
  // The step the confirm is about. While its card shows it is taken
  // out of the part that scrolls and shown right above the card, so
  // it cannot be scrolled out of view.
  const waitingStep = confirm ? (steps.find((step) => step.id === confirm.step_id) ?? null) : null;
  const listedSteps = waitingStep ? steps.filter((step) => step !== waitingStep) : steps;
  // The confirm whose names (the tool's title, its own name and its
  // connection) did not fit whole in the room the card had: the card
  // says so once it has measured (`ConfirmCard`), and everything
  // that only repeats the names then steps aside for it.
  const [crampedRequest, setCrampedRequest] = useState<string | null>(null);
  const tight = confirm !== null && confirm.request_id === crampedRequest;
  // Changes when a step is added or ends.
  const stepsKey = steps.map((step) => `${step.id}:${step.state}`).join(" ");

  // Follow the newest text while it streams, unless the person has
  // scrolled up to read. Whether the view is pinned to the end is
  // kept in a ref and the scroll position is written to the DOM: no
  // render depends on it.
  const scroller = useRef<HTMLDivElement>(null);
  const pinned = useRef(true);
  useLayoutEffect(() => {
    pinned.current = true;
    setCopied(false);
    setAnswered(null);
    setCrampedRequest(null);
  }, [id]);
  useLayoutEffect(() => {
    const el = scroller.current;
    if (el && pinned.current) el.scrollTop = el.scrollHeight;
  }, [answer, phase, searching, stepsKey]);
  const onScroll = () => {
    const el = scroller.current;
    if (!el) return;
    pinned.current = el.scrollHeight - el.scrollTop - el.clientHeight <= FOLLOW_SLACK_PX;
  };

  // A cancelled ask says so briefly, then closes: the shell hides
  // the window once the ask is dismissed. The close names the ask it
  // is for. If a new ask has started by the time it lands (the timer
  // fired and the next tick was still on its way), Rust leaves that
  // one alone.
  useEffect(() => {
    if (phase !== "cancelled") return;
    const timer = setTimeout(
      () => void invoke("ask_dismiss", { id }).catch(() => {}),
      CANCELLED_SHOWN_MS,
    );
    return () => clearTimeout(timer);
  }, [phase, id]);

  useEffect(() => {
    if (!copied) return;
    const timer = setTimeout(() => setCopied(false), COPIED_SHOWN_MS);
    return () => clearTimeout(timer);
  }, [copied]);

  const onCopy = () => {
    void invoke<boolean>("ask_copy")
      .then((ok) => {
        if (ok) setCopied(true);
      })
      .catch(() => {});
  };
  // Every command names the ask this panel is showing, so it can
  // only ever act on that one.
  const run = (command: "ask_cancel" | "ask_insert" | "ask_dismiss") => {
    void invoke(command, { id }).catch(() => {});
  };

  // Yes or no to a tool call that waits. The ask and the request are
  // the ones of the card the click was made on (it hands them over),
  // never "whatever waits now": a click can only ever answer the
  // confirm the person was looking at. Rust ignores one that has run
  // out of time or been replaced, and a yes that comes too early.
  const answerConfirm = (command: ConfirmCommand, askId: number, requestId: string) => {
    setAnswered(requestId);
    void invoke<boolean>(command, { askId, requestId })
      .then((ok) => {
        // Not taken: show the confirm again if it is still there.
        if (!ok) setAnswered((now) => (now === requestId ? null : now));
      })
      .catch(() => setAnswered((now) => (now === requestId ? null : now)));
  };

  // "Turn on web search": the main window comes forward on the Ask
  // pane, at the Web search row. This window stays as it is.
  const openWebSearchSetting = () => {
    void invoke("open_settings_window", { pane: "ask", focus: "web-search" }).catch(() => {});
  };

  const busy = askIsBusy(phase);
  const showAnswer = hasAnswer && phase !== "cancelled";
  // The model marked the question as one that needed the web, and
  // web search is off. The sentence and the link are the app's own:
  // nothing the model wrote is shown for it, and nothing it writes
  // ever becomes a link. Not for an ask that failed or was cancelled.
  const suggestWebSearch =
    ask.suggestion === "web-search" && (phase === "answering" || phase === "done");
  const status = statusLine(ask);
  const source = sourceLine(ask);
  // No screenshot although one was wanted, with a reason to show.
  const noScreen =
    (ask.screen === "blocked" || ask.screen === "unavailable") && ask.screen_notice !== "";
  // The app in front has a connection, and the ask was pointed at
  // that instead of the screen.
  const usedConnection =
    ask.context_source?.kind === "connection" ? ask.context_source.name : null;
  // While a confirm shows, the part that scrolls is only there when
  // it has something in it: it would take room from the card.
  const hasBody =
    !confirm || listedSteps.length > 0 || showAnswer || status !== null || phase === "error";

  return (
    <Card
      size="sm"
      className="answer-card"
      data-testid="answer-card"
      data-confirming={confirm ? "true" : undefined}
      data-tight={tight ? "true" : undefined}
      role="region"
      aria-label="Answer"
    >
      {ask.question && (
        <CardHeader className="answer-header shrink-0">
          <CardDescription
            className="answer-question line-clamp-2 text-xs"
            data-testid="answer-question"
          >
            {ask.question}
          </CardDescription>
        </CardHeader>
      )}

      {hasBody && (
        <CardContent
          ref={scroller}
          onScroll={onScroll}
          className="answer-body flex flex-col gap-2"
          data-testid="answer-scroll"
        >
          {listedSteps.length > 0 && <Steps steps={listedSteps} />}
          {showAnswer && <AnswerText text={answer} />}
          {status && (
            <p
              className="flex items-center gap-1.5 text-[13px] text-muted-foreground"
              role="status"
              data-testid="answer-status"
            >
              {status.working && <Spinner className="size-3.5 motion-reduce:animate-none" />}
              <span>{status.text}</span>
            </p>
          )}
          {phase === "error" && (
            <Alert variant="destructive" data-testid="answer-error">
              <CircleAlert />
              <AlertTitle>No answer</AlertTitle>
              <AlertDescription>{ask.error || "Something went wrong."}</AlertDescription>
            </Alert>
          )}
        </CardContent>
      )}

      {/* Outside the part that scrolls, so it is always in view: the
          step that waits, and the card that asks about it. The card
          is keyed by the ask and the request, so a new confirm is a
          new card: nothing of the one before (a click on its way,
          how long it has been on show) carries over. */}
      {confirm && (
        <CardContent className="answer-confirm">
          {waitingStep && <Steps steps={[waitingStep]} waiting />}
          <ConfirmCard
            key={`${id}:${confirm.request_id}`}
            askId={id}
            confirm={confirm}
            onAnswer={answerConfirm}
            onCramped={setCrampedRequest}
          />
        </CardContent>
      )}

      <CardFooter className="answer-footer shrink-0 flex-col items-stretch gap-2 text-xs text-muted-foreground">
        {/* While a confirm shows, the notes of the footer step aside:
            the window is fixed in size, and the input to judge needs
            the room. They are back when the confirm is answered. */}
        {!confirm && ask.notice && (
          <p className="answer-wrap" data-testid="answer-notice">
            {ask.notice}
          </p>
        )}
        {!confirm && suggestWebSearch && (
          <p data-testid="answer-suggestion">
            Web search is off, so this answer could not look anything up.{" "}
            {/* Underlined always: this window is never the active
                one, so a hover state cannot be what marks the link. */}
            <Button
              variant="link"
              size="xs"
              className="h-auto p-0 align-baseline underline"
              onClick={openWebSearchSetting}
            >
              Turn on web search
            </Button>
          </p>
        )}
        {/* Vision (TASK-98): a screenshot was wanted and there is
            none. Core's sentence says why (an app that is never
            captured is in front, no permission, no display). */}
        {!confirm && noScreen && (
          <p data-testid="answer-screen-notice" data-screen={ask.screen}>
            {ask.screen_notice}
          </p>
        )}
        {/* Agent (TASK-99): the app in front has a connection, so
            the ask reads from that. No screenshot was taken, unless
            "Screen included" below says one was asked for. A line of
            its own: next to the buttons it would wrap. */}
        {!confirm && usedConnection !== null && (
          <p data-testid="answer-context-source">
            {busy ? "Using" : "Used"} the {usedConnection} connection
            {ask.screen === "included" ? "" : " instead of the screen"}
          </p>
        )}
        <div className="flex items-end justify-between gap-2">
          <div className="answer-source flex min-w-0 flex-1 flex-col gap-0.5">
            <p data-testid="answer-source">{source}</p>
            {/* The question went out with a picture of the screen. */}
            {ask.screen === "included" && (
              <p data-testid="answer-screen-included">Screen included</p>
            )}
          </div>
          <div className="flex shrink-0 items-center gap-1.5">
            {(phase === "done" || phase === "error") && (
              <Button
                variant={phase === "done" ? "ghost" : "outline"}
                size="sm"
                onClick={() => run("ask_dismiss")}
              >
                Dismiss
              </Button>
            )}
            {showAnswer && (
              <Button variant="outline" size="sm" onClick={onCopy}>
                {copied ? <Check data-icon="inline-start" /> : <Copy data-icon="inline-start" />}
                {copied ? "Copied" : "Copy"}
              </Button>
            )}
            {busy && (
              <Button variant="outline" size="sm" onClick={() => run("ask_cancel")}>
                Cancel
              </Button>
            )}
            {phase === "done" && (
              <Button size="sm" onClick={() => run("ask_insert")}>
                Insert
              </Button>
            )}
          </div>
        </div>
      </CardFooter>
    </Card>
  );
}

// The line that says what is happening when it is not (only) the
// answer: listening, thinking, searching the web, cancelled.
// `working` puts a spinner in front of it.
function statusLine(ask: AskSnapshot): { text: string; working: boolean } | null {
  switch (ask.phase) {
    case "listening":
      // The pill shows the recording; no second meter here.
      return { text: "Listening", working: false };
    case "thinking":
    case "answering":
      // The screen is being captured for this ask (Vision, TASK-98).
      // It takes a moment at most; the pill shows it too.
      if (ask.screen === "capturing") {
        return { text: "Looking at your screen", working: true };
      }
      if (ask.searching) {
        const query = ask.search_query.trim();
        return {
          text: query ? `Searching the web for “${query}”` : "Searching the web",
          working: true,
        };
      }
      return ask.phase === "thinking" ? { text: "Thinking", working: true } : null;
    // "confirming" has no line of its own: the confirm is on screen,
    // and the step it is about says it waits.
    case "cancelled":
      return { text: "Cancelled", working: false };
    default:
      return null;
  }
}

// Where the question goes, in plain words: the backend, the model
// and whether web search is on. "" before the backend is known.
function sourceLine(ask: AskSnapshot): string {
  if (!ask.backend) return "";
  const claude = ask.backend === "claude-cli";
  const backend = claude
    ? "Claude"
    : ask.backend === "openai-compatible"
      ? "your OpenAI-compatible server"
      : ask.backend;
  const model = claude
    ? (CLAUDE_MODELS.find((m) => m.value === ask.model)?.label ?? ask.model)
    : ask.model;
  let line = `${ask.phase === "listening" ? "Will be sent to" : "Sent to"} ${backend}`;
  if (model) line += ` (${model})`;
  if (ask.web_search) line += ", web search on";
  return line;
}

// --- the tool calls of an agent ask ----------------------------------
//
// One line per call: what it is, on which connection, and where it
// stands, with its input under it on one line. Everything in here
// that came from a tool or from the model (names, the input, why a
// call failed) is shown as text.

const STEP_STATES: Record<AskStepState, string> = {
  running: "Running",
  done: "Done",
  failed: "Failed",
  denied: "Not run",
  waiting: "Waiting for you",
};

function StepIcon({ state }: { state: AskStepState }) {
  switch (state) {
    case "running":
      return <Spinner className="size-3.5 motion-reduce:animate-none" />;
    case "done":
      return <Check />;
    case "failed":
      return <CircleAlert />;
    case "denied":
      return <Ban />;
    case "waiting":
      return <Hand />;
  }
}

// `waiting`: the one step a confirm is about, shown above its card.
// Its input is left out there: the card under it shows all of it.
function Steps({ steps, waiting = false }: { steps: AskStep[]; waiting?: boolean }) {
  return (
    <Trace
      role="list"
      aria-label={waiting ? "The tool call that waits for you" : "Tool calls"}
      className={waiting ? "answer-step-waiting shrink-0 gap-1.5" : "gap-1.5"}
      data-testid={waiting ? "answer-steps-waiting" : "answer-steps"}
    >
      {steps.map((step) => (
        <div
          key={step.id}
          role="listitem"
          className="min-w-0 flex-col"
          data-testid="answer-step"
          data-state={step.state}
          data-kind={step.kind}
        >
          <TraceLine
            status={step.state === "failed" ? "error" : undefined}
            className="text-[13px]"
          >
            <TraceIcon>
              <StepIcon state={step.state} />
            </TraceIcon>
            <TraceTitle className="answer-untrusted" data-testid="answer-step-title">
              {step.title || step.tool}
            </TraceTitle>
            <TraceDescription
              className="answer-untrusted min-w-0 shrink truncate"
              data-testid="answer-step-connection"
            >
              {step.connection}
            </TraceDescription>
            <span className="shrink-0" data-testid="answer-step-state">
              {STEP_STATES[step.state] ?? step.state}
            </span>
          </TraceLine>
          {step.input_summary && !waiting && (
            <span
              className="answer-untrusted truncate pl-6 font-mono text-[11px] text-muted-foreground"
              data-testid="answer-step-input"
            >
              {step.input_summary}
            </span>
          )}
          {/* Why it failed or did not run, in core's words (you said
              no, nobody answered in time, Claude Code stopped while
              it waited). Long text wraps, wherever it has to. What a
              call that worked returned is the model's to use, not
              shown here. */}
          {step.outcome && (step.state === "failed" || step.state === "denied") && (
            <span
              className="answer-untrusted answer-wrap pl-6 text-xs text-muted-foreground"
              data-testid="answer-step-outcome"
            >
              {step.outcome}
            </span>
          )}
        </div>
      ))}
    </Trace>
  );
}

type ConfirmCommand = "ask_confirm_allow" | "ask_confirm_deny";

// Whether a yes to the confirm this card shows counts yet. Core
// counts down from the moment it raised the confirm and says how
// long is left (`armed_in_ms`) on every snapshot; a timer here takes
// it from there, so the button becomes active without waiting for
// another tick. The card also has to have been on show in this
// window for as long: a click that was on its way to something else
// when the card came up must not land on an active Allow.
function useArmed(armedInMs: number | undefined): boolean {
  const [armed, setArmed] = useState(false);
  // When this card came up here. A card is mounted once per confirm.
  const shownAt = useRef<number | null>(null);
  useEffect(() => {
    if (armed) return;
    const now = performance.now();
    if (shownAt.current === null) shownAt.current = now;
    const floor = shownAt.current + CONFIRM_ARM_MS - now;
    const timer = setTimeout(() => setArmed(true), Math.max(floor, armedInMs ?? 0, 0));
    return () => clearTimeout(timer);
  }, [armed, armedInMs]);
  return armed;
}

// How tall the input is at least, in lines: four, or all of it when
// it is shorter than that. A long line counts for the lines it wraps
// to (about fifty characters each).
function inputLines(input: string): number {
  const lines = input.split("\n").reduce((n, line) => n + Math.max(1, Math.ceil(line.length / 50)), 0);
  return Math.min(4, Math.max(1, lines));
}

// A tool call that waits for a yes or no: which tool, on which
// connection, and everything it would be given. The names are shown
// whole, however long, and the input scrolls in the room that is
// left. Deny is the button that stands out, comes first for the
// keyboard and is active from the start. Allow is never a default,
// sits at the other end of the card and is inactive for a moment
// after the card came up (`useArmed`). The window does not take the
// keyboard, so no key press answers this.
function ConfirmCard({
  askId,
  confirm,
  onAnswer,
  onCramped,
}: {
  askId: number;
  confirm: AskConfirm;
  onAnswer: (command: ConfirmCommand, askId: number, requestId: string) => void;
  // The names of this request do not fit whole in the room there is.
  onCramped: (requestId: string) => void;
}) {
  // The ask and the request this card was made for. A click answers
  // these and nothing else.
  const [made] = useState(() => ({ askId, requestId: confirm.request_id }));
  const armed = useArmed(confirm.armed_in_ms);
  const title = confirm.title || confirm.tool;

  // The names are never cut. When they do not fit in the room the
  // card has (the part they are in would have to scroll), the panel
  // is told, before anything is painted, and makes room. Measured
  // again whenever that part changes size. Layout only: the DOM is
  // read, nothing here animates.
  const names = useRef<HTMLDivElement>(null);
  const { requestId } = made;
  useLayoutEffect(() => {
    const el = names.current;
    if (!el) return;
    const measure = () => {
      if (el.scrollHeight > el.clientHeight) onCramped(requestId);
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, [requestId, onCramped]);
  return (
    <Alert
      role="group"
      aria-label="Allow this tool call?"
      className="answer-confirm-card"
      data-testid="answer-confirm"
      data-request={made.requestId}
      data-armed={armed ? "true" : "false"}
    >
      <div ref={names} className="answer-confirm-names">
        <AlertTitle className="text-[13px] leading-[18px]" data-testid="answer-confirm-title">
          Allow{" "}
          <span className="answer-untrusted" data-testid="answer-confirm-name">
            {title}
          </span>{" "}
          on{" "}
          <span className="answer-untrusted" data-testid="answer-confirm-connection">
            {confirm.connection}
          </span>
          ?
        </AlertTitle>
        <p className="text-xs text-muted-foreground" data-testid="answer-confirm-what">
          <span className="answer-untrusted font-mono text-[11px]" data-testid="answer-confirm-tool">
            {confirm.tool}
          </span>{" "}
          would run with exactly this input:
        </p>
      </div>
      <pre
        className="answer-confirm-input answer-untrusted ow-selectable"
        style={{ minHeight: inputLines(confirm.input) * 16 + 12 }}
        data-testid="answer-confirm-input"
      >
        {confirm.input}
      </pre>
      {/* Deny first in the document and last on screen, where the
          main button of the panel sits; Allow at the far end. */}
      <div className="answer-confirm-actions">
        <Button
          type="button"
          size="sm"
          onClick={() => onAnswer("ask_confirm_deny", made.askId, made.requestId)}
        >
          Deny
        </Button>
        <p className="text-xs text-muted-foreground" data-testid="answer-confirm-timeout">
          No answer within {confirm.timeout_s} seconds counts as no.
        </p>
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={!armed}
          onClick={() => {
            if (armed) onAnswer("ask_confirm_allow", made.askId, made.requestId);
          }}
        >
          Allow
        </Button>
      </div>
    </Alert>
  );
}

// --- the answer text -------------------------------------------------
//
// The answer is plain text from a model. It is shown with its line
// breaks, and the lightest Markdown is rendered: paragraphs, bullet
// and numbered lists, bold, inline code and code blocks; a heading
// becomes a bold line. Everything is built as React elements, so the
// text can never become markup: a `<script>` in an answer is shown as
// the characters it is. Links are shown as text (the window must not
// navigate anywhere).

function AnswerText({ text }: { text: string }) {
  const blocks = useMemo(() => parseBlocks(text), [text]);
  return (
    <div className="answer-md ow-selectable" data-testid="answer-text">
      {blocks.map((block, i) => (
        <BlockView key={i} block={block} />
      ))}
    </div>
  );
}

interface ListItem {
  lines: string[];
  // Items indented under this one. Shown as bullets.
  children: string[][];
}

type Block =
  | { kind: "paragraph"; lines: string[] }
  | { kind: "heading"; text: string }
  | { kind: "code"; text: string }
  | { kind: "list"; ordered: boolean; start: number; items: ListItem[] };

const FENCE = /^ {0,3}(`{3,}|~{3,})/;
const HEADING = /^ {0,3}#{1,6}\s+(.*?)\s*#*\s*$/;
const RULE = /^ {0,3}([-*_])(?:\s*\1){2,}\s*$/;
const BULLET = /^(\s*)[-*+]\s+(.*)$/;
const NUMBERED = /^(\s*)(\d{1,9})[.)]\s+(.*)$/;

function isBlank(line: string): boolean {
  return line.trim() === "" || RULE.test(line);
}

function listMarker(
  line: string,
): { indent: number; ordered: boolean; start: number; text: string } | null {
  const bullet = BULLET.exec(line);
  if (bullet) return { indent: bullet[1].length, ordered: false, start: 1, text: bullet[2] };
  const numbered = NUMBERED.exec(line);
  if (numbered) {
    return {
      indent: numbered[1].length,
      ordered: true,
      start: Number(numbered[2]),
      text: numbered[3],
    };
  }
  return null;
}

// A line right under a paragraph begins a list: a bullet, or a
// numbered item that starts at 1 (so "2019. That year" stays prose).
function startsList(line: string): boolean {
  const marker = listMarker(line);
  return marker !== null && (!marker.ordered || marker.start === 1);
}

// The answer as blocks. Written for text that is still arriving: a
// code block without its closing fence runs to the end.
function parseBlocks(text: string): Block[] {
  const lines = text.replace(/\r\n?/g, "\n").split("\n");
  const blocks: Block[] = [];
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];
    if (isBlank(line)) {
      i += 1;
      continue;
    }

    const fence = FENCE.exec(line);
    if (fence) {
      const close = new RegExp(`^ {0,3}${fence[1][0]}{${fence[1].length},}\\s*$`);
      const body: string[] = [];
      i += 1;
      while (i < lines.length && !close.test(lines[i])) {
        body.push(lines[i]);
        i += 1;
      }
      i += 1;
      blocks.push({ kind: "code", text: body.join("\n") });
      continue;
    }

    const heading = HEADING.exec(line);
    if (heading) {
      blocks.push({ kind: "heading", text: heading[1] });
      i += 1;
      continue;
    }

    const first = listMarker(line);
    if (first) {
      const items: ListItem[] = [];
      while (i < lines.length) {
        const current = lines[i];
        if (isBlank(current)) {
          // A blank line between two items does not end the list.
          let next = i + 1;
          while (next < lines.length && isBlank(lines[next])) next += 1;
          const after = next < lines.length ? listMarker(lines[next]) : null;
          if (!after || (after.indent < first.indent + 2 && after.ordered !== first.ordered)) break;
          i = next;
          continue;
        }
        if (FENCE.test(current)) break;
        const marker = listMarker(current);
        const last = items[items.length - 1];
        if (marker) {
          const nested = last !== undefined && marker.indent >= first.indent + 2;
          if (nested) {
            last.children.push([marker.text]);
          } else if (marker.ordered === first.ordered) {
            items.push({ lines: [marker.text], children: [] });
          } else {
            break;
          }
        } else if (last !== undefined && /^\s/.test(current)) {
          // An indented line continues the item above it.
          const target = last.children[last.children.length - 1] ?? last.lines;
          target.push(current.trim());
        } else {
          break;
        }
        i += 1;
      }
      blocks.push({ kind: "list", ordered: first.ordered, start: first.start, items });
      continue;
    }

    const paragraph: string[] = [];
    while (
      i < lines.length &&
      !isBlank(lines[i]) &&
      !FENCE.test(lines[i]) &&
      !HEADING.test(lines[i]) &&
      !(paragraph.length > 0 && startsList(lines[i]))
    ) {
      paragraph.push(lines[i]);
      i += 1;
    }
    blocks.push({ kind: "paragraph", lines: paragraph });
  }
  return blocks;
}

function BlockView({ block }: { block: Block }) {
  switch (block.kind) {
    case "code":
      return (
        <pre>
          <code>{block.text}</code>
        </pre>
      );
    case "heading":
      return (
        <p>
          <strong>{inline(block.text)}</strong>
        </p>
      );
    case "list": {
      const items = block.items.map((item, i) => (
        <li key={i}>
          <Lines lines={item.lines} />
          {item.children.length > 0 && (
            <ul>
              {item.children.map((child, j) => (
                <li key={j}>
                  <Lines lines={child} />
                </li>
              ))}
            </ul>
          )}
        </li>
      ));
      return block.ordered ? <ol start={block.start}>{items}</ol> : <ul>{items}</ul>;
    }
    case "paragraph":
      return (
        <p>
          <Lines lines={block.lines} />
        </p>
      );
  }
}

// Lines of one paragraph or list item, with their line breaks kept.
function Lines({ lines }: { lines: string[] }) {
  return (
    <>
      {lines.map((line, i) => (
        <span key={i}>
          {i > 0 && <br />}
          {inline(line)}
        </span>
      ))}
    </>
  );
}

// `code`, **bold** and [label](address) inside one line. Group 1 is
// the code, 2 the bold text, 3 and 4 a link's label and address.
const INLINE = /`([^`\n]+)`|\*\*(\S(?:[^\n]*?\S)?)\*\*|\[([^\]\n]+)\]\(([^)\s]+)\)/;

function inline(text: string): ReactNode[] {
  const out: ReactNode[] = [];
  const pattern = new RegExp(INLINE.source, "g");
  let from = 0;
  for (let match = pattern.exec(text); match !== null; match = pattern.exec(text)) {
    if (match.index > from) out.push(text.slice(from, match.index));
    const [, code, bold, label, address] = match;
    if (code !== undefined) {
      out.push(<code key={match.index}>{code}</code>);
    } else if (bold !== undefined) {
      out.push(<strong key={match.index}>{inline(bold)}</strong>);
    } else {
      // A link is text here, never something to click.
      out.push(label === address ? address : `${label} (${address})`);
    }
    from = match.index + match[0].length;
  }
  if (from < text.length) out.push(text.slice(from));
  return out;
}
