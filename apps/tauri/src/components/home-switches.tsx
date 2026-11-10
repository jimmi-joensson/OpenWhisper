import { Settings2 } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import {
  Field,
  FieldContent,
  FieldDescription,
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
import type { HistorySettings } from "@/lib/history";
import { BUILTIN_MODE_ID } from "@/lib/modes";
import { useModes } from "@/lib/use-modes";
import { useRefineEnabled } from "@/lib/use-refine-enabled";

interface HomeSwitchesProps {
  // Owned by HomePane, which also feeds the history list from it.
  history: HistorySettings | null;
  onHistoryChange: (enabled: boolean) => void;
  // Jump to the settings behind a card: the Refine pane, the History
  // section of General, or the Modes pane.
  onOpenSettings?: (target: "refine" | "history" | "modes") => void;
}

// Quick switches on Home (TASK-100): turn Refine and History on and
// off without opening Settings. Both write the same settings blocks
// as their Settings panes, and each card has a shortcut to those
// settings. Each description says where the text goes, because
// flipping either one changes that.
//
// The Mode card (TASK-96) picks what Refine does with a dictation. A
// select, not a toggle group: the list is the user's own and can
// grow. It is disabled while Refine is off, since no mode applies
// then, and says so.
export function HomeSwitches({ history, onHistoryChange, onOpenSettings }: HomeSwitchesProps) {
  const refine = useRefineEnabled();
  const modes = useModes();
  const refineOn = refine.enabled === true;
  const modeItems = (modes.state?.modes ?? []).map((m) => ({ value: m.id, label: m.name }));

  return (
    <div className="ow-home-switches" data-testid="home-switches">
      <Field orientation="horizontal" className="ow-home-switch">
        <FieldContent>
          <FieldLabel htmlFor="home-refine">Refine</FieldLabel>
          <FieldDescription>
            A language model tidies each dictation. The text leaves this
            computer.
          </FieldDescription>
        </FieldContent>
        {onOpenSettings && (
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label="Refine settings"
            data-testid="home-refine-settings"
            onClick={() => onOpenSettings("refine")}
          >
            <Settings2 />
          </Button>
        )}
        <Switch
          id="home-refine"
          data-testid="home-refine-switch"
          checked={refine.enabled ?? false}
          disabled={refine.enabled === null}
          onCheckedChange={(next) => void refine.setEnabled(next)}
        />
      </Field>
      <Field orientation="horizontal" className="ow-home-switch">
        <FieldContent>
          <FieldLabel htmlFor="home-history">History</FieldLabel>
          <FieldDescription>
            Keeps the text of your dictations, on this computer only.
          </FieldDescription>
        </FieldContent>
        {onOpenSettings && (
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label="History settings"
            data-testid="home-history-settings"
            onClick={() => onOpenSettings("history")}
          >
            <Settings2 />
          </Button>
        )}
        <Switch
          id="home-history"
          data-testid="home-history-switch"
          checked={history?.enabled ?? false}
          disabled={history === null}
          onCheckedChange={onHistoryChange}
        />
      </Field>
      <Field
        orientation="horizontal"
        className="ow-home-switch ow-home-switch--wide"
        data-disabled={refineOn ? undefined : "true"}
      >
        <FieldContent>
          <FieldLabel>Mode</FieldLabel>
          <FieldDescription data-testid="home-mode-note">
            {refineOn
              ? "What Refine does with each dictation."
              : "Modes apply when Refine is on."}
          </FieldDescription>
        </FieldContent>
        {onOpenSettings && (
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label="Mode settings"
            data-testid="home-mode-settings"
            onClick={() => onOpenSettings("modes")}
          >
            <Settings2 />
          </Button>
        )}
        <Select
          items={modeItems}
          value={modes.state?.settings.active ?? BUILTIN_MODE_ID}
          disabled={!refineOn || !modes.state}
          onValueChange={(v) => {
            if (typeof v === "string" && v) modes.select(v);
          }}
        >
          <SelectTrigger className="w-[180px]" aria-label="Mode" data-testid="home-mode-select">
            <SelectValue />
          </SelectTrigger>
          <SelectContent alignItemWithTrigger={false} align="end">
            <SelectGroup>
              {modeItems.map((item) => (
                <SelectItem key={item.value} value={item.value}>
                  {item.label}
                </SelectItem>
              ))}
            </SelectGroup>
          </SelectContent>
        </Select>
      </Field>
    </div>
  );
}
