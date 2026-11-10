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
import { RETENTION_OPTIONS, retentionLabel } from "@/lib/history";
import { useHistorySettings } from "@/lib/use-history";

// Settings → General → History (TASK-95): the switch, how long text
// is kept, and a plain statement of what is stored and where. The
// list itself, with delete and clear, lives on Home.
export function HistorySettingsSection() {
  const { settings, database, error, update } = useHistorySettings();
  const days = settings?.retention_days ?? 30;
  // A retention set from the CLI may not be one of the four choices.
  const items = RETENTION_OPTIONS.some((o) => o.value === String(days))
    ? RETENTION_OPTIONS
    : [...RETENTION_OPTIONS, { value: String(days), label: retentionLabel(days) }];

  return (
    <>
      <Field orientation="horizontal">
        <FieldContent>
          <FieldLabel htmlFor="history-enabled">Keep transcript history</FieldLabel>
          <FieldDescription>
            Saves the text of each dictation, the text before Refine changed
            it, and which app it went into. It stays on this computer and is
            shown on Home. Off by default; switching it off stops saving and
            leaves what is already stored.
          </FieldDescription>
        </FieldContent>
        <Switch
          id="history-enabled"
          data-testid="history-enabled-switch"
          checked={settings?.enabled ?? false}
          disabled={settings === null}
          onCheckedChange={(enabled) => update({ enabled })}
        />
      </Field>

      <Field orientation="horizontal">
        <FieldContent>
          <FieldLabel>Keep text for</FieldLabel>
          <FieldDescription>
            Older text is removed automatically, also while history is off.
            Word counts and Time Saved stay. Single dictations can be deleted
            on Home.
          </FieldDescription>
        </FieldContent>
        <Select
          items={items}
          value={String(days)}
          disabled={settings === null}
          onValueChange={(v) => {
            const next = Number.parseInt(String(v), 10);
            if (Number.isFinite(next) && next >= 0) {
              update({ retention_days: next });
            }
          }}
        >
          <SelectTrigger
            className="w-[140px]"
            aria-label="Keep text for"
            data-testid="history-retention"
          >
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

      {database && (
        <p className="text-xs text-muted-foreground" data-testid="history-location">
          Stored in{" "}
          <span className="ow-selectable font-mono break-all">{database}</span>
        </p>
      )}
      {error && (
        <p className="text-xs text-destructive" role="alert">
          Could not save the history setting: {error}
        </p>
      )}
    </>
  );
}
