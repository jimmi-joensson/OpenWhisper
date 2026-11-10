import { useState } from "react";
import { CircleAlert, Pencil, Plus, Trash2 } from "lucide-react";

import { ModeDialog } from "@/components/mode-dialog";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyTitle,
} from "@/components/ui/empty";
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
import type { ModeApp, ModeView } from "@/lib/modes";
import { CLAUDE_MODELS, EFFORT_LABELS } from "@/lib/refine";
import type { SettingsPaneId } from "@/lib/settings-panes";
import { useModes, useRefineState } from "@/lib/use-modes";

// Select value for an app that follows the selected mode.
const NO_DEFAULT = "__none__";

interface ModesPaneProps {
  // Switch to another settings pane: the Dictation mode's instructions
  // and the Refine switch live in the Refine pane.
  onOpenPane?: (pane: SettingsPaneId) => void;
}

// Modes pane (TASK-96): the user's own modes for the Refine step and
// which apps get which mode by default.
//
// A thin view over Rust (`modes_*` commands): what a new mode's id
// is, what a delete takes with it and which mode a dictation runs in
// are all decided in core, and this pane shows what comes back.
export function ModesPane({ onOpenPane }: ModesPaneProps) {
  const modes = useModes();
  const refine = useRefineState();
  // The mode dialog: `mode` null is a new mode. Each dialog keeps what
  // it was opened for while it closes, so its text does not change on
  // the way out.
  const [editor, setEditor] = useState<{ open: boolean; mode: ModeView | null }>({
    open: false,
    mode: null,
  });
  const [removal, setRemoval] = useState<{ open: boolean; mode: ModeView | null }>({
    open: false,
    mode: null,
  });

  const state = modes.state;
  const refineOff = refine !== null && !refine.settings.enabled;

  return (
    <FieldGroup className="px-1 py-2">
      <SectionHeader>Modes</SectionHeader>
      <FieldDescription>
        A mode is a set of instructions Refine follows for a dictation:
        write it formally, write it in English, turn it into a prompt.
        Modes only apply while Refine is on.
      </FieldDescription>

      {refineOff && (
        <Alert data-testid="modes-refine-off">
          <CircleAlert />
          <AlertTitle>Refine is off</AlertTitle>
          <AlertDescription>
            <p>Your words are typed as transcribed, whichever mode is selected.</p>
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

      {modes.error && (
        <FieldDescription className="text-destructive" role="alert">
          {modes.error}
        </FieldDescription>
      )}

      {state && (
        <ul className="flex flex-col" aria-label="Modes">
          {state.modes.map((mode) => (
            <ModeRow
              key={mode.id}
              mode={mode}
              selected={mode.id === state.settings.active}
              standardInstructions={refine?.default_instructions ?? ""}
              onEdit={() => {
                if (mode.builtin) onOpenPane?.("refine");
                else setEditor({ open: true, mode });
              }}
              onDelete={() => setRemoval({ open: true, mode })}
            />
          ))}
        </ul>
      )}

      <Button
        variant="outline"
        className="self-start"
        disabled={!state}
        data-testid="mode-new"
        onClick={() => setEditor({ open: true, mode: null })}
      >
        <Plus data-icon="inline-start" />
        New mode
      </Button>

      <Separator />

      <SectionHeader>Default mode per app</SectionHeader>
      <FieldDescription>
        When you dictate into one of these apps, its mode is used instead
        of the selected one.
      </FieldDescription>

      {state && state.apps.length === 0 && (
        <Empty className="border" data-testid="modes-apps-empty">
          <EmptyHeader>
            <EmptyTitle>No apps yet</EmptyTitle>
            <EmptyDescription>
              Apps appear here once your history has dictations made in
              them. History is off until you turn it on in General. To set
              a default without it, run{" "}
              <code className="font-mono text-xs">openwhisper modes app set</code>{" "}
              with the id of the app.
            </EmptyDescription>
          </EmptyHeader>
        </Empty>
      )}
      {state?.apps.map((app) => (
        <AppRow
          key={app.id}
          app={app}
          modes={state.modes}
          modeId={state.settings.app_defaults.find((d) => d.app_id === app.id)?.mode ?? null}
          onChange={(modeId) => {
            if (modeId === null) modes.clearAppDefault(app.id);
            else modes.setAppDefault({ app, modeId });
          }}
        />
      ))}

      <ModeDialog
        open={editor.open}
        onOpenChange={(open) => setEditor((d) => ({ ...d, open }))}
        mode={editor.mode}
        backend={refine?.settings.backend ?? "claude-cli"}
        onSave={(draft) => modes.save(editor.mode?.id ?? null, draft)}
      />

      <AlertDialog
        open={removal.open}
        onOpenChange={(open) => setRemoval((d) => ({ ...d, open }))}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete the mode “{removal.mode?.name}”?</AlertDialogTitle>
            <AlertDialogDescription>
              Apps that use it by default go back to the selected mode. If
              it is the selected mode, Dictation is selected instead.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              data-testid="mode-delete-confirm"
              onClick={() => {
                if (removal.mode) modes.remove(removal.mode.id);
                setRemoval((d) => ({ ...d, open: false }));
              }}
            >
              Delete mode
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </FieldGroup>
  );
}

// The model and effort a mode has of its own, in words; "" when it
// follows the Refine settings for both.
function ownSettings(mode: ModeView): string {
  const parts: string[] = [];
  if (mode.model) {
    const known = CLAUDE_MODELS.find((m) => m.value === mode.model);
    parts.push(`Model: ${known ? known.label : mode.model}`);
  }
  if (mode.effort) parts.push(`Effort: ${EFFORT_LABELS[mode.effort]}`);
  return parts.join(" · ");
}

function ModeRow({
  mode,
  selected,
  standardInstructions,
  onEdit,
  onDelete,
}: {
  mode: ModeView;
  selected: boolean;
  // What Dictation does when the user wrote no Refine instructions.
  standardInstructions: string;
  onEdit: () => void;
  onDelete: () => void;
}) {
  // One line; the row cuts it off where it runs out of room.
  const written = mode.instructions.replace(/\s+/g, " ").trim();
  const excerpt =
    written || (mode.builtin ? standardInstructions : "Cleans up like Dictation.");
  const own = ownSettings(mode);

  return (
    <li
      className="ow-hover-host flex items-center gap-2 border-b py-2 last:border-b-0"
      data-testid="mode-row"
      data-mode={mode.id}
    >
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <div className="flex items-center gap-2">
          <span className="truncate text-sm font-medium">{mode.name}</span>
          {mode.builtin && <Badge variant="secondary">Built in</Badge>}
          {selected && (
            <Badge variant="outline" data-testid="mode-row-selected">
              Selected
            </Badge>
          )}
        </div>
        <span className="truncate text-xs text-muted-foreground" data-testid="mode-row-excerpt">
          {excerpt}
        </span>
        {own && (
          <span className="truncate text-xs text-muted-foreground" data-testid="mode-row-own">
            {own}
          </span>
        )}
      </div>
      <div className="ow-hover-reveal flex shrink-0 gap-1">
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label={
            mode.builtin ? "Edit the Dictation instructions in Refine" : `Edit ${mode.name}`
          }
          onClick={onEdit}
        >
          <Pencil />
        </Button>
        {!mode.builtin && (
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={`Delete ${mode.name}`}
            onClick={onDelete}
          >
            <Trash2 />
          </Button>
        )}
      </div>
    </li>
  );
}

function AppRow({
  app,
  modes,
  modeId,
  onChange,
}: {
  app: ModeApp;
  modes: ModeView[];
  // The app's default mode; null when it has none.
  modeId: string | null;
  onChange: (modeId: string | null) => void;
}) {
  const items = [
    { value: NO_DEFAULT, label: "No default" },
    ...modes.map((m) => ({ value: m.id, label: m.name })),
  ];
  return (
    <Field orientation="horizontal" data-testid="mode-app-row" data-app={app.id}>
      <FieldContent>
        <FieldLabel>{app.name}</FieldLabel>
        {app.id !== app.name && (
          <FieldDescription className="break-all font-mono text-xs">{app.id}</FieldDescription>
        )}
      </FieldContent>
      <Select
        items={items}
        value={modeId ?? NO_DEFAULT}
        onValueChange={(v) => {
          if (v === NO_DEFAULT) onChange(null);
          else if (typeof v === "string" && v) onChange(v);
        }}
      >
        <SelectTrigger className="w-[180px]" aria-label={`Default mode for ${app.name}`}>
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

function SectionHeader({ children }: { children: React.ReactNode }) {
  return (
    <h3 className="font-mono text-xs uppercase tracking-wider text-muted-foreground">
      {children}
    </h3>
  );
}
