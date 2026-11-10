import { useEffect, useState } from "react";
import {
  CircleAlert,
  CircleCheck,
  KeyRound,
  PlugZap,
  RefreshCw,
  RotateCcw,
} from "lucide-react";

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Separator } from "@/components/ui/separator";
import { Spinner } from "@/components/ui/spinner";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import {
  BACKEND_EFFORTS,
  CLAUDE_MODELS,
  EFFORT_LABELS,
  type RefineBackend,
  type RefineEffort,
} from "@/lib/refine";
import { useRefine } from "@/lib/use-refine";

// Where the OS keeps the API key. Each platform sees only its own
// wording (see openwhisper-project-principles).
const KEY_STORE = (() => {
  if (typeof navigator === "undefined") return "your system keychain";
  if (/mac/i.test(navigator.platform)) return "your Mac's Keychain";
  if (/win/i.test(navigator.platform)) return "Windows Credential Manager";
  return "your system keychain";
})();

// Shown until the backend probe answers.
const FALLBACK_DESTINATION: Record<RefineBackend, string> = {
  "claude-cli":
    "Anthropic (Claude), through the Claude Code app signed in on this computer",
  "openai-compatible": "the server you configure below",
};

// Refine pane: the optional language-model cleanup after transcription.
//
// Everything here is a thin view over Rust (`refine_*` commands). The
// one thing this pane must get right is saying plainly where the text
// goes: OpenWhisper is local until this switch is turned on.
export function RefinePane() {
  const {
    state,
    status,
    checking,
    test,
    testing,
    error,
    update,
    refreshStatus,
    runTest,
    setApiKey,
  } = useRefine();

  const settings = state?.settings;
  const backend: RefineBackend = settings?.backend ?? "claude-cli";
  // The status belongs to the saved backend; ignore a stale one while
  // the probe for a just-picked backend is in flight.
  const current = status && status.backend === backend ? status : null;
  const destination = current?.destination ?? FALLBACK_DESTINATION[backend];
  const seconds = ((settings?.timeout_ms ?? 5000) / 1000).toFixed(0);

  return (
    <FieldGroup className="px-1 py-2">
      <SectionHeader>Refine</SectionHeader>
      <FieldDescription>
        Cleans up each dictation with a language model before it is
        typed: punctuation, capitals, filler words and obvious
        mis-hearings. Transcription itself always stays on this
        computer.
      </FieldDescription>

      <Field orientation="horizontal">
        <FieldContent>
          <FieldLabel htmlFor="refine-enabled">Refine dictations</FieldLabel>
          <FieldDescription data-testid="refine-privacy">
            When on, the text of every dictation is sent to {destination}.
            Audio never leaves this computer. If no answer comes back
            within {seconds} seconds, or anything goes wrong, your words
            are typed as transcribed.
          </FieldDescription>
        </FieldContent>
        <Switch
          id="refine-enabled"
          checked={settings?.enabled ?? false}
          disabled={!settings}
          onCheckedChange={(enabled) => void update({ enabled })}
        />
      </Field>

      {error && (
        <FieldDescription className="text-destructive" role="alert">
          {error}
        </FieldDescription>
      )}

      <Separator />

      <SectionHeader>Model</SectionHeader>
      <Field orientation="horizontal">
        <FieldContent>
          <FieldLabel>Use</FieldLabel>
          <FieldDescription>
            {backend === "claude-cli"
              ? "Your own Claude Code, signed in with your Claude plan. Every dictation counts against that plan."
              : "Any service or local server that speaks the OpenAI chat API, with your own key."}
          </FieldDescription>
        </FieldContent>
        <ToggleGroup
          aria-label="Refine model"
          value={[backend]}
          disabled={!settings}
          onValueChange={(v) => {
            const next = v[0];
            if (next === "claude-cli" || next === "openai-compatible") {
              void update({ backend: next });
            }
          }}
          variant="outline"
        >
          <ToggleGroupItem value="claude-cli">Claude</ToggleGroupItem>
          <ToggleGroupItem value="openai-compatible">
            OpenAI-compatible
          </ToggleGroupItem>
        </ToggleGroup>
      </Field>

      {settings && backend === "claude-cli" && (
        <>
          <ClaudeModelField
            model={settings.claude_cli.model}
            onChange={(model) =>
              void update({ claude_cli: { ...settings.claude_cli, model } })
            }
          />
          <EffortField
            value={settings.claude_cli.effort}
            levels={BACKEND_EFFORTS["claude-cli"]}
            onChange={(effort) => {
              if (effort) void update({ claude_cli: { ...settings.claude_cli, effort } });
            }}
          />
        </>
      )}

      {settings && backend === "openai-compatible" && (
        <>
          <OpenAiFields
            baseUrl={settings.openai.base_url}
            model={settings.openai.model}
            hasKey={current?.has_api_key ?? false}
            onSave={({ base_url, model }) =>
              void update({ openai: { ...settings.openai, base_url, model } })
            }
            onSaveKey={setApiKey}
          />
          <EffortField
            value={settings.openai.effort}
            levels={BACKEND_EFFORTS["openai-compatible"]}
            allowNone
            onChange={(effort) =>
              void update({ openai: { ...settings.openai, effort } })
            }
          />
        </>
      )}

      <BackendStatusAlert
        ready={current?.ready ?? null}
        message={current?.message ?? "Checking…"}
        checking={checking}
        onRecheck={() => void refreshStatus()}
      />

      <Field orientation="horizontal">
        <FieldContent>
          <FieldLabel>Connection</FieldLabel>
          <FieldDescription>
            Sends one test sentence to {destination} and shows what
            comes back.
          </FieldDescription>
        </FieldContent>
        <Button
          variant="outline"
          disabled={!settings || testing}
          onClick={() => void runTest()}
        >
          {testing ? (
            <Spinner data-icon="inline-start" />
          ) : (
            <PlugZap data-icon="inline-start" />
          )}
          {testing ? "Testing…" : "Test connection"}
        </Button>
      </Field>

      {test && (
        <Alert
          variant={test.ok ? "default" : "destructive"}
          data-testid="refine-test-result"
        >
          {test.ok ? <CircleCheck /> : <CircleAlert />}
          <AlertTitle>
            {test.ok
              ? `Working. Answered in ${(test.elapsed_ms / 1000).toFixed(1)} s.`
              : "Not working"}
          </AlertTitle>
          <AlertDescription>
            {test.ok ? (
              <dl className="grid grid-cols-[auto_1fr] gap-x-2 gap-y-1">
                <dt>Sent</dt>
                <dd className="text-foreground">{state?.test_sentence}</dd>
                <dt>Got back</dt>
                <dd className="text-foreground">{test.output}</dd>
              </dl>
            ) : (
              test.message
            )}
          </AlertDescription>
        </Alert>
      )}

      <Separator />

      <SectionHeader>Instructions</SectionHeader>
      {state && settings && (
        <InstructionsField
          value={settings.instructions}
          fallback={state.default_instructions}
          onSave={(instructions) => void update({ instructions })}
        />
      )}
    </FieldGroup>
  );
}

