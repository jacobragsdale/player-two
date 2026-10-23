/** The first-run page: one field for the join code someone sent, and the name, offered from the Windows user name. */

import { useState } from "react";
import type { JSX, SyntheticEvent } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";

import { ask, problem } from "./deskClient.ts";

const FIELD = "flex flex-col items-stretch gap-1.5";

export function FirstRun({ suggested, onJoined }: { readonly suggested: string | null; readonly onJoined: () => void }): JSX.Element {
  const [code, setCode] = useState("");
  const [name, setName] = useState(suggested ?? "");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  const join = (event: SyntheticEvent<HTMLFormElement>): void => {
    event.preventDefault();
    setBusy(true);
    setError("");
    ask({ type: "join", code, name }).then(
      () => {
        setBusy(false);
        onJoined();
      },
      (failure: unknown) => {
        setBusy(false);
        setError(problem(failure));
      }
    );
  };

  return (
    <form className="mx-auto flex w-96 max-w-[calc(100vw-2rem)] flex-col gap-4 pt-[12vh]" onSubmit={join}>
      <h1 className="text-2xl font-bold">Welcome to Player Two</h1>
      <p className="text-muted-foreground">Someone on your team sent you a join code. Paste it here to join their chat.</p>
      <Label className={FIELD}>
        Join code
        <Textarea
          className="font-mono text-xs font-normal"
          rows={3}
          value={code}
          onChange={(event) => {
            setCode(event.target.value);
          }}
          placeholder="p2-…"
          required
          autoFocus
        />
      </Label>
      <Label className={FIELD}>
        Your name
        <Input
          className="font-normal"
          value={name}
          onChange={(event) => {
            setName(event.target.value);
          }}
          required
        />
        <span className="text-xs font-normal text-muted-foreground">How your team will see you and mention you. Lowercase letters, digits and dashes.</span>
      </Label>
      {error !== "" && (
        <p role="alert" className="text-destructive">
          {error}
        </p>
      )}
      <Button type="submit" className="self-start" disabled={busy || code.trim() === "" || name.trim() === ""}>
        {busy ? "Joining…" : "Join"}
      </Button>
    </form>
  );
}
