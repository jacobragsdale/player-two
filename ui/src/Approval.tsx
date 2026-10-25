/**
 * The card the owner of a place sees under a teammate's request, or under a follow-up of a run of theirs: where it runs when there is a choice, whether the asker may keep going, that the agent
 * then asks nobody and everyone in the chat sees what it does, and Approve or Decline.
 */

import { useEffect, useState } from "react";
import type { JSX } from "react";

import { Button } from "@/components/ui/button";

import type { ChatId, RunsIn } from "./desk.ts";
import { ask, problem } from "./deskClient.ts";

/** Where something runs, in words. */
function system(runsIn: RunsIn): string {
  return runsIn === "windows" ? "Windows" : runsIn.replace(/^wsl:/, "");
}

export interface Settings {
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

/** What the owner should know before approving: what the agent may do, who sees it, what keep going means, and changes not committed. */
function Warnings({ keepGoing, asker, dirty }: { readonly keepGoing: boolean; readonly asker: string; readonly dirty: boolean }): JSX.Element {
  return (
    <>
      <p className="text-xs text-muted-foreground">
        It can change files and run commands as you, without asking. Everything it reads or prints is shown in this chat.
        {keepGoing ? ` ${asker}'s follow-ups here run without asking you, until you press Stop or the thread is quiet for an hour.` : ""}
      </p>
      {dirty && <p className="text-xs text-warn">This folder has changes not committed to git; the agent works among them.</p>}
    </>
  );
}

export function Approval({ chat, request, place, asker, follows }: Props): JSX.Element {
  // Unticked every time: keep going is given to one asker at a time, by a click of its own.
  const [keepGoing, setKeepGoing] = useState(false);
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
    const done = approve ? ask({ type: "approve", chat, request, place, runs_in: runsIn ?? "windows", keep_going: keepGoing }) : ask({ type: "decline", chat, request, place });
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
      <Check label={`Let ${asker} keep going in this thread`} checked={keepGoing} onChange={setKeepGoing} />
      <Warnings keepGoing={keepGoing} asker={asker} dirty={dirty} />
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
