import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { FolderOpen, Plus, X } from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Separator } from "@/components/ui/separator";

// Mirrors `DictionaryTerm` / `DictionaryState` in
// core/src/dictionary.rs and src-tauri/src/settings/mod.rs.
export type DictionaryTerm = { term: string; heard_as?: string[] };
type DictionaryState = { path: string; terms: DictionaryTerm[] };

// Dictionary pane — words OpenWhisper should spell the user's way.
//
// The file (`dictionary.json`) is the source of truth and is often
// edited outside the app (by hand or by an agent). Core re-reads it on
// every dictation, so this pane only needs a fresh read when it mounts;
// every save sends the whole list and renders what Rust stored (trimmed,
// merged, sorted).
export function DictionaryPane() {
  const [state, setState] = useState<DictionaryState | null>(null);
  const [term, setTerm] = useState("");
  const [heardAs, setHeardAs] = useState("");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    invoke<DictionaryState>("dictionary_get")
      .then(setState)
      .catch((e) => setError(String(e)));
  }, []);

  const save = useCallback(async (terms: DictionaryTerm[]) => {
    try {
      const next = await invoke<DictionaryState>("dictionary_save", { terms });
      setState(next);
      setError(null);
      return true;
    } catch (e) {
      setError(String(e));
      return false;
    }
  }, []);

  const onAdd = async () => {
    const word = term.trim();
    if (!word || !state) return;
    const heard = heardAs
      .split(",")
      .map((s) => s.trim())
      .filter(Boolean);
    // Core merges a term that already exists, so re-adding "PayProff"
    // with a new misspelling extends its list.
    const ok = await save([...state.terms, { term: word, heard_as: heard }]);
    if (ok) {
      setTerm("");
      setHeardAs("");
    }
  };

  const onRemove = (index: number) => {
    if (!state) return;
    void save(state.terms.filter((_, i) => i !== index));
  };

  const onReveal = () => {
    invoke("dictionary_reveal").catch((e) => setError(String(e)));
  };

  return (
    <FieldGroup className="px-1 py-2">
      <SectionHeader>Dictionary</SectionHeader>
      <FieldDescription>
        Names and words OpenWhisper should spell your way. Add what it
        wrote instead under “Heard as” and it gets replaced in every
        transcript.
      </FieldDescription>

      <form
        className="flex items-end gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          void onAdd();
        }}
      >
        <Field className="flex-1">
          <FieldLabel htmlFor="dictionary-term">Word</FieldLabel>
          <Input
            id="dictionary-term"
            placeholder="PayProff"
            value={term}
            onChange={(e) => setTerm(e.target.value)}
          />
        </Field>
        <Field className="flex-[2]">
          <FieldLabel htmlFor="dictionary-heard-as">Heard as</FieldLabel>
          <Input
            id="dictionary-heard-as"
            placeholder="pay prof, pay proof"
            value={heardAs}
            onChange={(e) => setHeardAs(e.target.value)}
          />
        </Field>
        <Button type="submit" disabled={!term.trim() || !state}>
          <Plus data-icon="inline-start" />
          Add
        </Button>
      </form>

      {error && (
        <FieldDescription className="text-destructive" role="alert">
          {error}
        </FieldDescription>
      )}

      <Separator />

      {state && state.terms.length === 0 && (
        <FieldDescription>No words yet.</FieldDescription>
      )}
      {state && state.terms.length > 0 && (
        <ul className="flex flex-col" aria-label="Dictionary words">
          {state.terms.map((t, i) => (
            <li
              key={`${t.term}-${i}`}
              className="flex items-center gap-2 border-b py-2 last:border-b-0"
            >
              <div className="flex min-w-0 flex-1 flex-col">
                <span className="truncate text-sm font-medium">{t.term}</span>
                {t.heard_as && t.heard_as.length > 0 && (
                  <span className="truncate text-xs text-muted-foreground">
                    Heard as {t.heard_as.join(", ")}
                  </span>
                )}
              </div>
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label={`Remove ${t.term}`}
                onClick={() => onRemove(i)}
              >
                <X />
              </Button>
            </li>
          ))}
        </ul>
      )}

      <Separator />

      <Field orientation="horizontal">
        <FieldContent>
          <FieldLabel>Dictionary file</FieldLabel>
          <FieldDescription className="break-all font-mono text-xs">
            {state?.path ?? "…"}
          </FieldDescription>
        </FieldContent>
        <Button variant="outline" onClick={onReveal}>
          <FolderOpen data-icon="inline-start" />
          Show file
        </Button>
      </Field>
    </FieldGroup>
  );
}

function SectionHeader({ children }: { children: React.ReactNode }) {
  return (
    <h3 className="font-mono text-xs uppercase tracking-wider text-muted-foreground">
      {children}
    </h3>
  );
}
