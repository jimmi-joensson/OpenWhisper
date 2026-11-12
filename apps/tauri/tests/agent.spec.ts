import {
  emitAskTick,
  expect,
  pillTest,
  test,
  waitForAskTickListener,
} from "./fixtures/tauri-shim";

type Page = import("@playwright/test").Page;

// Agent (TASK-99.3): the Agent pane. The switch with its plain
// statement, drafts only, the connections with a refresh, their tools
// by kind, and the app table.
//
// Nothing here starts Claude Code or reaches a connection: the shim
// keeps an Agent block in memory that behaves like Rust. Every
// connection, tool, app and person is made up.

interface ToolSeed {
  name: string;
  title?: string;
  kind: "read" | "draft" | "act";
}

interface ConnectionSeed {
  name: string;
  kind?: "claude" | "custom";
  enabled?: boolean;
  allowed_tools?: string[];
  tools?: ToolSeed[];
  state?: string;
  usable?: boolean;
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
  usable: true,
  tools: TEAMCHAT_TOOLS,
};

const TRACKER: ConnectionSeed = {
  name: "tracker",
  state: "needs-auth",
  usable: true,
  tools: [{ name: "list_issues", kind: "read" }],
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

interface Choices {
  enabled: boolean;
  drafts_only: boolean;
  connections: { name: string; enabled: boolean; allowed_tools: string[] }[];
  app_connections: { app_id: string; app_name: string; connection: string }[] | null;
}

async function lastSave(page: Page): Promise<Choices | undefined> {
  return (await recorded<Choices[]>(page, "__owAgentSaves"))?.at(-1);
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
    await expect(cannot).toContainText("It cannot use a connection or a tool you have not ticked.");
    await expect(cannot).toContainText(
      "It cannot send, post, delete or change anything while Drafts only is on.",
    );
    await expect(cannot).toContainText("It cannot open your files or run commands");
    await expect(page.getByTestId("agent-privacy")).toContainText(
      "What a tool returns is sent to Claude together with your question.",
    );
    // Off: the lists wait for the switch, and say so.
    await expect(page.getByTestId("agent-off-note")).toBeVisible();
    await expect(page.getByTestId("agent-refresh")).toBeDisabled();
    await expect(page.getByRole("button", { name: "Add connection" })).toBeDisabled();
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
    expect(await lastSave(page)).toMatchObject({ enabled: true, drafts_only: true });
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
    expect(await lastSave(page)).toMatchObject({ enabled: true, drafts_only: false });
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
          { name: "alpha", state: "failed", usable: true },
          { name: "bravo", state: "pending", usable: true },
          { name: "charlie", state: "disabled", usable: true },
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
    expect((await lastSave(page))?.connections).toEqual([
      { name: "teamchat", enabled: true, allowed_tools: [] },
      { name: "tracker", enabled: false, allowed_tools: [] },
    ]);
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
    expect((await lastSave(page))?.connections[0]).toEqual({
      name: "teamchat",
      enabled: true,
      allowed_tools: ["read_thread"],
    });
    await box(page, "teamchat", "create_draft").click();
    expect((await lastSave(page))?.connections[0].allowed_tools).toEqual([
      "read_thread",
      "create_draft",
    ]);
    await expect(teamchat.getByTestId("agent-connection-summary")).toHaveText(
      "2 of 5 tools ticked",
    );
    await expect(page.getByTestId("agent-status")).toHaveText(
      "An ask can use 2 tools on 1 connection. Drafts only: nothing that sends, posts, deletes or changes anything can run.",
    );
    // And off again.
    await box(page, "teamchat", "read_thread").click();
    await expect(box(page, "teamchat", "read_thread")).not.toBeChecked();
    expect((await lastSave(page))?.connections[0].allowed_tools).toEqual(["create_draft"]);
  });

  test("a whole group of read tools can be ticked at once, the tools that act cannot", async ({
    page,
  }) => {
    await openAgent(page, { agent: { connections: [{ ...TEAMCHAT, enabled: true }] } });
    await showTools(page, "teamchat");
    const read = row(page, "teamchat").getByTestId("agent-tools-read");
    await read.getByRole("button", { name: "Tick all" }).click();
    expect((await lastSave(page))?.connections[0].allowed_tools).toEqual([
      "read_thread",
      "search_messages",
    ]);
    await read.getByRole("button", { name: "Untick all" }).click();
    expect((await lastSave(page))?.connections[0].allowed_tools).toEqual([]);
    const act = row(page, "teamchat").getByTestId("agent-tools-act");
    await expect(act.getByRole("button", { name: "Tick all" })).toHaveCount(0);
  });

  test("with drafts only, the tools that act are marked and cannot be ticked", async ({ page }) => {
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
      "Unavailable while Drafts only is on: none of these can run.",
    );
    // Ticked earlier or not, they are locked.
    await expect(box(page, "teamchat", "send_message")).toBeChecked();
    await expect(box(page, "teamchat", "send_message")).toBeDisabled();
    await expect(box(page, "teamchat", "delete_message")).toBeDisabled();
    // The others are not.
    await expect(box(page, "teamchat", "read_thread")).toBeEnabled();
    await expect(box(page, "teamchat", "create_draft")).toBeEnabled();
    // The ticked tool that acts does not count as one an ask can use.
    await expect(page.getByTestId("agent-status")).toContainText("An ask can use 1 tool on 1");
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
    expect((await lastSave(page))?.connections[0].allowed_tools).toEqual([
      "read_thread",
      "send_message",
    ]);
    await expect(page.getByTestId("agent-status")).toHaveText(
      "An ask can use 2 tools on 1 connection. 1 of them ask you first, every time.",
    );
    // Drafts only back on: locked again, still ticked.
    await draftsSwitch(page).click();
    await expect(box(page, "teamchat", "send_message")).toBeDisabled();
    await expect(box(page, "teamchat", "send_message")).toBeChecked();
    await expect(act.getByTestId("agent-act-badge")).toHaveText("Unavailable");
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
      usable: true,
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

  test("a connection of your own is added by command, switched off, and can be removed", async ({
    page,
  }) => {
    await openAgent(page, { agent: { connections: [TEAMCHAT] } });
    await page.getByRole("button", { name: "Add connection" }).click();
    const dialog = page.getByTestId("agent-connection-dialog");
    await expect(dialog).toBeVisible();
    const add = dialog.getByRole("button", { name: "Add connection" });
    await expect(add).toBeDisabled();
    await dialog.getByLabel("Name").fill("notes");
    await dialog.getByLabel("Command").fill("/opt/notes/notes-server");
    await dialog.getByLabel("Arguments").fill("--stdio\n\n--folder\n/tmp/notes");
    await add.click();
    await expect(dialog).toHaveCount(0);
    expect(await recorded(page, "__owAgentAdded")).toEqual([
      {
        name: "notes",
        command: "/opt/notes/notes-server",
        args: ["--stdio", "--folder", "/tmp/notes"],
        url: "",
      },
    ]);
    // Off, nothing ticked, tools not known yet. The command itself
    // is not on the page.
    await expect(use(page, "notes")).not.toBeChecked();
    await expect(row(page, "notes").getByTestId("agent-connection-summary")).toHaveText(
      "No tools known. Your own, started with a command",
    );
    await expect(page.getByText("/opt/notes/notes-server")).toHaveCount(0);

    // Remove shows when the row is hovered.
    await row(page, "notes").hover();
    await row(page, "notes").getByRole("button", { name: "Remove notes" }).click();
    await expect(row(page, "notes")).toHaveCount(0);
    expect(await commands(page)).toContain("agent_remove_connection");
    // A connection Claude Code has cannot be removed here.
    await row(page, "teamchat").hover();
    await expect(row(page, "teamchat").getByRole("button", { name: "Remove teamchat" })).toHaveCount(
      0,
    );
  });

  test("a connection of your own can be reached at a URL, and a taken name is refused", async ({
    page,
  }) => {
    await openAgent(page, { agent: { connections: [TEAMCHAT] } });
    await page.getByRole("button", { name: "Add connection" }).click();
    const dialog = page.getByTestId("agent-connection-dialog");
    await dialog.getByLabel("Name").fill("TeamChat");
    await dialog.getByRole("button", { name: "URL", exact: true }).click();
    await dialog.getByLabel("URL").fill("https://notes.example/mcp");
    await dialog.getByRole("button", { name: "Add connection" }).click();
    await expect(dialog.getByRole("alert")).toHaveText(
      'There is already a connection called "TeamChat".',
    );
    await dialog.getByLabel("Name").fill("web notes");
    await dialog.getByRole("button", { name: "Add connection" }).click();
    await expect(dialog).toHaveCount(0);
    expect(await recorded(page, "__owAgentAdded")).toEqual([
      { name: "web notes", command: "", args: [], url: "https://notes.example/mcp" },
    ]);
    await expect(row(page, "web notes").getByTestId("agent-connection-summary")).toHaveText(
      "No tools known. Your own, reached at a URL",
    );
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
    expect((await lastSave(page))?.app_connections).toEqual([
      { app_id: "com.example.teamchat", app_name: "Teamchat", connection: "Teamchat" },
      { app_id: "com.example.tracker", app_name: "Tracker", connection: "teamchat" },
    ]);
    await expect(appRow(page, "com.example.tracker")).toHaveAttribute("data-active", "true");
    // The table is the person's now, so it can go back.
    await page.getByTestId("agent-apps-restore").click();
    expect((await lastSave(page))?.app_connections).toBeNull();
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

    await emitAskTick(page, { phase: "confirming" });
    await expect(label).toHaveText("Confirm");
    await expect(label).toHaveAttribute("data-visible", "true");
    await expect(label).toHaveAttribute("data-kind", "confirm");
    // It does not time out by itself: the ticks repeat while it waits.
    await page.waitForTimeout(1700);
    await emitAskTick(page, { phase: "confirming" });
    await expect(label).toHaveAttribute("data-visible", "true");

    // Answered: gone at once.
    await emitAskTick(page, { phase: "thinking" });
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
    await expect(rows).toHaveCount(3);

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

    // An ask without tools and a dictation have neither.
    await expect(rows.nth(1).getByTestId("history-row-ask")).toHaveText("Ask");
    await expect(rows.nth(1).getByTestId("history-row-tools")).toHaveCount(0);
    await expect(rows.nth(1).getByRole("button", { name: "Show tools" })).toHaveCount(0);
    await expect(rows.nth(2).getByTestId("history-row-tools")).toHaveCount(0);
  });

  test("the History screen marks it too", async ({ page }) => {
    await seedHistory(page);
    await page.goto("/");
    await page.getByTestId("sidebar-item-history").click();
    await expect(page.getByTestId("history-pane")).toBeVisible();
    await expect(page.getByTestId("history-row")).toHaveCount(3);
    await expect(page.getByTestId("history-row-tools")).toHaveCount(1);
    await expect(page.getByTestId("history-row").nth(0).getByTestId("history-row-tools")).toHaveText(
      "3 tool calls",
    );
  });
});
