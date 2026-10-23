/** The step right after joining: setting up the agent that Player Two runs on this computer. It can be skipped, and comes back in Settings. */

import { useCallback, useEffect, useState } from "react";
import type { JSX } from "react";

import { Button } from "@/components/ui/button";

import { AgentSetup } from "./AgentSetup.tsx";
import type { PlacesView, Request } from "./desk.ts";
import { ask, problem } from "./deskClient.ts";

export function SetupStep({ onDone }: { readonly onDone: () => void }): JSX.Element {
  const [places, setPlaces] = useState<PlacesView | null>(null);
  const [error, setError] = useState("");
  const load = useCallback(() => {
    ask({ type: "places" }).then(
      (response) => {
        if (response.type === "places") {
          setPlaces(response);
        }
      },
      (failure: unknown) => {
        setError(problem(failure));
      }
    );
  }, []);
  useEffect(load, [load]);
  const change = (request: Request): void => {
    setError("");
    ask(request).then(load, (failure: unknown) => {
      setError(problem(failure));
    });
  };
  const ready = places?.agents.some((agent) => agent.runs_in === "windows") ?? false;
  return (
    <main className="mx-auto h-full w-full max-w-2xl overflow-y-auto p-6 pt-[8vh]">
      <h1 className="text-2xl font-bold">Set up your agent</h1>
      <p className="mt-2 text-muted-foreground">Player Two runs a coding agent on this computer when you ask it to, or when you approve a teammate&rsquo;s request. Nothing runs until you say so.</p>
      <div className="mt-4">{places === null ? <p className="text-muted-foreground">Looking for agents…</p> : <AgentSetup places={places} onChange={change} onLook={load} first />}</div>
      {error !== "" && (
        <p role="alert" className="mt-2 text-destructive">
          {error}
        </p>
      )}
      <Button type="button" variant={ready ? "default" : "ghost"} className="mt-6" onClick={onDone}>
        {ready ? "Continue to the chats" : "Skip for now"}
      </Button>
    </main>
  );
}
