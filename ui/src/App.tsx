/** The window: the first-run page until this machine has joined a hub, the agent's setup just after, then the chats. */

import { useCallback, useEffect, useState } from "react";
import type { JSX } from "react";

import { ask } from "./deskClient.ts";
import { FirstRun } from "./FirstRun.tsx";
import { useDeskEvents } from "./hooks.ts";
import { Joined } from "./Joined.tsx";
import { SetupStep } from "./SetupStep.tsx";

type Setup = { readonly joined: string | null; readonly suggested: string | null } | null;

export function App(): JSX.Element {
  const [setup, setSetup] = useState<Setup>(null);
  // Set once the person joins here, until they finish or skip setting up their agent.
  const [fresh, setFresh] = useState(false);
  const check = useCallback(() => {
    ask({ type: "setup" }).then(
      (response) => {
        if (response.type === "setup") {
          setSetup({ joined: response.joined, suggested: response.suggested_name });
        }
      },
      (error: unknown) => {
        console.error(error);
      }
    );
  }, []);
  useEffect(check, [check]);
  // A reset on the hub sends the desk back to joining; the overview says so.
  useDeskEvents((event) => {
    if (event.type === "overview") {
      check();
    }
  });

  if (setup === null) {
    return <main className="flex h-full items-center justify-center text-muted-foreground">Starting…</main>;
  }
  if (setup.joined === null) {
    return (
      <FirstRun
        suggested={setup.suggested}
        onJoined={() => {
          setFresh(true);
          check();
        }}
      />
    );
  }
  if (fresh) {
    return (
      <SetupStep
        onDone={() => {
          setFresh(false);
        }}
      />
    );
  }
  return <Joined />;
}
