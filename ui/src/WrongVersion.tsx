/** What the window shows instead of the chats when the hub runs another version than this app: the two versions, and the button that installs the hub's, which asks first while a run is going here. */

import { useState } from "react";
import type { JSX } from "react";

import { Button } from "@/components/ui/button";

import { ask, problem } from "./deskClient.ts";

export function WrongVersion({ hub, app, running }: { readonly hub: string; readonly app: string; readonly running: readonly string[] }): JSX.Element {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [asked, setAsked] = useState(false);
  const update = (): void => {
    // Updating restarts the app, which stops whatever its agent is doing.
    if (running.length > 0 && !asked) {
      setAsked(true);
      return;
    }
    setBusy(true);
    setError("");
    // When it works the app restarts, so only a failure comes back.
    ask({ type: "update" }).catch((failure: unknown) => {
      setBusy(false);
      setError(problem(failure));
    });
  };
  return (
    <main className="mx-auto flex w-96 max-w-[calc(100vw-2rem)] flex-col gap-3 pt-[15vh]">
      <h1 className="text-xl font-bold">Player Two needs updating</h1>
      <p>
        Your team&rsquo;s hub runs version {hub}, and this app is version {app}. Update to chat again; it takes a minute, and Player Two opens again by itself.
      </p>
      {asked && running.length > 0 && <p className="text-warn">Updating stops what your agent is doing: {running.join("; ")}.</p>}
      <Button type="button" className="self-start" disabled={busy} onClick={update}>
        {busy ? "Updating…" : asked && running.length > 0 ? "Stop it and update" : `Update to ${hub}`}
      </Button>
      {error !== "" && (
        <p role="alert" className="text-destructive">
          {error}
        </p>
      )}
    </main>
  );
}
