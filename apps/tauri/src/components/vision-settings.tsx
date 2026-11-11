import { useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { CircleAlert, Plus, ShieldAlert, X } from "lucide-react";

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
import { cn } from "@/lib/utils";
import { useVision, useVisionRunningApps } from "@/lib/use-vision";
import {
  VISION_TRIGGERS,
  appLabel,
  triggerUsesPhrases,
  type NeverCaptureApp,
  type VisionState,
  type VisionTrigger,
} from "@/lib/vision";

// Same limits as core (`VISION_PHRASE_MAX_CHARS`,
// `VISION_APP_FIELD_MAX_CHARS`): core cuts longer text off, so the
// inputs do not take more.
const PHRASE_MAX_CHARS = 80;
const APP_ID_MAX_CHARS = 200;

interface VisionSettingsProps {
  // Ask is switched on. While it is off no ask runs, so nothing here
  // is in use and the controls are disabled.
  askEnabled: boolean;
}

// The "Screen" section of the Ask pane (Vision, TASK-98): when an ask
// includes a picture of the screen, the phrases that ask for it, the
// apps that are never in it, and the macOS permission.
//
// A thin view over Rust (`vision_get`, `vision_set`): core owns the
// block, cleans the lists and decides per question. Nothing here
// takes a screenshot, and no picture ever reaches this window.
export function VisionSettings({ askEnabled }: VisionSettingsProps) {
  const { state, error, update, requestPermission, requesting, openSystemSettings } =
    useVision();

  const disabled = !state || !askEnabled;
  const trigger = state?.settings.trigger ?? "phrases";
  const mac = state?.platform === "macos";
  const allowed = state?.permission === "granted";
  // An ask that wants the screen would go out without it.
  const missingPermission = askEnabled && !!state && mac && !allowed && trigger !== "off";

  return (
    <FieldGroup
      data-testid="vision-section"
      data-disabled={disabled ? "true" : undefined}
      role="group"
      aria-label="Screen"
    >
      <SectionHeader>Screen</SectionHeader>

      <div className="flex flex-col gap-2" data-testid="vision-intro">
        <FieldDescription>
          An ask can include a picture of your whole screen (the display
          with the window you are working in), so you can refer to what
          you see.
        </FieldDescription>
        <FieldDescription>
          The picture is only ever taken during an ask. It is never
          taken while you dictate and never in the background. It is
          sent with your question to the same place as the question,
          kept in memory only and not saved.
        </FieldDescription>
      </div>

      {!askEnabled && (
        <FieldDescription data-testid="vision-ask-off">
          Ask is off, so no picture is ever taken and these settings are
          not in use. Switch Ask on at the top of this page to change
          them.
        </FieldDescription>
      )}

      {missingPermission && (
        // `note`, not the Alert's own `alert`: it states a standing
        // fact and is not something that just went wrong.
        <Alert role="note" data-testid="vision-permission-notice">
          <CircleAlert />
          <AlertTitle>The screen cannot be included yet</AlertTitle>
          <AlertDescription>
            macOS has not allowed OpenWhisper to record the screen. Until
            it does, an ask goes out without the picture and the answer
            says so. Allow it under Screen recording permission below,
            then restart OpenWhisper.
          </AlertDescription>
        </Alert>
      )}

      {error && (
        <FieldDescription className="text-destructive" role="alert">
          {error}
        </FieldDescription>
      )}

      <TriggerField
        trigger={trigger}
        disabled={disabled}
        onChange={(next) => update({ trigger: next })}
      />

      {state && triggerUsesPhrases(trigger) && (
        <PhrasesField
          state={state}
          disabled={disabled}
          onChange={(phrases) => update({ phrases })}
        />
      )}

      {state && (
        <NeverCaptureField
          state={state}
          disabled={disabled}
          onChange={(never_capture) => update({ never_capture })}
        />
      )}

      {state && mac && (
        <PermissionField
          allowed={allowed}
          disabled={disabled}
          requesting={requesting}
          onRequest={requestPermission}
          onOpenSettings={openSystemSettings}
        />
      )}
    </FieldGroup>
  );
}

function TriggerField({
  trigger,
  disabled,
  onChange,
}: {
  trigger: VisionTrigger;
  disabled: boolean;
  onChange: (trigger: VisionTrigger) => void;
}) {
  // Control choice: the data is a fixed enumeration of four choices
  // that exclude each other, set once and seldom changed. That is a
  // Select, like the model and the effort above. Not a ToggleGroup:
  // the labels do not fit side by side next to the text, and what a
  // choice does needs a sentence, which is written out underneath
  // for all four so nobody has to open the list to compare them.
  const items = VISION_TRIGGERS.map(({ value, label }) => ({ value, label }));
  return (
    <Field orientation="horizontal" data-disabled={disabled ? "true" : undefined}>
      <FieldContent>
        <FieldLabel>When to include the screen</FieldLabel>
        <ul
          className="flex flex-col gap-1 text-sm text-muted-foreground"
          data-testid="vision-trigger-choices"
        >
          {VISION_TRIGGERS.map((choice) => (
            <li
              key={choice.value}
              data-choice={choice.value}
              data-selected={choice.value === trigger ? "true" : undefined}
            >
              <span
                className={cn("font-medium", choice.value === trigger && "text-foreground")}
              >
                {choice.label}:
              </span>{" "}
              {choice.description}
            </li>
          ))}
        </ul>
      </FieldContent>
      <Select
        items={items}
        value={trigger}
        disabled={disabled}
        onValueChange={(v) => {
          const choice = VISION_TRIGGERS.find((t) => t.value === v);
          if (choice) onChange(choice.value);
        }}
      >
        <SelectTrigger className="w-[160px]" aria-label="When to include the screen">
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

function PhrasesField({
  state,
  disabled,
  onChange,
}: {
  state: VisionState;
  disabled: boolean;
  // null = back to the built-in list.
  onChange: (phrases: string[] | null) => void;
}) {
  const [text, setText] = useState("");
  const phrases = state.phrases;

  const onAdd = () => {
    const phrase = text.trim();
    if (!phrase || disabled) return;
    // New ones go first, where they are seen. Core drops a phrase
    // that is already on the list (ignoring case).
    onChange([phrase, ...phrases]);
    setText("");
  };

  return (
    <Field data-disabled={disabled ? "true" : undefined} data-testid="vision-phrases">
      <FieldLabel htmlFor="vision-phrase">Phrases</FieldLabel>
      <FieldDescription>
        Say one of these in your question and the screen is included.
        Matching ignores upper and lower case and punctuation.
      </FieldDescription>
      <form
        className="flex items-center gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          onAdd();
        }}
      >
        <Input
          id="vision-phrase"
          className="flex-1"
          placeholder="look at my screen"
          maxLength={PHRASE_MAX_CHARS}
          value={text}
          disabled={disabled}
          onChange={(e) => setText(e.target.value)}
        />
        <Button type="submit" variant="outline" disabled={disabled || !text.trim()}>
          <Plus data-icon="inline-start" />
          Add phrase
        </Button>
      </form>

      {phrases.length === 0 ? (
        <FieldDescription data-testid="vision-phrases-empty">
          {state.settings.trigger === "smart"
            ? "No phrases. The screen is only included when the model says it needs it."
            : "No phrases. The screen is never included."}
        </FieldDescription>
      ) : (
        <Rows
          label="Phrases"
          testId="vision-phrase-list"
          rowTestId="vision-phrase-row"
          items={phrases}
          keyOf={(phrase) => phrase}
          nameOf={(phrase) => phrase}
          disabled={disabled}
          onRemove={(index) => onChange(phrases.filter((_, i) => i !== index))}
        >
          {(phrase) => <span className="truncate text-sm">{phrase}</span>}
        </Rows>
      )}

      {!state.phrases_are_default && (
        // In a row of its own: a direct child of a Field is
        // stretched to the full width.
        <div className="flex">
          <Button
            variant="outline"
            size="sm"
            disabled={disabled}
            data-testid="vision-phrases-restore"
            onClick={() => onChange(null)}
          >
            Restore defaults
          </Button>
        </div>
      )}
    </Field>
  );
}

function NeverCaptureField({
  state,
  disabled,
  onChange,
}: {
  state: VisionState;
  disabled: boolean;
  // null = back to the built-in list.
  onChange: (apps: NeverCaptureApp[] | null) => void;
}) {
  const [id, setId] = useState("");
  const apps = state.never_capture;
  const windows = state.platform === "windows";
  const running = useVisionRunningApps(!disabled);
  // Ids are compared ignoring case, as core does.
  const listed = new Set(apps.map((a) => a.id.toLowerCase()));
  const open = running.filter((a) => !listed.has(a.id.toLowerCase()));
  const openItems = open.map((a) => ({ value: a.id, label: appLabel(a) }));

  // New ones go first, where they are seen. Core drops an app whose
  // id is already on the list and stores ids in lower case.
  const add = (app: NeverCaptureApp) => onChange([app, ...apps]);

  const onAddById = () => {
    const typed = id.trim();
    if (!typed || disabled) return;
    // The name is known when the app happens to be open.
    const known = running.find((a) => a.id.toLowerCase() === typed.toLowerCase());
    add({ id: typed, name: known?.name ?? "" });
    setId("");
  };

  return (
    <Field data-disabled={disabled ? "true" : undefined} data-testid="vision-never">
      <FieldLabel>Never in a screenshot</FieldLabel>
      <FieldDescription data-testid="vision-never-note">
        {windows
          ? "The windows of the apps on this list are painted over before the picture is sent anywhere, so they are not in it."
          : "Apps on this list are left out of the picture, and the place where their windows are is painted over as well."}{" "}
        When one of them is the app in front, no picture is taken at all
        and the answer says so.
      </FieldDescription>
      <FieldDescription>
        Add an app that is open now, or type its id:{" "}
        {windows
          ? "the name of its exe file, for example vault.exe."
          : "its bundle id, for example com.example.vault."}
      </FieldDescription>

      <div className="flex flex-wrap items-center gap-2">
        {/* Control choice: one of a handful of apps, picked from what
            is open now. A discrete list that changes, so a Select
            (as for the app defaults under Modes). Picking adds the
            app; the select itself keeps no value. */}
        <Select
          items={openItems}
          value={null}
          disabled={disabled || open.length === 0}
          onValueChange={(v) => {
            const app = open.find((a) => a.id === v);
            if (app) add(app);
          }}
        >
          <SelectTrigger className="w-[200px]" aria-label="Add an open app">
            <SelectValue
              placeholder={open.length === 0 ? "No other open apps" : "Add an open app"}
            />
          </SelectTrigger>
          <SelectContent alignItemWithTrigger={false} align="start">
            <SelectGroup>
              {openItems.map((item) => (
                <SelectItem key={item.value} value={item.value}>
                  {item.label}
                </SelectItem>
              ))}
            </SelectGroup>
          </SelectContent>
        </Select>
        <form
          className="flex min-w-[240px] flex-1 items-center gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            onAddById();
          }}
        >
          <Input
            aria-label="App id"
            className="flex-1 font-mono"
            placeholder={windows ? "vault.exe" : "com.example.vault"}
            maxLength={APP_ID_MAX_CHARS}
            value={id}
            disabled={disabled}
            onChange={(e) => setId(e.target.value)}
          />
          <Button type="submit" variant="outline" disabled={disabled || !id.trim()}>
            <Plus data-icon="inline-start" />
            Add app
          </Button>
        </form>
      </div>

      {apps.length === 0 ? (
        <FieldDescription data-testid="vision-never-empty">
          No apps. Every app can end up in the picture.
        </FieldDescription>
      ) : (
        <Rows
          label="Apps that are never in a screenshot"
          testId="vision-never-list"
          rowTestId="vision-never-row"
          items={apps}
          keyOf={(app) => app.id}
          nameOf={appLabel}
          disabled={disabled}
          onRemove={(index) => onChange(apps.filter((_, i) => i !== index))}
        >
          {(app) => (
            <div className="flex min-w-0 flex-col">
              <span className="truncate text-sm font-medium">{appLabel(app)}</span>
              {app.name.trim() !== "" && (
                <span className="truncate font-mono text-xs text-muted-foreground">
                  {app.id}
                </span>
              )}
            </div>
          )}
        </Rows>
      )}

      {!state.never_capture_is_default && (
        // In a row of its own: a direct child of a Field is
        // stretched to the full width.
        <div className="flex">
          <Button
            variant="outline"
            size="sm"
            disabled={disabled}
            data-testid="vision-never-restore"
            onClick={() => onChange(null)}
          >
            Restore defaults
          </Button>
        </div>
      )}

      {/* Always on the page, never behind a tooltip: this is the part
          of the promise the list does not keep. `note`: a standing
          statement, not an alert that just fired. */}
      <Alert role="note" data-testid="vision-cannot-cover">
        <ShieldAlert />
        <AlertTitle>What a list of apps cannot cover</AlertTitle>
        <AlertDescription>
          <p>
            A password manager's popup inside your browser belongs to
            the browser, not to the password manager, so it is in the
            picture. A password shown on a web page or in a terminal is
            part of that window, so it is in the picture too.
          </p>
          <p>Close or hide these before you ask about your screen.</p>
        </AlertDescription>
      </Alert>
    </Field>
  );
}

