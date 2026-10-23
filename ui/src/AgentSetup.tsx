/**
 * Setting up an agent, for the step after joining and for Settings: the agents found, each with a Check, and when there is none on Windows, which agent the person uses and how to install it.
 * Cursor installs in one click; Copilot and Claude Code have steps to copy. WSL is mentioned only to someone who has a distro.
 */

import { useEffect, useRef, useState } from "react";
import type { JSX } from "react";

import { Button } from "@/components/ui/button";

import type { AgentInfo, AgentKind, PlacesView, Request, RunsIn } from "./desk.ts";
import { ask, problem } from "./deskClient.ts";

/** The official install of Cursor's agent on Windows: the same command the desk runs for Set up. */
const CURSOR_INSTALL = "irm 'https://cursor.com/install?win32=true' | iex";
/** The same, inside a WSL distro. */
const CURSOR_IN_WSL = "curl https://cursor.com/install -fsS | bash";
const COPILOT_INSTALL = "winget install GitHub.Copilot";
const CLAUDE_INSTALL = "npm install -g @agentclientprotocol/claude-agent-acp";

export const LABELS: Readonly<Record<AgentKind, string>> = { cursor: "Cursor", copilot: "Copilot", claude: "Claude Code" };

/** Where something runs, in words: Windows, or a distro's name. */
export function where(runsIn: RunsIn): string {
  return runsIn === "windows" ? "Windows" : runsIn.replace(/^wsl:/, "");
}

/** Text to copy, in a box the person can select, with a button that copies it. */
function Command({ text }: { readonly text: string }): JSX.Element {
  const [copied, setCopied] = useState(false);
  return (
    <div className="my-1 flex items-center gap-2">
      <code className="flex-1 rounded bg-muted px-2 py-1 font-mono text-xs select-all">{text}</code>
      <Button
        type="button"
        variant="ghost"
        size="xs"
        onClick={() => {
          navigator.clipboard.writeText(text).then(
            () => {
              setCopied(true);
            },
            () => {
              setCopied(false);
            }
          );
        }}
      >
        {copied ? "Copied" : "Copy"}
      </Button>
    </div>
  );
}

/** One agent found: the one that runs on its system has a Check, and Sign in when the check says it must be; any other agent has a button that makes it the one. */
function AgentRow({
  agent,
  wsl,
  alone,
  checkNow,
  onChoose
}: {
  readonly agent: AgentInfo;
  readonly wsl: boolean;
  readonly alone: boolean;
  /** Whether to check it as soon as it is shown. */
  readonly checkNow: boolean;
  readonly onChoose: () => void;
}): JSX.Element {
  const [busy, setBusy] = useState<"checking" | "signing in" | null>(null);
  const [result, setResult] = useState<{ readonly ready: boolean; readonly message: string; readonly signIn: boolean } | null>(null);
  /** Checks the agent, or signs it in and then checks it. */
  const run = (request: Request, doing: "checking" | "signing in"): void => {
    setBusy(doing);
    ask(request).then(
      (response) => {
        setBusy(null);
        if (response.type === "checked") {
          setResult({ ready: response.ready, message: response.message, signIn: response.sign_in });
        }
      },
      (failure: unknown) => {
        setBusy(null);
        setResult({ ready: false, message: problem(failure), signIn: false });
      }
    );
  };
  const check = (): void => {
    run({ type: "check_agent", runs_in: agent.runs_in }, "checking");
  };
  const checked = useRef(false);
  useEffect(() => {
    if (checkNow && agent.chosen && !checked.current) {
      checked.current = true;
      check();
    }
  });
  return (
    <li className="py-1">
      <div className="flex items-center gap-2">
        <span className="font-medium">{LABELS[agent.kind]}</span>
        {wsl && <span className="text-xs text-muted-foreground">on {where(agent.runs_in)}</span>}
        {!alone && agent.chosen && <span className="text-xs text-muted-foreground">· runs there</span>}
        {agent.chosen ? (
          <Button type="button" variant="ghost" size="xs" className="ml-auto" disabled={busy !== null} onClick={check}>
            {busy === "checking" ? "Checking…" : "Check"}
          </Button>
        ) : (
          <Button type="button" variant="ghost" size="xs" className="ml-auto" onClick={onChoose}>
            Use this one
          </Button>
        )}
      </div>
      {busy === "signing in" && <p className="text-xs text-muted-foreground">Signing in: finish in the browser window that opened.</p>}
      {busy === null && result !== null && <p className={result.ready ? "text-xs text-emerald-700 dark:text-emerald-400" : "text-xs text-destructive"}>{result.message}</p>}
      {result?.signIn === true && (
        <Button
          type="button"
          size="sm"
          className="mt-1"
          disabled={busy !== null}
          onClick={() => {
            run({ type: "sign_in", runs_in: agent.runs_in }, "signing in");
          }}
        >
          Sign in
        </Button>
      )}
    </li>
  );
}