function BackendStatusAlert({
  ready,
  message,
  checking,
  onRecheck,
}: {
  // null = not known yet.
  ready: boolean | null;
  message: string;
  checking: boolean;
  onRecheck: () => void;
}) {
  return (
    <Alert
      variant={ready === false ? "destructive" : "default"}
      data-testid="refine-backend-status"
      data-ready={ready === null ? "unknown" : String(ready)}
    >
      {ready === null ? (
        <Spinner />
      ) : ready ? (
        <CircleCheck />
      ) : (
        <CircleAlert />
      )}
      <AlertTitle>
        {ready === null ? "Checking" : ready ? "Ready" : "Not ready"}
      </AlertTitle>
      <AlertDescription>
        <p>{message}</p>
        {ready === false && (
          <Button
            variant="outline"
            size="sm"
            className="mt-2"
            disabled={checking}
            onClick={onRecheck}
          >
            <RefreshCw data-icon="inline-start" />
            Check again
          </Button>
        )}
      </AlertDescription>
    </Alert>
  );
}

// "Other" takes any full model name; the named choices are
// `CLAUDE_MODELS`.
const OTHER_MODEL = "__other__";

// What a model choice means for a dictation. Sonnet and Haiku numbers
// are from the live eval on 2026-11-10 (19 test dictations, model
// started while talking); the others are not measured.
function claudeModelNote(model: string): string {
  if (model === "sonnet") {
    return "Handled every test dictation correctly, in about 1.4 seconds.";
  }
  if (model === "haiku") {
    return "Fastest, about 0.9 seconds, and lightest on your plan, but about 1 in 5 test dictations came back uncleaned or with a spoken instruction ignored.";
  }
  return "Not measured here. A larger model is slower on every dictation and uses more of your plan. Use the connection test to see that your plan includes it and how long it takes.";
}

