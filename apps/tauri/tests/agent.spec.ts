import {
  emitAskTick,
  emitModesChanged,
  expect,
  pillTest,
  test,
  waitForAskTickListener,
  waitForModesListener,
  type MockModesChanged,
} from "./fixtures/tauri-shim";

type Page = import("@playwright/test").Page;

// Agent (TASK-99.3): the Agent pane. The switch with its plain
// statement, drafts only, the connections with a refresh, their tools
// by kind, and the app table. A connection of the person's own is not
// added here: that is done in a terminal.
//
// Nothing here starts Claude Code or reaches a connection: the shim
// keeps an Agent block in memory that behaves like Rust. Every
// connection, tool, app and person is made up.

interface ToolSeed {
  name: string;
  title?: string;
  kind: "read" | "draft" | "act";
  // Core's sentence when the tool cannot be used at all.
  problem?: string;
}

// Whether an ask can use a connection is not seeded: the shim works
// it out from the state, as Rust does from what was looked up.
interface ConnectionSeed {
  name: string;
  kind?: "claude" | "custom";
  enabled?: boolean;
  allowed_tools?: string[];
  tools?: ToolSeed[];
  state?: string;
  via?: "command" | "url";
}

interface AgentSeed {
  enabled?: boolean;
  drafts_only?: boolean;
  connections?: ConnectionSeed[];
  app_connections?: { app_id: string; app_name: string; connection: string }[] | null;
}

interface Seed {
  // Agent is on unless a test says otherwise: the lists are only in
  // use then.
  agent?: AgentSeed;
  // Ask is on unless a test says otherwise.
  askEnabled?: boolean;
  // The backend Refine is set up with.
  backend?: "claude-cli" | "openai-compatible";
  listing?: { connections?: ConnectionSeed[]; error?: string; custom_error?: string };
  refreshMs?: number;
  running?: { id: string; name: string }[];
}

const TEAMCHAT_TOOLS: ToolSeed[] = [
  { name: "read_thread", title: "Read thread", kind: "read" },
  { name: "search_messages", title: "Search messages", kind: "read" },
  { name: "create_draft", title: "Create draft", kind: "draft" },
  { name: "send_message", title: "Send message", kind: "act" },
  { name: "delete_message", title: "Delete message", kind: "act" },
];

const TEAMCHAT: ConnectionSeed = {
  name: "teamchat",
  state: "connected",
  tools: TEAMCHAT_TOOLS,
};

const TRACKER: ConnectionSeed = {
  name: "tracker",
  state: "needs-auth",
  tools: [{ name: "list_issues", kind: "read" }],
};

// One of the person's own, added in a terminal.
const NOTES: ConnectionSeed = {
  name: "notes",
  kind: "custom",
  via: "command",
  tools: [{ name: "list_notes", kind: "read" }],
};

async function seed(page: Page, given: Seed = {}) {
  await page.addInitScript((s: Seed) => {
    const w = window as unknown as Record<string, unknown>;
    w.__owAskSettings = { enabled: s.askEnabled ?? true };
    if (s.backend) w.__owAskBackend = { backend: s.backend };
    w.__owAgentSettings = { enabled: true, ...(s.agent ?? {}) };
    if (s.listing) w.__owAgentListing = s.listing;
    if (s.refreshMs !== undefined) w.__owAgentRefreshMs = s.refreshMs;
    if (s.running) w.__owRunningApps = s.running;
  }, given);
}

