/** The window: the first-run page until this machine has joined a hub, then the chats. */

import { useCallback, useEffect, useState } from "react";
import type { JSX } from "react";

import { ask } from "./deskClient.ts";
import { FirstRun } from "./FirstRun.tsx";
import { useDeskEvents } from "./hooks.ts";

type Setup = { readonly joined: string | null; readonly suggested: string | null } | null;

export function App(): JSX.Element {
  const [setup, setSetup] = useState<Setup>(null);
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
    return <FirstRun suggested={setup.suggested} onJoined={check} />;
  }
  return <main className="flex h-full items-center justify-center">Joined as {setup.joined}</main>;
}
