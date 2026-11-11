import { useEffect, useState } from "react";
import { CircleAlert, RefreshCw } from "lucide-react";

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
import { Input } from "@/components/ui/input";
import { Separator } from "@/components/ui/separator";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
import { VisionSettings } from "@/components/vision-settings";
import { askContext, type AskContextSettings } from "@/lib/ask";
import { configToChipKeys } from "@/lib/hotkey-format";
import {
  BACKEND_EFFORTS,
  CLAUDE_MODELS,
  EFFORT_LABELS,
  type RefineEffort,
} from "@/lib/refine";
import { settingsRowId } from "@/lib/navigate";
import type { SettingsPaneId } from "@/lib/settings-panes";
import { useAsk, useAskBackend, useAskContextPreview } from "@/lib/use-ask";
import { useCurrentHotkey } from "@/lib/use-current-hotkey";

// The first choice of the model and effort selects: nothing is
// stored, and core picks.
const DEFAULT_CHOICE = "__default__";

// Shown until `ask_get` has answered.
const FALLBACK_DESTINATION = "the model Refine is set up with";

// Longest "where you are" text, until `ask_get` has said what core
// keeps (`ASK_PLACE_MAX_CHARS`).
const FALLBACK_PLACE_MAX_CHARS = 120;

// The four groups of facts an ask reads from the computer, in the
// order core sends them, each with what is in it. Which facts a
// group holds is core's (`core/src/ask/context.rs`); this is the
// sentence that says so.
const CONTEXT_GROUPS: {
  key: Exclude<keyof AskContextSettings, "place">;
  id: string;
  label: string;
  description: string;
}[] = [
  {
    key: "date_time",
    id: "date-time",
    label: "Date and time",
    description: "Today's date, the time and your time zone.",
  },
  {
    key: "computer",
    id: "computer",
    label: "Computer",
    description:
      "Operating system, model, chip, memory, battery or plugged in, displays, free disk space and whether you are online. Never the network name, the IP address, the serial number, the name of the computer or your user name.",
  },
  {
    key: "front_app",
    id: "front-app",
    label: "App in front",
    description:
      "The name of the app you are working in when you ask. Only the name, never the title of the window.",
  },
  {
    key: "locale",
    id: "locale",
    label: "Language and region",
    description:
      "System language, region, units, 12 or 24 hour clock, currency and keyboard layout.",
  },
];

interface AskPaneProps {
  // Opens another settings pane: Refine, where the backend is set
  // up, and Shortcuts, where the ask hotkey is changed.
  onOpenPane?: (pane: SettingsPaneId) => void;
}

// Ask pane (TASK-97): the switch for the second hotkey, where
// questions are sent, how they are answered, and (the Screen
// section, TASK-98) when an ask includes a picture of the screen.
//
// A thin view over Rust (`ask_get`, `ask_set`, and `ask_status` for
// whether the backend can run). The backend itself (which model
// service, the sign-in, the key) is Refine's and is set up on the
// Refine pane; this pane only says what it is and whether it can
// run.
export function AskPane({ onOpenPane }: AskPaneProps) {
  const { state, error, update } = useAsk();
  const hotkey = useCurrentHotkey("ask");
  const hotkeyKeys = configToChipKeys(hotkey);

  const backend = useAskBackend();

  const settings = state?.settings;
  const claude = state ? state.backend_kind === "claude-cli" : true;
  const destination = state?.destination ?? FALLBACK_DESTINATION;

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

      {/* The id is what "Turn on web search" in the answer window
          leads to: App scrolls this row into view (lib/navigate.ts). */}
      <Field
        id={settingsRowId("ask", "web-search")}
        orientation="horizontal"
        data-disabled={claude ? undefined : true}
      >
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

      {state && settings && (
        <>
          <Separator />
          <ContextSection
            context={askContext(settings)}
            enabled={settings.enabled}
            destination={destination}
            placeMaxChars={state.limits.place_max_chars ?? FALLBACK_PLACE_MAX_CHARS}
            onChange={(patch) => update({ context: { ...askContext(settings), ...patch } })}
          />
        </>
      )}

      <Separator />

      {/* Vision (TASK-98): when an ask includes the screen. Its own
          section with its own data; only in use while Ask is on. */}
      <VisionSettings askEnabled={settings?.enabled ?? false} />
    </FieldGroup>
  );
}

