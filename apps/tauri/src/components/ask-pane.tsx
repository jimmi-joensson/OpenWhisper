import { useEffect, useState } from "react";
import { CircleAlert } from "lucide-react";

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Separator } from "@/components/ui/separator";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
import { configToChipKeys } from "@/lib/hotkey-format";
import {
  BACKEND_EFFORTS,
  CLAUDE_MODELS,
  EFFORT_LABELS,
  type RefineEffort,
} from "@/lib/refine";
import type { SettingsPaneId } from "@/lib/settings-panes";
import { useAsk } from "@/lib/use-ask";
import { useCurrentHotkey } from "@/lib/use-current-hotkey";

// The first choice of the model and effort selects: nothing is
// stored, and core picks.
const DEFAULT_CHOICE = "__default__";

// Shown until `ask_get` has answered.
const FALLBACK_DESTINATION = "the model Refine is set up with";

interface AskPaneProps {
  // Opens another settings pane: Refine, where the backend is set
  // up, and Shortcuts, where the ask hotkey is changed.
  onOpenPane?: (pane: SettingsPaneId) => void;
}

// Ask pane (TASK-97): the switch for the second hotkey, where
// questions are sent, and how they are answered.
//
// A thin view over Rust (`ask_get`, `ask_set`). The backend itself
// (which model service, the sign-in, the key) is Refine's and is set
// up on the Refine pane; this pane only says what it is and whether
// it can run.
export function AskPane({ onOpenPane }: AskPaneProps) {
  const { state, error, update } = useAsk();
  const hotkey = useCurrentHotkey("ask");
  const hotkeyKeys = configToChipKeys(hotkey);

  const settings = state?.settings;
  const backend = state?.backend;
  const claude = backend ? backend.backend === "claude-cli" : true;
  const destination = backend?.destination ?? FALLBACK_DESTINATION;

  return (
    <FieldGroup className="px-1 py-2">
      <SectionHeader>Ask</SectionHeader>

      <Field orientation="horizontal">
        <FieldContent>
          <FieldLabel htmlFor="ask-enabled">Ask</FieldLabel>
          <FieldDescription>
            Gives you a second hotkey. Press it, ask a question out
            loud and press it again. The answer shows in a panel by
            the pill and is never typed by itself.
          </FieldDescription>
          <FieldDescription data-testid="ask-privacy">
            When on, the text of every question is sent to {destination}.
            Audio never leaves this computer.
          </FieldDescription>
        </FieldContent>
        <Switch
          id="ask-enabled"
          checked={settings?.enabled ?? false}
          disabled={!settings}
          onCheckedChange={(enabled) => update({ enabled })}
        />
      </Field>

      {backend && !backend.ready && (
        <Alert variant="destructive" data-testid="ask-backend-status">
          <CircleAlert />
          <AlertTitle>Not ready</AlertTitle>
          <AlertDescription>
            <p>{backend.message}</p>
            {onOpenPane && (
              <Button
                variant="outline"
                size="sm"
                className="mt-2"
                onClick={() => onOpenPane("refine")}
              >
                Open Refine settings
              </Button>
            )}
          </AlertDescription>
        </Alert>
      )}

      {error && (
        <FieldDescription className="text-destructive" role="alert">
          {error}
        </FieldDescription>
      )}

      <Field orientation="horizontal">
        <FieldContent>
          <FieldLabel>Hotkey</FieldLabel>
          <FieldDescription>
            The key that starts and stops a question. Change it under
            Shortcuts.
          </FieldDescription>
        </FieldContent>
        <div className="flex items-center gap-3">
          {hotkeyKeys.length > 0 ? (
            <span className="ow-shortcuts__chip" data-testid="ask-hotkey">
              {hotkeyKeys.map((key, i) => (
                <kbd key={i}>{key}</kbd>
              ))}
            </span>
          ) : (
            <span
              className="ow-shortcuts__chip ow-shortcuts__chip--empty"
              data-testid="ask-hotkey"
            >
              Not set
            </span>
          )}
          {onOpenPane && (
            <Button variant="outline" size="sm" onClick={() => onOpenPane("shortcuts")}>
              Open Shortcuts
            </Button>
          )}
        </div>
      </Field>

      <Separator />

      <SectionHeader>Answers</SectionHeader>

      <Field orientation="horizontal" data-disabled={claude ? undefined : true}>
        <FieldContent>
          <FieldLabel htmlFor="ask-web-search">Web search</FieldLabel>
          <FieldDescription data-testid="ask-web-search-note">
            {claude
              ? "The model can look things up on the web when a question needs it. Answers take longer. Works with the Claude backend only."
              : "Web search only works with the Claude backend. Refine is set up with an OpenAI-compatible server, so it is not available."}
          </FieldDescription>
        </FieldContent>
        <Switch
          id="ask-web-search"
          checked={claude && (settings?.web_search ?? false)}
          disabled={!settings || !claude}
          onCheckedChange={(web_search) => update({ web_search })}
        />
      </Field>

      {settings && claude && (
        <>
          <ModelField
            model={settings.model}
            onChange={(model) => update({ model })}
          />
          <EffortField
            effort={settings.effort}
            onChange={(effort) => update({ effort })}
          />
        </>
      )}
      {state && !claude && (
        <FieldDescription data-testid="ask-model-note">
          Questions are answered by the model set up under Refine
          {state.model ? ` (${state.model})` : ""}.
        </FieldDescription>
      )}

      {state && settings && (
        <InstructionsField
          value={settings.instructions}
          maxChars={state.limits.instructions_max_chars}
          onSave={(instructions) => update({ instructions })}
        />
      )}
    </FieldGroup>
  );
}