function PermissionField({
  allowed,
  disabled,
  requesting,
  onRequest,
  onOpenSettings,
}: {
  allowed: boolean;
  disabled: boolean;
  requesting: boolean;
  onRequest: () => void;
  onOpenSettings: () => void;
}) {
  return (
    <Field
      orientation="horizontal"
      data-disabled={disabled ? "true" : undefined}
      data-testid="vision-permission"
      data-state={allowed ? "allowed" : "not-allowed"}
    >
      <FieldContent>
        <FieldLabel>Screen recording permission</FieldLabel>
        <FieldDescription data-testid="vision-permission-state">
          {allowed
            ? "Allowed. macOS lets OpenWhisper record the screen for an ask."
            : "Not allowed yet. macOS has to allow OpenWhisper to record the screen before an ask can include it."}
        </FieldDescription>
        <FieldDescription data-testid="vision-permission-restart">
          After you allow it in System Settings, quit OpenWhisper and
          open it again. The permission only counts after a restart.
        </FieldDescription>
      </FieldContent>
      <div className="flex shrink-0 flex-col items-stretch gap-2">
        {!allowed && (
          <Button
            variant="outline"
            size="sm"
            disabled={disabled || requesting}
            onClick={onRequest}
          >
            Ask for permission
          </Button>
        )}
        <Button variant="outline" size="sm" disabled={disabled} onClick={onOpenSettings}>
          Open System Settings
        </Button>
      </div>
    </Field>
  );
}

