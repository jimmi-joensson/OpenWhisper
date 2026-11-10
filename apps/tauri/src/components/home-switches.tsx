import { Settings2 } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldLabel,
} from "@/components/ui/field";
import type { HistorySettings } from "@/lib/history";
import { useRefineEnabled } from "@/lib/use-refine-enabled";

interface HomeSwitchesProps {
  // Owned by HomePane, which also feeds the history list from it.
  history: HistorySettings | null;
  onHistoryChange: (enabled: boolean) => void;
  // Jump to the settings behind a card: the Refine pane, or the
  // History section of General.
  onOpenSettings?: (target: "refine" | "history") => void;
}

// Quick switches on Home (TASK-100): turn Refine and History on and
// off without opening Settings. Both write the same settings blocks
// as their Settings panes, and each card has a shortcut to those
// settings. Each description says where the text goes, because
// flipping either one changes that.
export function HomeSwitches({ history, onHistoryChange, onOpenSettings }: HomeSwitchesProps) {
  const refine = useRefineEnabled();

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
    </div>
  );
}