function ClaudeModelField({
  model,
  onChange,
}: {
  model: string;
  onChange: (model: string) => void;
}) {
  const known = CLAUDE_MODELS.some((m) => m.value === model);
  // "Other" stays open while the user types a name, even before it
  // is saved.
  const [other, setOther] = useState(!known);
  const [name, setName] = useState(known ? "" : model);
  useEffect(() => {
    if (!known) {
      setOther(true);
      setName(model);
    }
  }, [known, model]);

  const commit = () => {
    const next = name.trim();
    if (next && next !== model) onChange(next);
  };

  return (
    <>
      <Field orientation="horizontal">
        <FieldContent>
          <FieldLabel>Claude model</FieldLabel>
          <FieldDescription data-testid="refine-model-note">
            {claudeModelNote(other ? name.trim() || model : model)}
          </FieldDescription>
        </FieldContent>
        <Select
          items={[...CLAUDE_MODELS, { value: OTHER_MODEL, label: "Other…" }]}
          value={other ? OTHER_MODEL : model}
          onValueChange={(v) => {
            if (v === OTHER_MODEL) {
              setOther(true);
            } else if (typeof v === "string" && v) {
              setOther(false);
              setName("");
              onChange(v);
            }
          }}
        >
          <SelectTrigger className="w-[160px]" aria-label="Claude model">
            <SelectValue />
          </SelectTrigger>
          <SelectContent alignItemWithTrigger={false} align="end">
            <SelectGroup>
              {CLAUDE_MODELS.map((m) => (
                <SelectItem key={m.value} value={m.value}>
                  {m.label}
                </SelectItem>
              ))}
              <SelectItem value={OTHER_MODEL}>Other…</SelectItem>
            </SelectGroup>
          </SelectContent>
        </Select>
      </Field>
      {other && (
        <Field>
          <FieldLabel htmlFor="refine-claude-model-name">Model name</FieldLabel>
          <Input
            id="refine-claude-model-name"
            placeholder="A model name your Claude Code accepts"
            value={name}
            spellCheck={false}
            autoCapitalize="off"
            autoCorrect="off"
            onChange={(e) => setName(e.target.value)}
            onBlur={commit}
            onKeyDown={(e) => {
              if (e.key === "Enter") e.currentTarget.blur();
            }}
          />
          <FieldDescription>
            Passed to Claude Code as you type it, so anything your plan
            offers works.
          </FieldDescription>
        </Field>
      )}
    </>
  );
}

const EFFORT_NONE = "none";

// How hard the model thinks before answering.
function EffortField({
  value,
  levels,
  allowNone,
  onChange,
}: {
  value: RefineEffort | null;
  levels: RefineEffort[];
  // OpenAI-compatible servers often accept no effort at all.
  allowNone?: boolean;
  onChange: (effort: RefineEffort | null) => void;
}) {
  const items = [
    ...(allowNone ? [{ value: EFFORT_NONE, label: "Not sent" }] : []),
    ...levels.map((level) => ({ value: level as string, label: EFFORT_LABELS[level] })),
  ];
  return (
    <Field orientation="horizontal">
      <FieldContent>
        <FieldLabel>Effort</FieldLabel>
        <FieldDescription>
          {allowNone
            ? "How hard the model thinks first. “Not sent” leaves it to the server; many models take no effort setting."
            : "How hard the model thinks first. More effort is slower on every dictation. In testing, low was at least as accurate as medium."}
        </FieldDescription>
      </FieldContent>
      <Select
        items={items}
        value={value ?? EFFORT_NONE}
        onValueChange={(v) => {
          if (v === EFFORT_NONE) {
            if (allowNone) onChange(null);
          } else if (typeof v === "string" && (levels as string[]).includes(v)) {
            onChange(v as RefineEffort);
          }
        }}
      >
        <SelectTrigger className="w-[160px]" aria-label="Effort">
          <SelectValue />
        </SelectTrigger>
        <SelectContent alignItemWithTrigger={false} align="end">
          <SelectGroup>
            {items.map((item) => (
              <SelectItem key={item.value} value={item.value}>
                {item.label}
              </SelectItem>
            ))}
          </SelectGroup>
        </SelectContent>
      </Select>
    </Field>
  );
}