// A list whose rows can be removed, like the dictionary words. The
// list scrolls inside a fixed height: the built-in phrase list alone
// is some twenty rows.
function Rows<T>({
  label,
  testId,
  rowTestId,
  items,
  keyOf,
  nameOf,
  disabled,
  onRemove,
  children,
}: {
  label: string;
  testId: string;
  rowTestId: string;
  items: T[];
  keyOf: (item: T) => string;
  // What the remove button calls the row: "Remove <name>".
  nameOf: (item: T) => string;
  disabled: boolean;
  onRemove: (index: number) => void;
  children: (item: T) => ReactNode;
}) {
  const list = useRef<HTMLUListElement>(null);
  // Set when a row was removed with the keyboard: the row that takes
  // its place gets the focus, so it does not fall back to the page.
  const focusNext = useRef<number | null>(null);
  useLayoutEffect(() => {
    const index = focusNext.current;
    if (index === null) return;
    focusNext.current = null;
    const rows = list.current?.querySelectorAll<HTMLElement>(":scope > li");
    rows?.[Math.min(index, rows.length - 1)]?.focus();
  }, [items]);

  return (
    <ul
      ref={list}
      className="flex max-h-56 flex-col overflow-y-auto rounded-lg border"
      aria-label={label}
      data-testid={testId}
    >
      {items.map((item, i) => (
        // The row is a tab stop: the remove button is hidden until
        // the row is hovered or has keyboard focus (`.ow-hover-host`
        // in App.css), and a hidden button cannot be tabbed to. Tab
        // lands on the row, which shows it, and the next Tab reaches
        // the button.
        <li
          key={keyOf(item)}
          className="ow-hover-host flex min-h-9 items-center gap-2 border-b py-1 pr-1 pl-2.5 outline-none last:border-b-0 focus-visible:ring-2 focus-visible:ring-ring/50 focus-visible:ring-inset"
          data-testid={rowTestId}
          tabIndex={0}
        >
          <div className="flex min-w-0 flex-1 items-center">{children(item)}</div>
          {/* An explicit tab stop: the macOS webview leaves buttons
              out of the Tab order unless the system setting for
              keyboard navigation is on. */}
          <Button
            variant="ghost"
            size="icon-sm"
            className="ow-hover-reveal shrink-0"
            tabIndex={0}
            disabled={disabled}
            aria-label={`Remove ${nameOf(item)}`}
            onClick={(e) => {
              // `detail` is 0 for a click made with the keyboard.
              if (e.detail === 0) focusNext.current = i;
              onRemove(i);
            }}
          >
            <X />
          </Button>
        </li>
      ))}
    </ul>
  );
}

function SectionHeader({ children }: { children: ReactNode }) {
  return (
    <h3 className="font-mono text-xs uppercase tracking-wider text-muted-foreground">
      {children}
    </h3>
  );
}