function ModelField({
  model,
  onChange,
}: {
  // null = the default model.
  model: string | null;
  onChange: (model: string | null) => void;
}) {
  // A model name saved elsewhere (the CLI takes any name) is offered
  // too, so the select can show what is stored.
  const listed = model === null || CLAUDE_MODELS.some((m) => m.value === model);
  const items = [
    { value: DEFAULT_CHOICE, label: "Default" },
    ...CLAUDE_MODELS,
    ...(listed || model === null ? [] : [{ value: model, label: model }]),
  ];
  return (
    <Field orientation="horizontal">
      <FieldContent>
        <FieldLabel>Model</FieldLabel>
        <FieldDescription>
          The Claude model that answers. A larger model is slower and
          uses more of your plan.
        </FieldDescription>
      </FieldContent>
      <Select
        items={items}
        value={model ?? DEFAULT_CHOICE}
        onValueChange={(v) => {
          if (typeof v !== "string" || !v) return;
          onChange(v === DEFAULT_CHOICE ? null : v);
        }}
      >
        <SelectTrigger className="w-[160px]" aria-label="Model">
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

function EffortField({
  effort,
  onChange,
}: {
  // null = the default effort.
  effort: RefineEffort | null;
  onChange: (effort: RefineEffort | null) => void;
}) {
  const levels = BACKEND_EFFORTS["claude-cli"];
  const items = [
    { value: DEFAULT_CHOICE, label: "Default" },
    ...levels.map((level) => ({ value: level as string, label: EFFORT_LABELS[level] })),
  ];
  return (
    <Field orientation="horizontal">
      <FieldContent>
        <FieldLabel>Effort</FieldLabel>
        <FieldDescription>
          How hard the model thinks first. More effort makes every
          answer slower.
        </FieldDescription>
      </FieldContent>
      <Select
        items={items}
        value={effort ?? DEFAULT_CHOICE}
        onValueChange={(v) => {
          if (v === DEFAULT_CHOICE) onChange(null);
          else if (typeof v === "string" && (levels as string[]).includes(v)) {
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

function InstructionsField({
  value,
  maxChars,
  onSave,
}: {
  // "" = no standing instructions.
  value: string;
  maxChars: number;
  onSave: (instructions: string) => void;
}) {
  // Saved on blur, not per keystroke: every save rewrites
  // settings.json.
  const [text, setText] = useState(value);
  useEffect(() => setText(value), [value]);

  const commit = () => {
    const next = text.trim();
    if (next !== value) onSave(next);
  };

  return (
    <Field>
      <FieldLabel htmlFor="ask-instructions">Instructions</FieldLabel>
      <Textarea
        id="ask-instructions"
        value={text}
        rows={3}
        maxLength={maxChars}
        placeholder="For example: keep answers short."
        onChange={(e) => setText(e.target.value)}
        onBlur={commit}
      />
      <FieldDescription>
        Optional. Followed for every question. Your dictionary words
        are added automatically.
      </FieldDescription>
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