// What an ask tells the model about this computer: four switches,
// the place the person types, and the exact text that would be sent
// right now. Everything shown is read or stored by Rust; with Ask
// off nothing is sent, and the controls wait for the switch.
function ContextSection({
  context,
  enabled,
  destination,
  placeMaxChars,
  onChange,
}: {
  context: AskContextSettings;
  // Ask is switched on.
  enabled: boolean;
  destination: string;
  placeMaxChars: number;
  onChange: (patch: Partial<AskContextSettings>) => void;
}) {
  const preview = useAskContextPreview();
  const disabled = !enabled;

  return (
    <>
      <SectionHeader>What the model is told about your computer</SectionHeader>
      <FieldDescription data-testid="ask-context-intro">
        These facts go with every question, to the same place as the
        question: {destination}.
      </FieldDescription>
      {disabled && (
        <FieldDescription data-testid="ask-context-off">
          Ask is switched off, so nothing is sent. Switch Ask on to
          change what the model is told.
        </FieldDescription>
      )}

      {CONTEXT_GROUPS.map((group) => (
        <Field
          key={group.key}
          orientation="horizontal"
          data-disabled={disabled ? true : undefined}
        >
          <FieldContent>
            <FieldLabel htmlFor={`ask-context-${group.id}`}>{group.label}</FieldLabel>
            <FieldDescription>{group.description}</FieldDescription>
          </FieldContent>
          <Switch
            id={`ask-context-${group.id}`}
            checked={context[group.key]}
            disabled={disabled}
            onCheckedChange={(on) => onChange({ [group.key]: on })}
          />
        </Field>
      ))}

      <PlaceField
        value={context.place}
        maxChars={placeMaxChars}
        disabled={disabled}
        onSave={(place) => onChange({ place })}
      />

      <Field>
        <div className="flex items-center justify-between gap-3">
          <FieldLabel htmlFor="ask-context-preview">What is sent right now</FieldLabel>
          <Button
            variant="outline"
            size="sm"
            disabled={preview.refreshing}
            onClick={preview.refresh}
          >
            <RefreshCw data-icon="inline-start" />
            Refresh
          </Button>
        </div>
        <Textarea
          id="ask-context-preview"
          readOnly
          value={preview.text ?? ""}
          spellCheck={false}
          className="font-mono text-xs"
          placeholder={
            preview.error
              ? "The preview could not be read."
              : preview.text === ""
                ? "Nothing. Every switch is off and no place is given."
                : "Reading."
          }
        />
        <FieldDescription>
          This is the exact text that goes with a question asked now.
          {context.front_app
            ? " The app in front is read when you ask, so it is not in this preview while this window is in front."
            : ""}
        </FieldDescription>
      </Field>
    </>
  );
}

function PlaceField({
  value,
  maxChars,
  disabled,
  onSave,
}: {
  // "" = no place given.
  value: string;
  maxChars: number;
  disabled: boolean;
  onSave: (place: string) => void;
}) {
  // Saved on blur or Enter, not per keystroke: every save rewrites
  // settings.json.
  const [text, setText] = useState(value);
  useEffect(() => setText(value), [value]);

  const commit = () => {
    const next = text.trim();
    if (next !== value) onSave(next);
  };

  return (
    <Field data-disabled={disabled ? true : undefined}>
      <FieldLabel htmlFor="ask-context-place">Where you are</FieldLabel>
      <Input
        id="ask-context-place"
        value={text}
        maxLength={maxChars}
        disabled={disabled}
        placeholder="For example: Aarhus, Denmark"
        onChange={(e) => setText(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") e.currentTarget.blur();
        }}
      />
      <FieldDescription>
        Optional. Without it the model only knows your time zone and
        region. Nothing here uses location services.
      </FieldDescription>
    </Field>
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
