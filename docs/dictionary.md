# Dictionary

Parakeet has no hot-word biasing, so OpenWhisper fixes names, brands and
jargon after recognition: `core::transcript::process` runs every
transcript through the user's dictionary (`core/src/dictionary.rs`).

## Files

Both live in the app's config dir, next to `settings.json`
(`com.openwhisper.app`, or `com.openwhisper.app.dev` for the dev build):

| File | Written by | Purpose |
| --- | --- | --- |
| `dictionary.json` | The user, the Settings > Dictionary pane, AI agents | The terms and how each was misheard |
| `AGENTS.md` | The app, at startup | How an AI agent reads and updates the dictionary |

`dictionary.json` is a plain file on purpose: people and agents edit it
outside the app. Core stats it before every dictation and re-reads it
when its modification time or size changed, so an edit applies without a
restart. A file that fails to parse keeps the last good dictionary.

To let the dev build use the release build's dictionary, link its file:

```sh
cd "$HOME/Library/Application Support"
ln -sfn "$PWD/com.openwhisper.app/dictionary.json" com.openwhisper.app.dev/dictionary.json
```

Saves write through the link (`save` resolves it) and the reload stats
the target, so both apps see every change. Remove the link to test a
format change in the dev build on its own file.

`AGENTS.md` is `core/assets/dictionary-agents.md`, compiled in with
`include_str!` and rewritten whenever the installed text differs, so the
instructions always match the installed format. Edit the asset, never
the copy in the config dir. It holds the format and matching rules for
agents; this page doesn't repeat them.

## Pointing an agent at it

`AGENTS.md` is the cross-tool convention, but agents only read it from
the folder they work in. A user points their assistant at the app's
copy from the assistant's global instructions:

- Claude Code (`~/.claude/CLAUDE.md`): `@`-imports load files at session
  start. Import paths can't contain spaces, so link the files into
  `~/.claude` first:

  ```sh
  dir="$HOME/Library/Application Support/com.openwhisper.app"
  ln -sfn "$dir/dictionary.json" ~/.claude/openwhisper-dictionary.json
  ln -sfn "$dir/AGENTS.md" ~/.claude/openwhisper-AGENTS.md
  ```

  ```md
  @openwhisper-AGENTS.md
  @openwhisper-dictionary.json
  ```

- Codex (`~/.codex/AGENTS.md`) has no imports: name the absolute path
  and ask it to read the file before interpreting a dictated message.

Only assistants that can read local files can use it; ChatGPT on the web
cannot.

## Built-in terms

`BUILTIN_TERMS` in `core/src/dictionary.rs` ships with the app
(currently "OpenWhisper"). User terms override them.
