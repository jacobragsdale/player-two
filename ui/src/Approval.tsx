/**
 * The card the owner of a place sees under a teammate's request, or under a follow-up of a run of theirs: how much the agent may do, where it runs when there is a choice, whether the asker
 * answers its steps and may keep going, what everyone in the chat will see, and Approve or Decline.
 */

import { useEffect, useState } from "react";
import type { JSX } from "react";

import { Button } from "@/components/ui/button";

import type { ChatId, Mode, RunsIn, StepsBy } from "./desk.ts";
import { ask, problem } from "./deskClient.ts";

const MODES: readonly { readonly mode: Mode; readonly label: string }[] = [
  { mode: "read-only", label: "Read-only" },
  { mode: "ask", label: "Ask before changes" },
  { mode: "full", label: "Full" }
];

/** Where something runs, in words. */
function system(runsIn: RunsIn): string {
  return runsIn === "windows" ? "Windows" : runsIn.replace(/^wsl:/, "");
}

export interface Settings {
  readonly mode: Mode;
  readonly steps_by: StepsBy;
  readonly keep_going: boolean;
  readonly runs_in: RunsIn | null;
}

interface Props {
  readonly chat: ChatId;
  readonly request: number;
  readonly place: string;
  readonly asker: string;
  /** For a follow-up, the settings of the run it follows; for a new request, null. */
  readonly follows: Settings | null;
}

function Check({ label, checked, onChange }: { readonly label: string; readonly checked: boolean; readonly onChange: (checked: boolean) => void }): JSX.Element {
  return (
    <label className="flex items-center gap-2 text-sm">
      <input
        type="checkbox"
        checked={checked}
        onChange={(event) => {
          onChange(event.target.checked);
        }}
      />
      {label}
    </label>
  );
}

/** The three modes, as buttons. */
function ModeChoice({ mode, onMode }: { readonly mode: Mode; readonly onMode: (mode: Mode) => void }): JSX.Element {
  return (
    <div className="flex flex-wrap gap-1" role="radiogroup" aria-label="How much the agent may do">
      {MODES.map((each) => (
        <button
          key={each.mode}
          type="button"
          role="radio"
          aria-checked={mode === each.mode}
          className="rounded border px-2 py-0.5 text-xs aria-checked:border-foreground aria-checked:font-semibold"
          onClick={() => {
            onMode(each.mode);
          }}
        >
          {each.label}
        </button>
      ))}
    </div>
  );
}

/** What the owner should know before approving: who sees what, what full and keep going mean, and changes not committed. */
function Warnings({ mode, keepGoing, asker, dirty }: { readonly mode: Mode; readonly keepGoing: boolean; readonly asker: string; readonly dirty: boolean }): JSX.Element {
  return (
    <>
      <p className="text-xs text-muted-foreground">
        Everything the agent reads or prints is shown in this chat.
        {mode === "full" ? " In full mode it changes things without asking anyone." : ""}
        {keepGoing ? ` ${asker}'s follow-ups here run without asking you, until you press Stop or the thread is quiet for an hour.` : ""}
      </p>
      {dirty && <p className="text-xs text-warn">This checkout has changes you have not committed; the agent works among them.</p>}
    </>
  );
}

export function Approval({ chat, request, place, asker, follows }: Props): JSX.Element {
  const [mode, setMode] = useState<Mode>(follows?.mode ?? "ask");
  const [handOver, setHandOver] = useState(follows?.steps_by === "asker");
  const [keepGoing, setKeepGoing] = useState(follows?.keep_going ?? false);
  const [choices, setChoices] = useState<readonly RunsIn[]>([]);
  const [runsIn, setRunsIn] = useState<RunsIn | null>(follows?.runs_in ?? null);
  const [dirty, setDirty] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    ask({ type: "approval", place }).then(
      (response) => {
        if (response.type === "approval") {
          setChoices(response.choices);
          setRunsIn((chosen) => chosen ?? response.runs_in);
          setDirty(response.dirty);
        }
      },
      (failure: unknown) => {
        setError(problem(failure));
      }
    );
  }, [place]);

  const act = (approve: boolean): void => {
    setBusy(true);
    setError("");
    const done = approve
      ? ask({ type: "approve", chat, request, place, mode, runs_in: runsIn ?? "windows", steps_by: mode === "ask" && handOver ? "asker" : "owner", keep_going: keepGoing })
      : ask({ type: "decline", chat, request });
    done.then(
      () => {
        setBusy(false);
      },
      (failure: unknown) => {
        setBusy(false);
        setError(problem(failure));
      }
    );
  };

  const where = place === "machine" ? "your computer" : place;
  return (
    <section className="my-1 space-y-2 rounded-md border border-warn/60 p-3" aria-label="Approve or decline">
      <p className="text-sm">
        <span className="font-semibold">{asker}</span> {follows === null ? `asks your agent for help on ${where}.` : "follows up. Run it too?"}
      </p>
      <ModeChoice mode={mode} onMode={setMode} />
      {choices.length > 1 && (
        <label className="flex items-center gap-2 text-sm">
          Run on
          <select
            value={runsIn ?? ""}
            onChange={(event) => {
              setRunsIn(event.target.value);
            }}
          >
            {choices.map((choice) => (
              <option key={choice} value={choice}>
                {system(choice)}
              </option>
            ))}
          </select>
        </label>
      )}
      {mode === "ask" && <Check label={`Let ${asker} approve the steps`} checked={handOver} onChange={setHandOver} />}
      <Check label={`Let ${asker} keep going in this thread`} checked={keepGoing} onChange={setKeepGoing} />
      <Warnings mode={mode} keepGoing={keepGoing} asker={asker} dirty={dirty} />
      <div className="flex gap-2">
        <Button
          type="button"
          size="sm"
          disabled={busy}
          onClick={() => {
            act(true);
          }}
        >
          Approve
        </Button>
        <Button
          type="button"
          size="sm"
          variant="outline"
          disabled={busy}
          onClick={() => {
            act(false);
          }}
        >
          Decline
        </Button>
      </div>
      {error !== "" && <p className="text-xs text-destructive">{error}</p>}
    </section>
  );
}