function OpenAiFields({
  baseUrl,
  model,
  hasKey,
  onSave,
  onSaveKey,
}: {
  baseUrl: string;
  model: string;
  hasKey: boolean;
  onSave: (openai: { base_url: string; model: string }) => void;
  onSaveKey: (key: string) => Promise<boolean>;
}) {
  // Text fields commit on blur or Enter, not per keystroke: every
  // save rewrites settings.json and re-checks the backend.
  const [url, setUrl] = useState(baseUrl);
  const [name, setName] = useState(model);
  const [key, setKey] = useState("");
  useEffect(() => setUrl(baseUrl), [baseUrl]);
  useEffect(() => setName(model), [model]);

  const commit = () => {
    if (url.trim() !== baseUrl || name.trim() !== model) {
      onSave({ base_url: url, model: name });
    }
  };
  const onEnter = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") e.currentTarget.blur();
  };

  return (
    <>
      <Field>
        <FieldLabel htmlFor="refine-openai-url">Server address</FieldLabel>
        <Input
          id="refine-openai-url"
          placeholder="https://api.openai.com/v1"
          value={url}
          spellCheck={false}
          autoCapitalize="off"
          autoCorrect="off"
          onChange={(e) => setUrl(e.target.value)}
          onBlur={commit}
          onKeyDown={onEnter}
        />
      </Field>
      <Field>
        <FieldLabel htmlFor="refine-openai-model">Model name</FieldLabel>
        <Input
          id="refine-openai-model"
          placeholder="The model's name at that server"
          value={name}
          spellCheck={false}
          autoCapitalize="off"
          autoCorrect="off"
          onChange={(e) => setName(e.target.value)}
          onBlur={commit}
          onKeyDown={onEnter}
        />
        <FieldDescription>
          Pick a small, fast one: it runs on every dictation.
        </FieldDescription>
      </Field>
      <Field>
        <FieldLabel htmlFor="refine-openai-key">API key</FieldLabel>
        <form
          className="flex items-center gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            if (!key.trim()) return;
            void onSaveKey(key).then((ok) => {
              if (ok) setKey("");
            });
          }}
        >
          <Input
            id="refine-openai-key"
            type="password"
            autoComplete="off"
            placeholder={hasKey ? "A key is stored. Paste a new one to replace it." : "Paste your key"}
            value={key}
            onChange={(e) => setKey(e.target.value)}
          />
          <Button type="submit" variant="outline" disabled={!key.trim()}>
            <KeyRound data-icon="inline-start" />
            Save key
          </Button>
          {hasKey && (
            <Button
              type="button"
              variant="ghost"
              onClick={() => void onSaveKey("")}
            >
              Remove
            </Button>
          )}
        </form>
        <FieldDescription data-testid="refine-key-note">
          Kept in {KEY_STORE}, never in a settings file. Leave empty for a
          local server that needs no key.
        </FieldDescription>
      </Field>
    </>
  );
}

function InstructionsField({
  value,
  fallback,
  onSave,
}: {
  // "" = the built-in instructions are in use.
  value: string;
  fallback: string;
  onSave: (instructions: string) => void;
}) {
  const [text, setText] = useState(value || fallback);
  useEffect(() => setText(value || fallback), [value, fallback]);
  const custom = value !== "";

  const commit = () => {
    const next = text.trim();
    // Saving the built-in text as "" keeps the user on the default,
    // so they get improvements to it in later versions.
    const stored = next === fallback.trim() ? "" : next;
    if (stored !== value) onSave(stored);
    if (!next) setText(fallback);
  };

  return (
    <Field>
      <FieldLabel htmlFor="refine-instructions">
        What Refine should always do
      </FieldLabel>
      <Textarea
        id="refine-instructions"
        value={text}
        rows={4}
        onChange={(e) => setText(e.target.value)}
        onBlur={commit}
      />
      <FieldDescription>
        This is done on every dictation. Your dictionary words are added
        automatically.
      </FieldDescription>
      <FieldDescription data-testid="refine-spoken-note">
        You can also tell Refine how to write something while you
        dictate: “scratch that”, “make that more formal”, “write this in
        English”, “put that in a bullet list”. It follows instructions
        about the wording, form or language of your text. A question or
        a task meant for someone else, like “write an email to Jeppe”,
        is typed as you said it and never answered or carried out.
      </FieldDescription>
      {custom && (
        <Button
          variant="outline"
          size="sm"
          className="self-start"
          onClick={() => onSave("")}
        >
          <RotateCcw data-icon="inline-start" />
          Back to the standard instructions
        </Button>
      )}
    </Field>
  );
}

function SectionHeader({ children }: { children: React.ReactNode }) {
  return (
    <h3 className="font-mono text-xs uppercase tracking-wider text-muted-foreground">
      {children}
    </h3>
  );
}