async function openAgent(page: Page, given: Seed = {}) {
  await seed(page, given);
  await page.goto("/");
  await page.waitForSelector('[data-testid="sidebar-item-home"]');
  await page.evaluate(() => window.__owEmit("ow_navigate", "settings"));
  await expect(page.getByRole("heading", { name: "Settings" })).toBeVisible();
  await page.getByRole("tab", { name: "Agent", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Agent", exact: true })).toBeVisible();
  // The first read has answered.
  await expect(page.getByTestId("agent-connections-intro")).toBeVisible();
}

// Read a value the shim recorded on `window`.
function recorded<T>(page: Page, key: string): Promise<T | undefined> {
  return page.evaluate(
    (k) => (window as unknown as Record<string, unknown>)[k] as T | undefined,
    key,
  );
}

// One save: a change of one thing (`AgentPatch` in Rust).
interface Patch {
  enabled?: boolean;
  drafts_only?: boolean;
  connection?: { name: string; enabled?: boolean; tick?: string[]; untick?: string[] };
  app_connections?: { app_id: string; app_name: string; connection: string }[] | null;
}

async function lastSave(page: Page): Promise<Patch | undefined> {
  return (await recorded<Patch[]>(page, "__owAgentSaves"))?.at(-1);
}

// The block as the shim has it stored: what is "on disk".
interface StoredBlock {
  enabled: boolean;
  drafts_only: boolean;
  connections: { name: string; enabled: boolean; allowed_tools: string[] }[];
  app_connections: { app_id: string; app_name: string; connection: string }[] | null;
}

async function onDisk(page: Page): Promise<StoredBlock> {
  const stored = await recorded<StoredBlock>(page, "__owAgentStored");
  if (!stored) throw new Error("nothing stored");
  return stored;
}

async function ticksOnDisk(page: Page, name: string): Promise<string[]> {
  const found = (await onDisk(page)).connections.find((c) => c.name === name);
  if (!found) throw new Error(`no connection ${name}`);
  return found.allowed_tools;
}

// Change the stored block the way the headless CLI does: behind the
// window's back, without telling it.
async function changeElsewhere(page: Page, change: (stored: StoredBlock) => void) {
  await page.evaluate(
    (body) => {
      const w = window as unknown as { __owAgentStored: unknown };
      new Function("stored", `(${body})(stored)`)(w.__owAgentStored);
    },
    change.toString(),
  );
}

// Call a command the way a window does, and say how it ended.
async function call(
  page: Page,
  cmd: string,
  args?: Record<string, unknown>,
): Promise<{ ok: unknown } | { error: string }> {
  return page.evaluate(
    ({ cmd, args }) =>
      (
        window.__TAURI_INTERNALS__ as {
          invoke: (cmd: string, args?: unknown) => Promise<unknown>;
        }
      )
        .invoke(cmd, args)
        .then(
          (ok) => ({ ok }),
          (error) => ({ error: String(error) }),
        ),
    { cmd, args },
  );
}

async function commands(page: Page): Promise<string[]> {
  return ((await recorded<{ cmd: string }[]>(page, "__owAgentCalls")) ?? []).map((c) => c.cmd);
}

const agentSwitch = (page: Page) => page.getByRole("switch", { name: "Agent", exact: true });
const draftsSwitch = (page: Page) => page.getByRole("switch", { name: "Drafts only" });
const row = (page: Page, name: string) =>
  page.locator(`[data-testid="agent-connection"][data-name="${name}"]`);
const use = (page: Page, name: string) =>
  page.getByRole("switch", { name: `Use ${name}`, exact: true });
const tool = (page: Page, connection: string, name: string) =>
  row(page, connection).locator(`[data-testid="agent-tool"][data-tool="${name}"]`);
const box = (page: Page, connection: string, name: string) =>
  tool(page, connection, name).getByRole("checkbox");
const appRow = (page: Page, id: string) =>
  page.locator(`[data-testid="agent-app-row"][data-app="${id}"]`);

async function showTools(page: Page, name: string) {
  await row(page, name).getByRole("button", { name: `Show the tools of ${name}` }).click();
  await expect(row(page, name).getByTestId("agent-tools")).toBeVisible();
}

test.describe("agent pane: the switch", () => {
  test("sits after Ask in the settings sidebar", async ({ page }) => {
    await openAgent(page);
    const tabs = await page.getByRole("tab").allTextContents();
    expect(tabs.slice(-2)).toEqual(["Ask", "Agent"]);
  });

  test("is off by default with drafts only on, and says what it can and cannot do", async ({
    page,
  }) => {
    await openAgent(page, { agent: { enabled: false } });
    await expect(agentSwitch(page)).not.toBeChecked();
    await expect(draftsSwitch(page)).toBeChecked();
    await expect(page.getByTestId("agent-can")).toContainText(
      "It can look things up there and prepare drafts",
    );
    await expect(page.getByTestId("agent-can")).toContainText("only those you switch on below");
    const cannot = page.getByTestId("agent-cannot");
    await expect(cannot).toContainText(
      "It cannot use a connection you have not switched on or a tool you have not ticked.",
    );
    // What a connection can do is what its tools do: nothing is
    // promised about that beyond the three kinds.
    await expect(cannot).toContainText("A connection can do whatever its ticked tools do.");
    await expect(cannot).toContainText("Tools that read or draft run without asking.");
    await expect(cannot).toContainText(
      "Tools that act ask you every time, and cannot run at all while Drafts only is on.",
    );
    await expect(cannot).toContainText(
      "Claude Code's own tools for files and the shell are switched off in an ask.",
    );
    // Nothing is promised about what a connection's own tools reach.
    await expect(page.getByText("It cannot open your files or run commands")).toHaveCount(0);
    await expect(page.getByTestId("agent-privacy")).toContainText(
      "What a tool returns is sent to Claude together with your question.",
    );
    // Off: the lists wait for the switch, and say so.
    await expect(page.getByTestId("agent-off-note")).toBeVisible();
    await expect(page.getByTestId("agent-refresh")).toBeDisabled();
    // Nothing was looked up or saved by opening the pane.
    const called = await commands(page);
    expect(called).toContain("agent_get");
    expect(called).not.toContain("agent_refresh");
    expect(called).not.toContain("agent_set");
  });

  test("switching it on saves the choice and looks the connections up", async ({ page }) => {
    await openAgent(page, {
      agent: { enabled: false },
      listing: { connections: [TEAMCHAT, TRACKER] },
    });
    await expect(page.getByTestId("agent-connections-empty")).toBeVisible();
    await agentSwitch(page).click();
    await expect(agentSwitch(page)).toBeChecked();
    // The one switch, and nothing else: drafts only is not sent.
    expect(await lastSave(page)).toEqual({ enabled: true });
    expect(await onDisk(page)).toMatchObject({ enabled: true, drafts_only: true });
    // The lookup that follows brings the connections, all switched
    // off with nothing ticked.
    await expect(row(page, "teamchat")).toBeVisible();
    await expect(row(page, "tracker")).toBeVisible();
    await expect(use(page, "teamchat")).not.toBeChecked();
    await expect(row(page, "teamchat").getByTestId("agent-connection-summary")).toHaveText(
      "0 of 5 tools ticked",
    );
    await expect(page.getByTestId("agent-off-note")).toHaveCount(0);
    await expect(page.getByTestId("agent-refresh")).toBeEnabled();
    await expect(page.getByTestId("agent-status")).toHaveText("No connection is enabled yet.");
  });

  test("drafts only can be switched off, and the pane says what that means", async ({ page }) => {
    await openAgent(page, { agent: { connections: [TEAMCHAT] } });
    const note = page.getByTestId("agent-drafts-only-note");
    await expect(note).toContainText("Switched off, a ticked tool that acts can run");
    await draftsSwitch(page).click();
    await expect(draftsSwitch(page)).not.toBeChecked();
    // Switching it off takes saying so: an explicit false.
    expect(await lastSave(page)).toEqual({ drafts_only: false });
    await expect(note).toContainText("only after you allow it in the answer panel, every time");
    await expect(note).toContainText("No answer within 120 seconds counts as no.");
  });

  test("says plainly when Ask is off, and leads to the Ask settings", async ({ page }) => {
    await openAgent(page, { askEnabled: false, agent: { connections: [TEAMCHAT] } });
    const note = page.getByTestId("agent-needs-ask");
    await expect(note).toContainText("Ask is switched off");
    await expect(note).toContainText("Agent only works inside an ask.");
    await expect(page.getByTestId("agent-status")).toHaveCount(0);
    await note.getByRole("button", { name: "Open Ask settings" }).click();
    await expect(page.getByRole("heading", { name: "Ask", exact: true })).toBeVisible();
  });

  test("says plainly when the backend cannot use connections, and leads to Refine", async ({
    page,
  }) => {
    await openAgent(page, { backend: "openai-compatible", agent: { connections: [TEAMCHAT] } });
    const note = page.getByTestId("agent-needs-claude");
    await expect(note).toContainText("Agent needs the Claude backend");
    await expect(note).toContainText("an OpenAI-compatible server, which cannot use connections");
    await expect(page.getByTestId("agent-needs-ask")).toHaveCount(0);
    await note.getByRole("button", { name: "Open Refine settings" }).click();
    await expect(page.getByRole("tab", { name: "Refine", exact: true })).toHaveAttribute(
      "aria-selected",
      "true",
    );
  });

  test("a save that fails is undone and shown", async ({ page }) => {
    await openAgent(page, { agent: { connections: [TEAMCHAT] } });
    await page.evaluate(() => {
      (window as unknown as Record<string, unknown>).__owAgentSetError =
        "write settings: permission denied";
    });
    await draftsSwitch(page).click();
    await expect(page.getByRole("alert").filter({ hasText: "permission denied" })).toBeVisible();
    await expect(draftsSwitch(page)).toBeChecked();
  });
});

test.describe("agent pane: connections", () => {
  test("refresh shows that it is looking, then what it found", async ({ page }) => {
    await openAgent(page, {
      listing: { connections: [TEAMCHAT, TRACKER] },
      refreshMs: 400,
    });
    await expect(page.getByTestId("agent-listing-state")).toHaveText(
      "Not looked up since the app started.",
    );
    const refresh = page.getByTestId("agent-refresh");
    await expect(refresh).toHaveText("Refresh");
    await refresh.click();
    await expect(refresh).toHaveText("Looking up");
    await expect(refresh).toBeDisabled();
    await expect(page.getByTestId("agent-listing-state")).toHaveText(
      "Asking Claude Code which connections it has.",
    );
    // Over: the connections with their state, nothing switched on.
    await expect(row(page, "teamchat")).toBeVisible();
    await expect(refresh).toHaveText("Refresh");
    await expect(refresh).toBeEnabled();
    await expect(page.getByTestId("agent-listing-state")).toContainText("0 of 2 switched on.");
    await expect(row(page, "teamchat").getByTestId("agent-connection-state")).toHaveText(
      "Connected",
    );
    await expect(row(page, "tracker").getByTestId("agent-connection-state")).toHaveText(
      "Needs sign-in",
    );
    await expect(use(page, "teamchat")).not.toBeChecked();
    await expect(use(page, "tracker")).not.toBeChecked();
    expect((await commands(page)).filter((c) => c === "agent_refresh")).toHaveLength(1);
  });

  test("a lookup that fails says why and changes nothing", async ({ page }) => {
    await openAgent(page, {
      agent: { connections: [{ ...TEAMCHAT, enabled: true, allowed_tools: ["read_thread"] }] },
      listing: { error: "Claude Code is not installed on this computer." },
    });
    await page.getByTestId("agent-refresh").click();
    const failure = page.getByTestId("agent-listing-error");
    await expect(failure).toContainText("The connections could not be looked up");
    await expect(failure).toContainText("Claude Code is not installed on this computer.");
    await expect(use(page, "teamchat")).toBeChecked();
    await expect(row(page, "teamchat").getByTestId("agent-connection-summary")).toHaveText(
      "1 of 5 tools ticked",
    );
  });

  test("each state has a word, and a connection that was not looked up says so", async ({
    page,
  }) => {
    await openAgent(page, {
      agent: {
        connections: [
          { name: "alpha", state: "failed" },
          { name: "bravo", state: "pending" },
          { name: "charlie", state: "disabled" },
          { name: "delta", state: "missing", enabled: true },
          { name: "echo", enabled: true, tools: [{ name: "list_notes", kind: "read" }] },
        ],
      },
    });
    const word = (name: string) => row(page, name).getByTestId("agent-connection-state");
    await expect(word("alpha")).toHaveText("Failed");
    await expect(word("bravo")).toHaveText("Still connecting");
    await expect(word("charlie")).toHaveText("Off in Claude Code");
    await expect(word("delta")).toHaveText("Not found");
    await expect(word("echo")).toHaveText("Not looked up yet");
    // Switched on, but the app has not looked it up since it started:
    // an ask could not use it.
    await expect(row(page, "echo").getByTestId("agent-connection-unusable")).toContainText(
      "Press Refresh.",
    );
    // Gone from Claude Code: that is said instead.
    await expect(row(page, "delta").getByTestId("agent-connection-unusable")).toContainText(
      "Claude Code no longer has it.",
    );
    await expect(row(page, "alpha").getByTestId("agent-connection-unusable")).toHaveCount(0);
    await expect(row(page, "alpha").getByTestId("agent-connection-summary")).toHaveText(
      "No tools known",
    );
  });

  test("switching a connection on saves it and opens its tools, collapsed until then", async ({
    page,
  }) => {
    await openAgent(page, { agent: { connections: [TEAMCHAT, TRACKER] } });
    // Tool lists start closed.
    await expect(page.getByTestId("agent-tools")).toHaveCount(0);
    await use(page, "teamchat").click();
    await expect(use(page, "teamchat")).toBeChecked();
    // That one connection's switch, and nothing else.
    expect(await lastSave(page)).toEqual({ connection: { name: "teamchat", enabled: true } });
    // Its tools are showing now, the other connection's are not.
    await expect(row(page, "teamchat").getByTestId("agent-tools")).toBeVisible();
    await expect(row(page, "tracker").getByTestId("agent-tools")).toHaveCount(0);
    await expect(page.getByTestId("agent-status")).toContainText(
      "No tool that can run is ticked.",
    );
    // And it can be closed again.
    await row(page, "teamchat").getByRole("button", { name: "Hide the tools of teamchat" }).click();
    await expect(page.getByTestId("agent-tools")).toHaveCount(0);
  });

  test("tools are grouped by kind and ticking one saves it", async ({ page }) => {
    await openAgent(page, { agent: { connections: [{ ...TEAMCHAT, enabled: true }] } });
    await showTools(page, "teamchat");
    const teamchat = row(page, "teamchat");
    await expect(teamchat.getByTestId("agent-tools-read").getByTestId("agent-tool")).toHaveCount(2);
    await expect(teamchat.getByTestId("agent-tools-draft").getByTestId("agent-tool")).toHaveCount(1);
    await expect(teamchat.getByTestId("agent-tools-act").getByTestId("agent-tool")).toHaveCount(2);
    // A tool shows its title, with its own name next to it.
    await expect(tool(page, "teamchat", "read_thread")).toContainText("Read thread");
    await expect(tool(page, "teamchat", "read_thread")).toContainText("read_thread");

    await box(page, "teamchat", "read_thread").click();
    await expect(box(page, "teamchat", "read_thread")).toBeChecked();
    // The one tick, and nothing else.
    expect(await lastSave(page)).toEqual({
      connection: { name: "teamchat", tick: ["read_thread"] },
    });
    await box(page, "teamchat", "create_draft").click();
    await expect(box(page, "teamchat", "create_draft")).toBeChecked();
    expect(await lastSave(page)).toEqual({
      connection: { name: "teamchat", tick: ["create_draft"] },
    });
    expect(await ticksOnDisk(page, "teamchat")).toEqual(["read_thread", "create_draft"]);
    await expect(teamchat.getByTestId("agent-connection-summary")).toHaveText(
      "2 of 5 tools ticked",
    );
    await expect(page.getByTestId("agent-status")).toHaveText(
      "An ask can use 2 tools on 1 connection. Drafts only: nothing that sends, posts, deletes or changes anything can run.",
    );
    // And off again.
    await box(page, "teamchat", "read_thread").click();
    await expect(box(page, "teamchat", "read_thread")).not.toBeChecked();
    expect(await lastSave(page)).toEqual({
      connection: { name: "teamchat", untick: ["read_thread"] },
    });
    expect(await ticksOnDisk(page, "teamchat")).toEqual(["create_draft"]);
  });

  test("a whole group of read tools can be ticked at once, the tools that act cannot", async ({
    page,
  }) => {
    await openAgent(page, { agent: { connections: [{ ...TEAMCHAT, enabled: true }] } });
    await showTools(page, "teamchat");
    const read = row(page, "teamchat").getByTestId("agent-tools-read");
    await read.getByRole("button", { name: "Tick all" }).click();
    await expect(read.getByRole("button", { name: "Untick all" })).toBeVisible();
    expect(await lastSave(page)).toEqual({
      connection: { name: "teamchat", tick: ["read_thread", "search_messages"] },
    });
    expect(await ticksOnDisk(page, "teamchat")).toEqual(["read_thread", "search_messages"]);
    await read.getByRole("button", { name: "Untick all" }).click();
    await expect(read.getByRole("button", { name: "Tick all" })).toBeVisible();
    expect(await lastSave(page)).toEqual({
      connection: { name: "teamchat", untick: ["read_thread", "search_messages"] },
    });
    expect(await ticksOnDisk(page, "teamchat")).toEqual([]);
    const act = row(page, "teamchat").getByTestId("agent-tools-act");
    await expect(act.getByRole("button", { name: "Tick all" })).toHaveCount(0);
  });

  test("with drafts only, a tool that acts cannot be ticked, and one ticked before can be unticked", async ({
    page,
  }) => {
    await openAgent(page, {
      agent: {
        connections: [{ ...TEAMCHAT, enabled: true, allowed_tools: ["read_thread", "send_message"] }],
      },
    });
    await showTools(page, "teamchat");
    const act = row(page, "teamchat").getByTestId("agent-tools-act");
    await expect(act).toHaveAttribute("data-unavailable", "true");
    await expect(act.getByTestId("agent-act-badge")).toHaveText("Unavailable");
    await expect(act.getByTestId("agent-tools-act-note")).toContainText(
      "Unavailable while Drafts only is on: none of these can run, and none can be ticked. One that is ticked can be unticked.",
    );
    // One that is not ticked cannot be ticked now.
    await expect(box(page, "teamchat", "delete_message")).not.toBeChecked();
    await expect(box(page, "teamchat", "delete_message")).toBeDisabled();
    // The others can.
    await expect(box(page, "teamchat", "read_thread")).toBeEnabled();
    await expect(box(page, "teamchat", "create_draft")).toBeEnabled();
    // The ticked tool that acts does not count as one an ask can use.
    await expect(page.getByTestId("agent-status")).toContainText("An ask can use 1 tool on 1");
    // It was ticked before Drafts only went on. Left ticked it would
    // come alive the day Drafts only is switched off, so it can be
    // taken off.
    await expect(box(page, "teamchat", "send_message")).toBeChecked();
    await expect(box(page, "teamchat", "send_message")).toBeEnabled();
    await box(page, "teamchat", "send_message").click();
    await expect(box(page, "teamchat", "send_message")).not.toBeChecked();
    expect(await lastSave(page)).toEqual({
      connection: { name: "teamchat", untick: ["send_message"] },
    });
    expect(await ticksOnDisk(page, "teamchat")).toEqual(["read_thread"]);
    // And not put back on while Drafts only is on.
    await expect(box(page, "teamchat", "send_message")).toBeDisabled();
    // Rust refuses such a tick whoever sends it, and says why.
    const refused = await call(page, "agent_set", {
      patch: { connection: { name: "teamchat", tick: ["send_message"] } },
    });
    expect(refused).toEqual({
      error:
        '"send_message" sends, posts, deletes or changes something, so it cannot be ticked while Drafts only is on.',
    });
    expect(await ticksOnDisk(page, "teamchat")).toEqual(["read_thread"]);
  });

  test("a tool that cannot be used is shown with the reason and can never be ticked", async ({
    page,
  }) => {
    const NAME =
      "Its name has characters that cannot be passed on safely, so it cannot be used.";
    const TWIN =
      "Another tool of this connection has a name that is spelled the same way, so neither can be used.";
    await openAgent(page, {
      agent: {
        // Also with drafts only off: this has nothing to do with it.
        drafts_only: false,
        connections: [
          {
            ...TEAMCHAT,
            enabled: true,
            tools: [
              ...TEAMCHAT_TOOLS,
              { name: "list,all", kind: "read", problem: NAME },
              { name: "post.note", title: "Post note", kind: "act", problem: TWIN },
              { name: "post_note", title: "Post note", kind: "act", problem: TWIN },
            ],
          },
        ],
      },
    });
    await showTools(page, "teamchat");
    const odd = tool(page, "teamchat", "list,all");
    await expect(odd).toHaveAttribute("data-unusable", "true");
    await expect(odd.getByTestId("agent-tool-problem")).toHaveText(NAME);
    await expect(box(page, "teamchat", "list,all")).toBeDisabled();
    await expect(box(page, "teamchat", "list,all")).not.toBeChecked();
    for (const twin of ["post.note", "post_note"]) {
      await expect(tool(page, "teamchat", twin).getByTestId("agent-tool-problem")).toHaveText(TWIN);
      await expect(box(page, "teamchat", twin)).toBeDisabled();
    }
    // A tool that can be used has no such line.
    await expect(tool(page, "teamchat", "read_thread").getByTestId("agent-tool-problem")).toHaveCount(
      0,
    );
    await expect(box(page, "teamchat", "read_thread")).toBeEnabled();
    // "Tick all" leaves it out.
    const read = row(page, "teamchat").getByTestId("agent-tools-read");
    await read.getByRole("button", { name: "Tick all" }).click();
    await expect(read.getByRole("button", { name: "Untick all" })).toBeVisible();
    expect(await lastSave(page)).toEqual({
      connection: { name: "teamchat", tick: ["read_thread", "search_messages"] },
    });
    await expect(box(page, "teamchat", "list,all")).not.toBeChecked();
    // Rust refuses a tick of it whoever sends it, with the reason.
    expect(
      await call(page, "agent_set", {
        patch: { connection: { name: "teamchat", tick: ["list,all"] } },
      }),
    ).toEqual({ error: `"list,all" cannot be ticked. ${NAME}` });
    expect(await ticksOnDisk(page, "teamchat")).toEqual(["read_thread", "search_messages"]);
  });

  test("with drafts only off, a tool that acts can be ticked and is marked as asking first", async ({
    page,
  }) => {
    await openAgent(page, {
      agent: {
        drafts_only: false,
        connections: [{ ...TEAMCHAT, enabled: true, allowed_tools: ["read_thread"] }],
      },
    });
    await showTools(page, "teamchat");
    const act = row(page, "teamchat").getByTestId("agent-tools-act");
    await expect(act).not.toHaveAttribute("data-unavailable", "true");
    await expect(act.getByTestId("agent-act-badge")).toHaveText("Asks first");
    await expect(act.getByTestId("agent-tools-act-note")).toContainText(
      "Each of these asks you in the answer panel before it runs, every time.",
    );
    await box(page, "teamchat", "send_message").click();
    await expect(box(page, "teamchat", "send_message")).toBeChecked();
    expect(await ticksOnDisk(page, "teamchat")).toEqual(["read_thread", "send_message"]);
    await expect(page.getByTestId("agent-status")).toHaveText(
      "An ask can use 2 tools on 1 connection. 1 of them ask you first, every time.",
    );
    expect(await lastSave(page)).toEqual({
      connection: { name: "teamchat", tick: ["send_message"] },
    });
    // Drafts only back on: it cannot run, stays ticked until it is
    // unticked, and the one that is not ticked is locked.
    await draftsSwitch(page).click();
    await expect(act.getByTestId("agent-act-badge")).toHaveText("Unavailable");
    await expect(box(page, "teamchat", "send_message")).toBeChecked();
    await expect(box(page, "teamchat", "send_message")).toBeEnabled();
    await expect(box(page, "teamchat", "delete_message")).toBeDisabled();
  });

  test("the tools of a connection that is off can be read, not ticked", async ({ page }) => {
    await openAgent(page, { agent: { connections: [TEAMCHAT] } });
    await showTools(page, "teamchat");
    await expect(row(page, "teamchat").getByTestId("agent-tools-off")).toHaveText(
      "Switch the connection on to tick its tools.",
    );
    await expect(box(page, "teamchat", "read_thread")).toBeDisabled();
    await expect(
      row(page, "teamchat").getByTestId("agent-tools-read").getByRole("button", { name: "Tick all" }),
    ).toBeDisabled();
  });

  test("a long list stays usable: closed by default, and the filter finds a tool", async ({
    page,
  }) => {
    // Thirty connections with fifteen tools each.
    const many: ConnectionSeed[] = Array.from({ length: 30 }, (_, i) => ({
      name: `connection ${String(i + 1).padStart(2, "0")}`,
      state: "connected",
      tools: Array.from({ length: 15 }, (_, j) => ({
        name: `list_things_${j + 1}`,
        kind: "read" as const,
      })),
    }));
    many[11].tools = [...(many[11].tools ?? []), { name: "create_reply_draft", kind: "draft" }];
    await openAgent(page, { agent: { connections: many } });
    await expect(page.getByTestId("agent-connection")).toHaveCount(30);
    // 465 tools, none of them on the page until a list is opened.
    await expect(page.getByTestId("agent-tool")).toHaveCount(0);

    const filter = page.getByRole("textbox", { name: "Filter connections and tools" });
    await filter.fill("reply_draft");
    await expect(page.getByTestId("agent-connection")).toHaveCount(1);
    await expect(row(page, "connection 12")).toBeVisible();
    // Opened, only the tool that matches is listed.
    await showTools(page, "connection 12");
    await expect(page.getByTestId("agent-tool")).toHaveCount(1);
    await expect(tool(page, "connection 12", "create_reply_draft")).toBeVisible();

    // By the name of a connection: all of its tools.
    await filter.fill("Connection 07");
    await expect(page.getByTestId("agent-connection")).toHaveCount(1);
    await showTools(page, "connection 07");
    await expect(page.getByTestId("agent-tool")).toHaveCount(15);

    await filter.fill("nothing like this");
    await expect(page.getByTestId("agent-filter-empty")).toBeVisible();
    await expect(page.getByTestId("agent-connection")).toHaveCount(0);
  });

  test("a connection of your own is not added here: the pane says where", async ({ page }) => {
    await openAgent(page, { agent: { connections: [TEAMCHAT, NOTES] } });
    // No button, no dialog, no form: a connection of one's own is a
    // command that is started on this computer.
    await expect(page.getByRole("button", { name: /add connection/i })).toHaveCount(0);
    await expect(page.getByTestId("agent-add-note")).toHaveText(
      "A connection of your own is added in a terminal, with openwhisper agent connections add-custom.",
    );
    // One that was added there shows like the others, without how it
    // is reached.
    await expect(row(page, "notes").getByTestId("agent-connection-summary")).toHaveText(
      "0 of 1 tools ticked. Your own, started with a command",
    );
    // Switched off, it is never started by a lookup.
    await expect(row(page, "notes").getByTestId("agent-connection-state")).toHaveText(
      "Not looked up yet",
    );
    await page.getByTestId("agent-refresh").click();
    await expect(page.getByTestId("agent-listing-state")).toContainText("Looked up at");
    await expect(row(page, "notes").getByTestId("agent-connection-state")).toHaveText(
      "Not looked up yet",
    );
    await expect(row(page, "teamchat").getByTestId("agent-connection-state")).toHaveText(
      "Connected",
    );
    // There is no command behind the pane to add one, either.
    expect(
      await call(page, "agent_add_connection", {
        name: "sneaked",
        command: "/bin/sh",
        args: [],
        url: "",
      }),
    ).toEqual({ error: "Command agent_add_connection not found" });
    await expect(row(page, "sneaked")).toHaveCount(0);
  });

  test("Remove asks first, and Cancel keeps the connection", async ({ page }) => {
    await openAgent(page, {
      agent: {
        connections: [TEAMCHAT, { ...NOTES, enabled: true, allowed_tools: ["list_notes"] }],
      },
    });
    // Remove shows when the row is hovered.
    await row(page, "notes").hover();
    await row(page, "notes").getByRole("button", { name: "Remove notes" }).click();
    const dialog = page.getByRole("alertdialog");
    await expect(dialog).toBeVisible();
    await expect(dialog).toContainText("Remove the connection “notes”?");
    await expect(dialog).toContainText("It is taken off the list with its ticks");
    await expect(dialog).toContainText("add it again in a terminal");
    // Nothing has been removed by asking.
    expect(await commands(page)).not.toContain("agent_remove_connection");
    await dialog.getByRole("button", { name: "Cancel" }).click();
    await expect(dialog).toHaveCount(0);
    await expect(row(page, "notes")).toBeVisible();
    expect(await commands(page)).not.toContain("agent_remove_connection");
    expect(await ticksOnDisk(page, "notes")).toEqual(["list_notes"]);

    // Again, and this time for real.
    await row(page, "notes").hover();
    await row(page, "notes").getByRole("button", { name: "Remove notes" }).click();
    await page.getByTestId("agent-remove-confirm").click();
    await expect(page.getByRole("alertdialog")).toHaveCount(0);
    await expect(row(page, "notes")).toHaveCount(0);
    const calls =
      (await recorded<{ cmd: string; args: Record<string, unknown> }[]>(page, "__owAgentCalls")) ??
      [];
    expect(calls.filter((c) => c.cmd === "agent_remove_connection").map((c) => c.args)).toEqual([
      { name: "notes" },
    ]);
    // A connection Claude Code has cannot be removed here.
    await row(page, "teamchat").hover();
    await expect(row(page, "teamchat").getByRole("button", { name: "Remove teamchat" })).toHaveCount(
      0,
    );
  });

  test("a connection Claude Code no longer has can be removed, after asking", async ({ page }) => {
    await openAgent(page, {
      agent: { connections: [{ name: "gone", state: "missing", enabled: true }] },
    });
    await row(page, "gone").hover();
    await row(page, "gone").getByRole("button", { name: "Remove gone" }).click();
    const dialog = page.getByRole("alertdialog");
    await expect(dialog).toContainText("Remove the connection “gone”?");
    await expect(dialog).toContainText("Claude Code no longer has it.");
    await page.getByTestId("agent-remove-confirm").click();
    await expect(row(page, "gone")).toHaveCount(0);
  });

  test("Refresh says what a lookup starts, and waits for Ask and the Claude backend", async ({
    page,
  }) => {
    await openAgent(page, { askEnabled: false, agent: { connections: [TEAMCHAT] } });
    await expect(page.getByTestId("agent-refresh-note")).toHaveText(
      "Refresh starts Claude Code and every connection it has for a few seconds, to ask each one for its tools. No tool is called and the model is not asked anything.",
    );
    // Agent is on, Ask is not: nothing would use what a lookup
    // finds, so none is started.
    await expect(page.getByTestId("agent-needs-ask")).toBeVisible();
    await expect(page.getByTestId("agent-refresh")).toBeDisabled();
    // Rust refuses one whoever asks, and says why.
    expect(await call(page, "agent_refresh")).toEqual({
      error:
        "Ask is switched off, so the connections were not looked up. Agent only works in an ask.",
    });
    expect(await recorded(page, "__owAgentLookups")).toBeUndefined();
  });

  test("no lookup with another backend, or with Agent off", async ({ page }) => {
    await openAgent(page, { backend: "openai-compatible", agent: { connections: [TEAMCHAT] } });
    await expect(page.getByTestId("agent-needs-claude")).toBeVisible();
    await expect(page.getByTestId("agent-refresh")).toBeDisabled();
    expect(await call(page, "agent_refresh")).toEqual({
      error: "Agent needs the Claude backend, so the connections were not looked up.",
    });
    await agentSwitch(page).click();
    await expect(agentSwitch(page)).not.toBeChecked();
    expect(await call(page, "agent_refresh")).toEqual({
      error: "Agent is switched off, so the connections were not looked up.",
    });
    expect(await recorded(page, "__owAgentLookups")).toBeUndefined();
  });

  test("a lookup starts when Ask is switched on while Agent is on", async ({ page }) => {
    await openAgent(page, {
      askEnabled: false,
      agent: { connections: [] },
      listing: { connections: [TEAMCHAT] },
    });
    await expect(page.getByTestId("agent-connections-empty")).toBeVisible();
    expect(await recorded(page, "__owAgentLookups")).toBeUndefined();
    // Ask goes on (in its own pane, or in a terminal): the status is
    // asked again, and Rust notices that Agent can use connections
    // now.
    await page.evaluate(() => {
      const w = window as unknown as {
        __owAskStored?: { enabled: boolean };
        __owAskSettings?: { enabled: boolean };
      };
      w.__owAskSettings = { enabled: true };
      if (w.__owAskStored) w.__owAskStored.enabled = true;
      window.__owEmit("ask_changed", w.__owAskStored ?? w.__owAskSettings);
    });
    await expect(row(page, "teamchat")).toBeVisible();
    expect(await recorded(page, "__owAgentLookups")).toBe(1);
    expect(await commands(page)).not.toContain("agent_refresh");
  });

  test("Agent switched on in a terminal is noticed whichever read comes first, and looked up once", async ({
    page,
  }) => {
    await openAgent(page, {
      agent: { enabled: false },
      listing: { connections: [TEAMCHAT, TRACKER] },
    });
    await expect(agentSwitch(page)).not.toBeChecked();
    await expect(page.getByTestId("agent-connections-empty")).toBeVisible();
    // `openwhisper agent on` in a terminal: the file changes, nobody
    // is told.
    await changeElsewhere(page, (stored) => {
      stored.enabled = true;
    });
    const before = (await commands(page)).length;
    // The person comes back to the window: the pane reads again. The
    // status is read too, and whichever of the two reaches Rust
    // first, the lookup starts, once.
    await page.evaluate(() => window.dispatchEvent(new Event("focus")));
    await expect(agentSwitch(page)).toBeChecked();
    await expect(row(page, "teamchat")).toBeVisible();
    await expect(row(page, "tracker")).toBeVisible();
    await expect(page.getByTestId("agent-listing-state")).toContainText("0 of 2 switched on.");
    const after = (await commands(page)).slice(before);
    expect(after).toContain("agent_get");
    expect(after).toContain("agent_status");
    expect(after).not.toContain("agent_refresh");
    expect(await recorded(page, "__owAgentLookups")).toBe(1);

    // The other order: the status is the first to see the change.
    await changeElsewhere(page, (stored) => {
      stored.enabled = false;
    });
    expect(await call(page, "agent_status")).toMatchObject({ ok: { ready: false } });
    await changeElsewhere(page, (stored) => {
      stored.enabled = true;
    });
    expect(await call(page, "agent_status")).toMatchObject({ ok: { ready: false } });
    await call(page, "agent_get");
    await expect.poll(() => recorded(page, "__owAgentLookups")).toBe(2);
    await call(page, "agent_get");
    await call(page, "agent_status");
    expect(await recorded(page, "__owAgentLookups")).toBe(2);
  });

  test("a change made behind the window's back survives an unrelated tick", async ({ page }) => {
    await openAgent(page, {
      agent: {
        connections: [
          { ...TEAMCHAT, enabled: true, allowed_tools: ["read_thread"] },
          { ...TRACKER, enabled: true },
        ],
        app_connections: [{ app_id: "com.example.teamchat", app_name: "Teamchat", connection: "teamchat" }],
      },
    });
    await showTools(page, "teamchat");
    await expect(box(page, "teamchat", "read_thread")).toBeChecked();
    // In a terminal, with the window open and not told: drafts only
    // off, a tool of another connection ticked, another tool of this
    // one ticked, one unticked, the app table changed.
    await changeElsewhere(page, (stored) => {
      stored.drafts_only = false;
      stored.connections[1].allowed_tools = ["list_issues"];
      stored.connections[0].allowed_tools = ["search_messages"];
      stored.app_connections = [
        { app_id: "com.example.mailbox", app_name: "Mailbox", connection: "teamchat" },
      ];
    });
    // The window still shows what it read before.
    await expect(draftsSwitch(page)).toBeChecked();
    await expect(box(page, "teamchat", "search_messages")).not.toBeChecked();
    // One tick in the window, of something else.
    await box(page, "teamchat", "create_draft").click();
    await expect(box(page, "teamchat", "create_draft")).toBeChecked();
    expect(await lastSave(page)).toEqual({
      connection: { name: "teamchat", tick: ["create_draft"] },
    });
    // Everything that was changed elsewhere is still there.
    const disk = await onDisk(page);
    expect(disk.drafts_only).toBe(false);
    expect(disk.connections[1].allowed_tools).toEqual(["list_issues"]);
    expect(disk.connections[0].allowed_tools).toEqual(["search_messages", "create_draft"]);
    expect(disk.app_connections).toEqual([
      { app_id: "com.example.mailbox", app_name: "Mailbox", connection: "teamchat" },
    ]);
    // And the save brought it into the window.
    await expect(draftsSwitch(page)).not.toBeChecked();
    await expect(box(page, "teamchat", "search_messages")).toBeChecked();
    await expect(box(page, "teamchat", "read_thread")).not.toBeChecked();
    await expect(row(page, "tracker").getByTestId("agent-connection-summary")).toHaveText(
      "1 of 1 tools ticked",
    );
    await expect(appRow(page, "com.example.mailbox")).toBeVisible();
  });

  test("a switch in the window does not write the other switch back", async ({ page }) => {
    await openAgent(page, { agent: { connections: [TEAMCHAT] } });
    await expect(draftsSwitch(page)).toBeChecked();
    await changeElsewhere(page, (stored) => {
      stored.drafts_only = false;
    });
    // Agent off and on again from the window, which still shows
    // drafts only as on.
    await agentSwitch(page).click();
    await expect(agentSwitch(page)).not.toBeChecked();
    expect(await lastSave(page)).toEqual({ enabled: false });
    expect((await onDisk(page)).drafts_only).toBe(false);
    await expect(draftsSwitch(page)).not.toBeChecked();
  });

  test("the pane reads again when the window gets the focus back", async ({ page }) => {
    await openAgent(page, { agent: { connections: [TEAMCHAT] } });
    await expect(use(page, "teamchat")).not.toBeChecked();
    await changeElsewhere(page, (stored) => {
      stored.connections[0].enabled = true;
      stored.connections[0].allowed_tools = ["read_thread"];
    });
    await page.evaluate(() => window.dispatchEvent(new Event("focus")));
    await expect(use(page, "teamchat")).toBeChecked();
    await expect(row(page, "teamchat").getByTestId("agent-connection-summary")).toHaveText(
      "1 of 5 tools ticked",
    );
    await expect(page.getByTestId("agent-status")).toContainText("An ask can use 1 tool");
  });

  test("the answer of a lookup does not take a tick made meanwhile off the screen", async ({
    page,
  }) => {
    await openAgent(page, { agent: { connections: [{ ...TEAMCHAT, enabled: true }] } });
    await showTools(page, "teamchat");
    const tick = box(page, "teamchat", "read_thread");
    await expect(tick).not.toBeChecked();
    // Every state the box goes through from here on.
    await tick.evaluate((el) => {
      const w = window as unknown as { __owTickSeen: string[] };
      w.__owTickSeen = [];
      const read = () => el.getAttribute("aria-checked") ?? String(el.hasAttribute("data-checked"));
      new MutationObserver(() => w.__owTickSeen.push(read())).observe(el, { attributes: true });
    });
    await page.evaluate(() => {
      const w = window as unknown as Record<string, number>;
      // The lookup has its answer ready at once and delivers it
      // late; the save of the tick is on its way for longer still.
      w.__owAgentRefreshAnswerMs = 500;
      w.__owAgentSetMs = 1100;
    });
    await page.getByTestId("agent-refresh").click();
    await tick.click();
    await expect(tick).toBeChecked();
    // Both have answered: the lookup first, then the save.
    await expect(page.getByTestId("agent-listing-state")).toContainText("Looked up at");
    await expect.poll(async () => (await lastSave(page))?.connection?.tick).toEqual(["read_thread"]);
    await expect
      .poll(() => recorded<number>(page, "__owAgentAnswered"), { timeout: 5000 })
      .toBe(2);
    await expect(tick).toBeChecked();
    // Ticked once, and never unticked in between.
    const seen = (await recorded<string[]>(page, "__owTickSeen")) ?? [];
    expect(seen.length).toBeGreaterThan(0);
    expect(new Set(seen.filter((v) => v === "true" || v === "false"))).toEqual(new Set(["true"]));
    expect(await ticksOnDisk(page, "teamchat")).toEqual(["read_thread"]);
  });

  test("a command without what Rust requires is refused, as in Rust", async ({ page }) => {
    await openAgent(page, { agent: { connections: [TEAMCHAT] } });
    const refused = async (cmd: string, args?: Record<string, unknown>) => {
      const answer = await call(page, cmd, args);
      return "error" in answer ? answer.error : `taken: ${JSON.stringify(answer.ok)}`;
    };
    expect(await refused("agent_set")).toContain("missing required key patch");
    expect(await refused("agent_set", { choices: { enabled: false } })).toContain(
      "missing required key patch",
    );
    expect(await refused("agent_remove_connection")).toContain("missing required key name");
    // Fields Rust does not know: the whole list, the old way, and
    // anything that says what a connection is.
    expect(
      await refused("agent_set", { patch: { connections: [{ name: "teamchat", enabled: true }] } }),
    ).toContain("unknown field `connections`");
    expect(
      await refused("agent_set", {
        patch: { connection: { name: "teamchat", allowed_tools: ["send_message"] } },
      }),
    ).toContain("unknown field `allowed_tools`");
    expect(
      await refused("agent_set", { patch: { connection: { name: "nobody", enabled: true } } }),
    ).toBe('There is no connection called "nobody".');
    // A tool nobody has seen.
    expect(
      await refused("agent_set", {
        patch: { connection: { name: "teamchat", tick: ["wipe_everything"] } },
      }),
    ).toBe('"wipe_everything" is not a tool this connection is known to have. Refresh and try again.');
    // A yes or no to a confirm is not taken from this window.
    for (const cmd of ["ask_confirm_allow", "ask_confirm_deny"]) {
      expect(await refused(cmd, { askId: 1, requestId: "r1" })).toBe(
        `${cmd} is not accepted from this window.`,
      );
    }
    // Nothing of all that changed anything.
    expect(await recorded(page, "__owAgentSaves")).toBeUndefined();
    expect(await onDisk(page)).toMatchObject({ enabled: true, drafts_only: true });
    expect(await ticksOnDisk(page, "teamchat")).toEqual([]);
  });

  test("a change made elsewhere shows without a reload", async ({ page }) => {
    await openAgent(page, { agent: { connections: [TEAMCHAT] } });
    await expect(use(page, "teamchat")).not.toBeChecked();
    // The headless CLI switched the connection on: Rust announces it.
    await page.evaluate(() => {
      const w = window as unknown as {
        __owAgentStored: { connections: { enabled: boolean; allowed_tools: string[] }[] };
      };
      w.__owAgentStored.connections[0].enabled = true;
      w.__owAgentStored.connections[0].allowed_tools = ["read_thread"];
      const { invoke } = window.__TAURI_INTERNALS__ as {
        invoke: (cmd: string) => Promise<unknown>;
      };
      return invoke("agent_get").then((state) => window.__owEmit("agent_changed", state));
    });
    await expect(use(page, "teamchat")).toBeChecked();
    await expect(row(page, "teamchat").getByTestId("agent-connection-summary")).toHaveText(
      "1 of 5 tools ticked",
    );
    await expect(page.getByTestId("agent-status")).toContainText("An ask can use 1 tool");
  });
});

test.describe("agent pane: apps", () => {
  const READY: ConnectionSeed = { ...TEAMCHAT, enabled: true, allowed_tools: ["read_thread"] };

  test("shows the built-in table and which rows an ask would follow", async ({ page }) => {
    await openAgent(page, { agent: { connections: [READY, TRACKER] } });
    await expect(page.getByTestId("agent-apps-intro")).toContainText(
      "the ask reads from the app's connection and takes no screenshot",
    );
    await expect(page.getByTestId("agent-app-row")).toHaveCount(2);
    // Teamchat: its connection is on with a tool ticked.
    await expect(appRow(page, "com.example.teamchat")).toHaveAttribute("data-active", "true");
    await expect(appRow(page, "com.example.teamchat")).toContainText("Teamchat");
    await expect(appRow(page, "com.example.teamchat")).toContainText("com.example.teamchat");
    await expect(appRow(page, "com.example.teamchat").getByTestId("agent-app-note")).toHaveText(
      "In use.",
    );
    // Tracker: the connection is there, but off.
    await expect(appRow(page, "com.example.tracker")).toHaveAttribute("data-active", "false");
    await expect(appRow(page, "com.example.tracker").getByTestId("agent-app-note")).toHaveText(
      "Not in use: the connection is off or has no tool that can run.",
    );
    // The built-in table has nothing to restore.
    await expect(page.getByTestId("agent-apps-restore")).toHaveCount(0);
  });

  test("a row whose connection does not exist says so", async ({ page }) => {
    await openAgent(page, { agent: { connections: [READY] } });
    await expect(appRow(page, "com.example.tracker").getByTestId("agent-app-note")).toHaveText(
      "Not in use: there is no connection of this name.",
    );
  });

  test("the connection of an app can be changed", async ({ page }) => {
    await openAgent(page, { agent: { connections: [READY, TRACKER] } });
    await page.getByRole("combobox", { name: "Connection for Tracker" }).click();
    await page.getByRole("option", { name: "teamchat", exact: true }).click();
    // The table, and nothing else.
    expect(await lastSave(page)).toEqual({
      app_connections: [
        { app_id: "com.example.teamchat", app_name: "Teamchat", connection: "Teamchat" },
        { app_id: "com.example.tracker", app_name: "Tracker", connection: "teamchat" },
      ],
    });
    await expect(appRow(page, "com.example.tracker")).toHaveAttribute("data-active", "true");
    // The table is the person's now, so it can go back.
    await page.getByTestId("agent-apps-restore").click();
    await expect(page.getByTestId("agent-apps-restore")).toHaveCount(0);
    expect(await lastSave(page)).toEqual({ app_connections: null });
    await expect(appRow(page, "com.example.tracker")).toHaveAttribute("data-active", "false");
    await expect(page.getByTestId("agent-apps-restore")).toHaveCount(0);
  });

  test("a row can be removed", async ({ page }) => {
    await openAgent(page, { agent: { connections: [READY] } });
    await appRow(page, "com.example.tracker").hover();
    await appRow(page, "com.example.tracker")
      .getByRole("button", { name: "Remove Tracker" })
      .click();
    await expect(appRow(page, "com.example.tracker")).toHaveCount(0);
    expect((await lastSave(page))?.app_connections).toEqual([
      { app_id: "com.example.teamchat", app_name: "Teamchat", connection: "Teamchat" },
    ]);
    await expect(page.getByTestId("agent-app-row")).toHaveCount(1);
  });

  test("an open app can be added with its connection", async ({ page }) => {
    await openAgent(page, {
      agent: { connections: [READY] },
      running: [
        { id: "com.example.Mailbox", name: "Mailbox" },
        // Already on the table: not offered again.
        { id: "com.example.teamchat", name: "Teamchat" },
      ],
    });
    const add = page.getByTestId("agent-app-add-button");
    await expect(add).toBeDisabled();
    await page.getByRole("combobox", { name: "App to add", exact: true }).click();
    await expect(page.getByRole("option")).toHaveCount(1);
    await page.getByRole("option", { name: "Mailbox" }).click();
    await expect(add).toBeDisabled();
    await page.getByRole("combobox", { name: "Connection for the app to add" }).click();
    await page.getByRole("option", { name: "teamchat", exact: true }).click();
    await add.click();
    expect((await lastSave(page))?.app_connections?.at(-1)).toEqual({
      app_id: "com.example.mailbox",
      app_name: "Mailbox",
      connection: "teamchat",
    });
    await expect(appRow(page, "com.example.mailbox")).toHaveAttribute("data-active", "true");
    await expect(page.getByTestId("agent-app-row")).toHaveCount(3);
  });

  test("the table waits for the switch like the connections do", async ({ page }) => {
    await openAgent(page, { agent: { enabled: false, connections: [READY] } });
    await expect(page.getByRole("combobox", { name: "Connection for Teamchat" })).toBeDisabled();
    await expect(page.getByRole("combobox", { name: "App to add", exact: true })).toBeDisabled();
  });
});

// --- the pill -------------------------------------------------------------

pillTest.describe("pill: an ask that waits for a yes or no", () => {
  async function mount(page: Page) {
    // The size of the real pill window.
    await page.setViewportSize({ width: 180, height: 110 });
    await page.goto("/");
    await page.locator(".pill-capsule").waitFor({ state: "visible" });
    await waitForAskTickListener(page);
  }

  pillTest("says Confirm for as long as the confirm waits", async ({ page }) => {
    await mount(page);
    const label = page.getByTestId("pill-mode-label");
    await expect(label).toHaveAttribute("data-visible", "false");

    await emitAskTick(page, { phase: "thinking" });
    await expect(label).toHaveAttribute("data-visible", "false");

    // The page's clock is the test's from here: nothing runs out by
    // itself, time passes when the test says so.
    await page.clock.install();
    await page.clock.pauseAt((await page.evaluate(() => Date.now())) + 1000);
    await emitAskTick(page, { phase: "confirming" });
    await expect(label).toHaveText("Confirm");
    await expect(label).toHaveAttribute("data-visible", "true");
    await expect(label).toHaveAttribute("data-kind", "confirm");
    // It does not time out by itself, however long the confirm
    // waits: a minute passes, with the ticks that repeat meanwhile.
    await page.clock.runFor(30_000);
    await emitAskTick(page, { phase: "confirming" });
    await page.clock.runFor(30_000);
    await expect(label).toHaveText("Confirm");
    await expect(label).toHaveAttribute("data-visible", "true");

    // Answered: gone at once.
    await emitAskTick(page, { phase: "thinking" });
    await expect(label).toHaveAttribute("data-visible", "false");
  });

  // `modes_changed` as Rust sends it when the person switched to
  // the mode called `name` (the hotkey, the tray menu).
  const switchedTo = (name: string): MockModesChanged => {
    const mode = { id: name.toLowerCase(), name, instructions: "", model: null, effort: null };
    return {
      settings: { active: mode.id, custom: [mode], app_defaults: [] },
      modes: [
        { id: "dictation", name: "Dictation", instructions: "", model: null, effort: null, builtin: true },
        { ...mode, builtin: false },
      ],
      switched_to: name,
    };
  };

  pillTest("comes back after the name of a mode that was switched to meanwhile", async ({
    page,
  }) => {
    await mount(page);
    await waitForModesListener(page);
    const label = page.getByTestId("pill-mode-label");
    await page.clock.install();
    await page.clock.pauseAt((await page.evaluate(() => Date.now())) + 1000);
    await emitAskTick(page, { phase: "confirming" });
    await expect(label).toHaveText("Confirm");
    // The person switches the mode while the confirm waits: the
    // label says the mode, for its moment.
    await emitModesChanged(page, switchedTo("Formal"));
    await expect(label).toHaveText("Formal");
    await expect(label).toHaveAttribute("data-kind", "mode");
    await expect(label).toHaveAttribute("data-visible", "true");
    // The confirm still waits: its label is back, and stays.
    await page.clock.runFor(10_000);
    await expect(label).toHaveText("Confirm");
    await expect(label).toHaveAttribute("data-kind", "confirm");
    await expect(label).toHaveAttribute("data-visible", "true");
    await page.clock.runFor(60_000);
    await expect(label).toHaveAttribute("data-visible", "true");
    // Answered: gone.
    await emitAskTick(page, { phase: "thinking" });
    await expect(label).toHaveAttribute("data-visible", "false");

    // A mode switched to when nothing waits goes away by itself, as
    // before, and so does one whose confirm was answered meanwhile.
    await emitModesChanged(page, switchedTo("Formal"));
    await expect(label).toHaveText("Formal");
    await page.clock.runFor(10_000);
    await expect(label).toHaveAttribute("data-visible", "false");
    await emitAskTick(page, { phase: "confirming" });
    await emitModesChanged(page, switchedTo("Notes"));
    await expect(label).toHaveText("Notes");
    await emitAskTick(page, { phase: "thinking" });
    await expect(label).toHaveText("Notes");
    await page.clock.runFor(10_000);
    await expect(label).toHaveAttribute("data-visible", "false");
  });

  pillTest("goes away when the ask is cancelled while it waits", async ({ page }) => {
    await mount(page);
    const label = page.getByTestId("pill-mode-label");
    await emitAskTick(page, { phase: "confirming" });
    await expect(label).toHaveAttribute("data-visible", "true");
    await emitAskTick(page, { phase: "cancelled" });
    await expect(label).toHaveAttribute("data-visible", "false");
  });
});

// --- the history ----------------------------------------------------------

test.describe("history: asks that used tools", () => {
  async function seedHistory(page: Page) {
    await page.addInitScript(() => {
      const w = window as unknown as {
        __owHistorySettings?: unknown;
        __owHistoryEntries?: unknown[];
      };
      w.__owHistorySettings = { enabled: true };
      const noon = new Date();
      noon.setHours(12, 0, 0, 0);
      const at = (hoursAgo: number) => noon.getTime() - hoursAgo * 3_600_000;
      const base = {
        duration_ms: 4000,
        word_count: 5,
        original: null,
        app_id: null,
        app_name: null,
      };
      w.__owHistoryEntries = [
        {
          ...base,
          id: "a3",
          at_ms: at(1),
          text: "tell Bo I will be there at ten",
          kind: "ask",
          answer: "I prepared a draft to Bo. It was not sent.",
          tools: [
            { connection: "teamchat", tool: "read_thread", outcome: "done" },
            { connection: "teamchat", tool: "create_draft", outcome: "done" },
            { connection: "teamchat", tool: "send_message", outcome: "denied" },
          ],
        },
        {
          ...base,
          id: "a4",
          at_ms: at(1.5),
          text: "post the notes to the channel",
          kind: "ask",
          // An ask that did not finish, as core stores it.
          answer: "I am posting them now.\n\n(Incomplete. No answer within 60 seconds.)",
          tools: [
            { connection: "teamchat", tool: "read_thread", outcome: "done" },
            { connection: "teamchat", tool: "post_message", outcome: "result_not_known" },
          ],
        },
        {
          ...base,
          id: "a2",
          at_ms: at(2),
          text: "what is a tap",
          kind: "ask",
          answer: "A short press of a key.",
        },
        { ...base, id: "e1", at_ms: at(3), text: "Remember to book the meeting room." },
      ];
    });
  }

  test("Home marks the ask that used tools and lists them with how each ended", async ({
    page,
  }) => {
    await seedHistory(page);
    await page.goto("/");
    const rows = page.getByTestId("history-row");
    await expect(rows).toHaveCount(4);

    const withTools = rows.nth(0);
    await expect(withTools.getByTestId("history-row-ask")).toHaveText("Ask");
    await expect(withTools.getByTestId("history-row-tools")).toHaveText("3 tool calls");
    // Listed on request, like the answer.
    await expect(withTools.getByTestId("history-row-tool-list")).toHaveCount(0);
    await withTools.getByRole("button", { name: "Show tools" }).click();
    await expect(withTools.getByTestId("history-row-tool")).toHaveText([
      "read_thread in teamchat: done",
      "create_draft in teamchat: done",
      "send_message in teamchat: not run",
    ]);
    await withTools.getByRole("button", { name: "Hide tools" }).click();
    await expect(withTools.getByTestId("history-row-tool-list")).toHaveCount(0);

    // An ask that ended while a tool was running: whether that call
    // went through is not known, and the row says so in plain words.
    const unfinished = rows.nth(1);
    await expect(unfinished.getByTestId("history-row-tools")).toHaveText("2 tool calls");
    await unfinished.getByRole("button", { name: "Show tools" }).click();
    await expect(unfinished.getByTestId("history-row-tool")).toHaveText([
      "read_thread in teamchat: done",
      "post_message in teamchat: result not known, the ask ended while it was running",
    ]);
    await expect(unfinished.getByTestId("history-row-tool").nth(1)).toHaveAttribute(
      "data-outcome",
      "result_not_known",
    );
    // Its answer is kept as far as it got, and says that it is not
    // whole.
    await unfinished.getByRole("button", { name: "Show answer" }).click();
    await expect(unfinished.getByTestId("history-row-answer")).toContainText(
      "(Incomplete. No answer within 60 seconds.)",
    );

    // An ask without tools and a dictation have neither.
    await expect(rows.nth(2).getByTestId("history-row-ask")).toHaveText("Ask");
    await expect(rows.nth(2).getByTestId("history-row-tools")).toHaveCount(0);
    await expect(rows.nth(2).getByRole("button", { name: "Show tools" })).toHaveCount(0);
    await expect(rows.nth(3).getByTestId("history-row-tools")).toHaveCount(0);
  });

  test("the History screen marks it too", async ({ page }) => {
    await seedHistory(page);
    await page.goto("/");
    await page.getByTestId("sidebar-item-history").click();
    await expect(page.getByTestId("history-pane")).toBeVisible();
    await expect(page.getByTestId("history-row")).toHaveCount(4);
    await expect(page.getByTestId("history-row-tools")).toHaveCount(2);
    await expect(page.getByTestId("history-row").nth(0).getByTestId("history-row-tools")).toHaveText(
      "3 tool calls",
    );
  });
});
