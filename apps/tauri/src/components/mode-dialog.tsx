import { useState } from "react";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
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
import { Textarea } from "@/components/ui/textarea";
import type { ModeDraft, ModeView } from "@/lib/modes";
import {
  BACKEND_EFFORTS,
  CLAUDE_MODELS,
  EFFORT_LABELS,
  type RefineBackend,
  type RefineEffort,
} from "@/lib/refine";

// Select values that are not a model or an effort.
const SAME_AS_REFINE = "__same__";
const OTHER_MODEL = "__other__";

// Longest name core keeps (`MODE_NAME_MAX_CHARS`).
const NAME_MAX_CHARS = 40;

interface ModeDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  // The mode being edited; null for a new one.
  mode: ModeView | null;
  // The Refine backend in use: it decides which models and effort
  // levels a mode can ask for.
  backend: RefineBackend;
  // Resolves to what went wrong, or null when the mode is saved.
  onSave: (draft: ModeDraft) => Promise<string | null>;
}

// New mode and Edit mode: a name, the instructions, and optionally a
// model and effort of the mode's own. The form lives in a child that
// mounts with the dialog, so every opening starts from the mode as it
// is stored.
export function ModeDialog({ open, onOpenChange, mode, backend, onSave }: ModeDialogProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md" data-testid="mode-dialog">
        <ModeForm
          key={mode?.id ?? ""}
          mode={mode}
          backend={backend}
          onSave={onSave}
          onDone={() => onOpenChange(false)}
        />
      </DialogContent>
    </Dialog>
  );
}

function ModeForm({
  mode,
  backend,
  onSave,
  onDone,
}: Pick<ModeDialogProps, "mode" | "backend" | "onSave"> & { onDone: () => void }) {
  // Named models exist for Claude only; an OpenAI-compatible server
  // takes whatever name it is given.
  const models: readonly { value: string; label: string }[] =
    backend === "claude-cli" ? CLAUDE_MODELS : [];
  const storedModel = mode?.model ?? null;
  const namedModel = storedModel !== null && models.some((m) => m.value === storedModel);

  const [name, setName] = useState(mode?.name ?? "");
  const [instructions, setInstructions] = useState(mode?.instructions ?? "");
  const [modelChoice, setModelChoice] = useState(
    storedModel === null ? SAME_AS_REFINE : namedModel ? storedModel : OTHER_MODEL,
  );
  const [otherModel, setOtherModel] = useState(namedModel ? "" : (storedModel ?? ""));
  const [effort, setEffort] = useState<RefineEffort | null>(mode?.effort ?? null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const modelItems = [
    { value: SAME_AS_REFINE, label: "Same as Refine" },
    ...models,
    { value: OTHER_MODEL, label: "Other…" },
  ];
  // A level the mode already has stays on offer, also when the
  // backend in use does not take it.
  const levels = BACKEND_EFFORTS[backend];
  const storedEffort = mode?.effort ?? null;
  const offered =
    storedEffort && !levels.includes(storedEffort) ? [storedEffort, ...levels] : levels;
  const effortItems = [
    { value: SAME_AS_REFINE, label: "Same as Refine" },
    ...offered.map((level) => ({ value: level as string, label: EFFORT_LABELS[level] })),
  ];

  const submit = async () => {
    if (!name.trim() || saving) return;
    setSaving(true);
    setError(null);
    const model =
      modelChoice === SAME_AS_REFINE
        ? null
        : modelChoice === OTHER_MODEL
          ? otherModel.trim() || null
          : modelChoice;
    const problem = await onSave({ name, instructions, model, effort });
    setSaving(false);
    if (problem) setError(problem);
    else onDone();
  };

  return (
    <form
      className="grid gap-4"
      onSubmit={(e) => {
        e.preventDefault();
        void submit();
      }}
    >
      <DialogHeader>
        <DialogTitle>{mode ? "Edit mode" : "New mode"}</DialogTitle>
        <DialogDescription>
          Refine follows these instructions for every dictation in this mode.
        </DialogDescription>
      </DialogHeader>

      <FieldGroup>
        <Field>
          <FieldLabel htmlFor="mode-name">Name</FieldLabel>
          <Input
            id="mode-name"
            placeholder="Formal"
            value={name}
            maxLength={NAME_MAX_CHARS}
            autoComplete="off"
            onChange={(e) => setName(e.target.value)}
          />
        </Field>
        <Field>
          <FieldLabel htmlFor="mode-instructions">Instructions</FieldLabel>
          <Textarea
            id="mode-instructions"
            placeholder="Rewrite what I say in a formal tone."
            value={instructions}
            rows={4}
            onChange={(e) => setInstructions(e.target.value)}
          />
          <FieldDescription>
            What Refine should do with what you say. Left empty, the mode
            cleans up like Dictation.
          </FieldDescription>
        </Field>
        <Field orientation="horizontal">
          <FieldContent>
            <FieldLabel>Model</FieldLabel>
          </FieldContent>
          <Select
            items={modelItems}
            value={modelChoice}
            onValueChange={(v) => {
              if (typeof v === "string" && v) setModelChoice(v);
            }}
          >
            <SelectTrigger className="w-[180px]" aria-label="Model">
              <SelectValue />
            </SelectTrigger>
            <SelectContent alignItemWithTrigger={false} align="end">
              <SelectGroup>
                {modelItems.map((item) => (
                  <SelectItem key={item.value} value={item.value}>
                    {item.label}
                  </SelectItem>
                ))}
              </SelectGroup>
            </SelectContent>
          </Select>
        </Field>
        {modelChoice === OTHER_MODEL && (
          <Field>
            <FieldLabel htmlFor="mode-model-name">Model name</FieldLabel>
            <Input
              id="mode-model-name"
              placeholder="A model name your Refine setup accepts"
              value={otherModel}
              spellCheck={false}
              autoCapitalize="off"
              autoCorrect="off"
              onChange={(e) => setOtherModel(e.target.value)}
            />
          </Field>
        )}
        <Field orientation="horizontal">
          <FieldContent>
            <FieldLabel>Effort</FieldLabel>
          </FieldContent>
          <Select
            items={effortItems}
            value={effort ?? SAME_AS_REFINE}
            onValueChange={(v) => {
              if (v === SAME_AS_REFINE) setEffort(null);
              else if (typeof v === "string" && v in EFFORT_LABELS) setEffort(v as RefineEffort);
            }}
          >
            <SelectTrigger className="w-[180px]" aria-label="Effort">
              <SelectValue />
            </SelectTrigger>
            <SelectContent alignItemWithTrigger={false} align="end">
              <SelectGroup>
                {effortItems.map((item) => (
                  <SelectItem key={item.value} value={item.value}>
                    {item.label}
                  </SelectItem>
                ))}
              </SelectGroup>
            </SelectContent>
          </Select>
        </Field>
      </FieldGroup>

      {error && (
        <FieldDescription className="text-destructive" role="alert">
          {error}
        </FieldDescription>
      )}

      <DialogFooter>
        <DialogClose render={<Button type="button" variant="outline" />}>Cancel</DialogClose>
        <Button type="submit" disabled={!name.trim() || saving} data-testid="mode-save">
          Save
        </Button>
      </DialogFooter>
    </form>
  );
}