/** The button that runs Cursor's installer, what it printed, and the command for doing it by hand. */
function SetUpCursor({ onDone }: { readonly onDone: () => void }): JSX.Element {
  const [busy, setBusy] = useState(false);
  const [said, setSaid] = useState<{ readonly ok: boolean; readonly output: string } | null>(null);
  const install = (): void => {
    setBusy(true);
    setSaid(null);
    ask({ type: "install_cursor" }).then(
      (response) => {
        setBusy(false);
        if (response.type === "installed") {
          setSaid({ ok: response.ok, output: response.output });
        }
        onDone();
      },
      (failure: unknown) => {
        setBusy(false);
        setSaid({ ok: false, output: problem(failure) });
      }
    );
  };
  return (
    <>
      <p>Set up installs Cursor&rsquo;s agent for you alone. It needs no admin rights, and takes a minute.</p>
      <Button type="button" size="sm" className="mt-2" disabled={busy} onClick={install}>
        {busy ? "Setting up…" : "Set up Cursor"}
      </Button>
      {said !== null && (
        <>
          <p className={said.ok ? "mt-2 text-xs text-emerald-700 dark:text-emerald-400" : "mt-2 text-xs text-destructive"}>{said.ok ? "The installer finished." : "The installer did not finish."}</p>
          {said.output !== "" && <pre className="my-1 max-h-48 overflow-auto rounded bg-muted p-2 text-xs whitespace-pre-wrap">{said.output}</pre>}
        </>
      )}
      <p className="mt-3 text-xs text-muted-foreground">If the button does not work, open PowerShell and run this yourself:</p>
      <Command text={CURSOR_INSTALL} />
    </>
  );
}

/** How to install the agent picked, when none is on Windows. */
function Install({ kind, onLook }: { readonly kind: AgentKind; readonly onLook: () => void }): JSX.Element {
  switch (kind) {
    case "cursor":
      return <SetUpCursor onDone={onLook} />;
    case "copilot":
      return (
        <>
          <p>Copilot&rsquo;s agent needs PowerShell 7. Open PowerShell and run:</p>
          <Command text={COPILOT_INSTALL} />
          <p>Then press Look again.</p>
        </>
      );
    case "claude":
      return (
        <>
          <p>Claude Code&rsquo;s agent for Player Two needs Node. Open PowerShell and run:</p>
          <Command text={CLAUDE_INSTALL} />
          <p>Then press Look again.</p>
        </>
      );
  }
}

/** Which agent the person uses, as three buttons, with Cursor first. */
function Choice({ kind, onKind }: { readonly kind: AgentKind; readonly onKind: (kind: AgentKind) => void }): JSX.Element {
  const kinds: readonly AgentKind[] = ["cursor", "copilot", "claude"];
  return (
    <div className="my-2 flex flex-wrap gap-1" role="radiogroup" aria-label="Which agent you use">
      {kinds.map((each) => (
        <button
          key={each}
          type="button"
          role="radio"
          aria-checked={kind === each}
          className="rounded border px-2 py-0.5 text-sm aria-checked:border-foreground aria-checked:font-semibold"
          onClick={() => {
            onKind(each);
          }}
        >
          {LABELS[each]}
          {each === "cursor" ? " (most people)" : ""}
        </button>
      ))}
    </div>
  );
}

/** What is installed in part: the Cursor editor without its agent, or Claude Code without its adapter. */
function HalfInstalled({ kinds }: { readonly kinds: readonly AgentKind[] }): JSX.Element | null {
  if (kinds.length === 0) {
    return null;
  }
  return (
    <div className="mb-2 space-y-1">
      {kinds.includes("cursor") && <p className="text-sm text-warn">The Cursor editor is installed, but not Cursor&rsquo;s agent, which is what Player Two runs. Set up Cursor installs it.</p>}
      {kinds.includes("claude") && (
        <div className="text-sm text-warn">
          <p>Claude Code is installed, but not its adapter for Player Two. In PowerShell, run:</p>
          <Command text={CLAUDE_INSTALL} />
        </div>
      )}
    </div>
  );
}

/** For someone with a WSL distro: agents inside it are found too. */
function InWsl(): JSX.Element {
  return (
    <details className="mt-3 text-sm">
      <summary className="cursor-pointer text-muted-foreground">Using WSL?</summary>
      <p className="mt-1">Install an agent inside your distro, then press Look again. For Cursor&rsquo;s, run this in the distro:</p>
      <Command text={CURSOR_IN_WSL} />
    </details>
  );
}

interface Props {
  readonly places: PlacesView;
  /** Sends a change, and shows the page again. */
  readonly onChange: (request: Request) => void;
  /** Looks for the agents again. */
  readonly onLook: () => void;
  /** Whether this is the step after joining, which checks the agent found at once. */
  readonly first: boolean;
}

export function AgentSetup({ places, onChange, onLook, first }: Props): JSX.Element {
  const [kind, setKind] = useState<AgentKind>(places.half_installed.includes("claude") ? "claude" : "cursor");
  const onWindows = places.agents.some((agent) => agent.runs_in === "windows");
  const wsl = places.distros.length > 0;
  return (
    <div>
      <HalfInstalled kinds={places.half_installed} />
      <ul>
        {places.agents.map((agent) => (
          <AgentRow
            key={`${agent.kind}:${agent.runs_in}`}
            agent={agent}
            wsl={wsl}
            alone={places.agents.filter((other) => other.runs_in === agent.runs_in).length === 1}
            checkNow={first}
            onChoose={() => {
              onChange({ type: "choose_agent", runs_in: agent.runs_in, kind: agent.kind });
            }}
          />
        ))}
      </ul>
      {!onWindows && (
        <div className="rounded-md border p-3">
          <p>No agent is installed on this computer yet. Which do you use?</p>
          <Choice kind={kind} onKind={setKind} />
          <Install kind={kind} onLook={onLook} />
        </div>
      )}
      <Button type="button" variant="outline" size="sm" className="mt-2" onClick={onLook}>
        Look again
      </Button>
      {places.wsl && <InWsl />}
    </div>
  );
}
