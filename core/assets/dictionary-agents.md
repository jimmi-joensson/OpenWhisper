# OpenWhisper dictionary: instructions for AI agents

The person you are working with dictates with OpenWhisper, a local
speech-to-text app. Speech recognition gets names, brands and jargon
wrong, so their messages can contain misheard words. `dictionary.json`
in this folder lists the words OpenWhisper should spell their way and
how each one was misheard before. OpenWhisper writes this AGENTS.md
and replaces it when the app updates, so don't edit it.

## Read

- Read `dictionary.json` before interpreting a dictated message. A word
  under `heard_as` that still shows up in a message most likely means
  its `term`.
- OpenWhisper already fixes the words listed here before the text
  reaches you. What you still see wrong is what the dictionary is
  missing.

## Update

When you had to work out a misheard word from context (for example
"Immanuel" meaning the colleague Emanoil, or "pay prof" meaning
PayProff), add it so the next dictation comes out right:

- Put the correct spelling in `term` and what you saw in `heard_as`.
- If the term exists, add to its `heard_as` instead of adding the term
  again (matching ignores case).
- A term without `heard_as` still fixes its own casing ("payproff" to
  "PayProff") and its split form ("pay proff"), so add the term alone
  when only those were wrong.
- Never put a common word in `heard_as` ("heap", "yep", "proof"): it is
  replaced everywhere it appears, in every transcript. Ask the person
  instead.
- Keep the file valid JSON (format below). OpenWhisper re-reads it
  before every dictation, so no restart is needed. A file that fails to
  parse is ignored until it is fixed, so check it after editing.
- Tell the person in one line what you added.

```json
{
  "terms": [
    { "term": "PayProff", "heard_as": ["pay prof", "payprof"] },
    { "term": "Emanoil", "heard_as": ["Immanuel", "Emanuel"] },
    { "term": "HeroDesk" }
  ]
}
```

## Matching rules

- Whole words, case-insensitive. A space in a pattern also matches a
  hyphen or no space at all, so "pay proof" covers "pay-proof" and
  "payproof": don't add those variants separately.
- The longest match wins, and a replacement is never matched again.
- The person can also edit the list in OpenWhisper under Settings >
  Dictionary.

## Dictation history (only if the person switched it on)

OpenWhisper can keep what the person dictated. It is off by default.
When it is on, `history/` in this folder holds one Markdown file per
day, for example `history/2026/2026-11-10.md`. The person may have
moved it: see `history.folder` in `settings.json`.

- Each dictation is a `## HH:MM:SS · App` heading (the app it was
  dictated into), a comment line with its id, then the text. A
  `> Before Refine:` quote is the text before the cleanup model
  changed it.
- To find something they said, search the folder, for example
  `rg -i "release plan" history/`. A day's file is named after its
  date, so "what did I say this morning" is today's file.
- These are private notes. Read them when the person asks you to look
  something up; don't quote them elsewhere and don't edit them unless
  asked.

## Where the file lives

| OS | Folder |
| --- | --- |
| macOS | `~/Library/Application Support/com.openwhisper.app/` |
| Windows | `%APPDATA%\com.openwhisper.app\` |
| Linux | `~/.config/com.openwhisper.app/` |

Development builds use `com.openwhisper.app.dev` instead.
