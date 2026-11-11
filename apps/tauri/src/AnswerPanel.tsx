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
import { Check, CircleAlert, Copy } from "lucide-react";

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
import { ASK_TICK_EVENT, askIsBusy, type AskSnapshot } from "@/lib/ask";
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

// The answer window of Ask (TASK-97): what the person asked, where
// the ask stands, and the answer with Copy, Insert and Dismiss.
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

  // Follow the newest text while it streams, unless the person has
  // scrolled up to read. Whether the view is pinned to the end is
  // kept in a ref and the scroll position is written to the DOM: no
  // render depends on it.
  const scroller = useRef<HTMLDivElement>(null);
  const pinned = useRef(true);
  useLayoutEffect(() => {
    pinned.current = true;
    setCopied(false);
  }, [id]);
  useLayoutEffect(() => {
    const el = scroller.current;
    if (el && pinned.current) el.scrollTop = el.scrollHeight;
  }, [answer, phase, searching]);
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

  const busy = askIsBusy(phase);
  const showAnswer = hasAnswer && phase !== "cancelled";
  const status = statusLine(ask);
  const source = sourceLine(ask);

  return (
    <Card
      size="sm"
      className="answer-card"
      data-testid="answer-card"
      role="region"
      aria-label="Answer"
    >
      {ask.question && (
        <CardHeader className="shrink-0">
          <CardDescription className="line-clamp-2 text-xs" data-testid="answer-question">
            {ask.question}
          </CardDescription>
        </CardHeader>
      )}

      <CardContent
        ref={scroller}
        onScroll={onScroll}
        className="answer-body flex flex-col gap-2"
        data-testid="answer-scroll"
      >
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

      <CardFooter className="shrink-0 flex-col items-stretch gap-2 text-xs text-muted-foreground">
        {ask.notice && <p data-testid="answer-notice">{ask.notice}</p>}
        <div className="flex items-end justify-between gap-2">
          <p className="min-w-0 flex-1" data-testid="answer-source">
            {source}
          </p>
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
      if (ask.searching) {
        const query = ask.search_query.trim();
        return {
          text: query ? `Searching the web for “${query}”` : "Searching the web",
          working: true,
        };
      }
      return ask.phase === "thinking" ? { text: "Thinking", working: true } : null;
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
