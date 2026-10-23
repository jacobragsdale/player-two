/** One message: who wrote it and when, what it says, and how many replies its thread has. */

import type { JSX } from "react";

import { Button } from "@/components/ui/button";

import type { ChatId, Item, Mode } from "./desk.ts";
import { Approval } from "./Approval.tsx";
import { Run, Waiting } from "./Run.tsx";
import { Text } from "./Text.tsx";

/** The time of day, or the date for anything before today. */
export function when(ts: number): string {
  const at = new Date(ts);
  const today = new Date().toDateString() === at.toDateString();
  return today ? at.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" }) : at.toLocaleString(undefined, { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });
}

interface Props {
  readonly item: Item;
  readonly me: string;
  readonly chat: ChatId;
  readonly onThread: ((root: number) => void) | null;
  /** The person's places with a run going now. */
  readonly busy: readonly string[];
  /** The mode the person's own agent runs at now. */
  readonly mode: Mode;
}

export function Message({ item, me, chat, onThread, busy, mode }: Props): JSX.Element {
  const open = (): void => {
    onThread?.(item.id);
  };
  return (
    <article className="group px-4 py-1.5 hover:bg-muted/40" data-id={item.id}>
      <header className="flex items-baseline gap-2">
        <span className="font-semibold">{item.author === me ? "you" : item.author}</span>
        <time className="text-xs text-muted-foreground" dateTime={new Date(item.ts).toISOString()} title={new Date(item.ts).toLocaleString()}>
          {when(item.ts)}
        </time>
        {onThread !== null && item.replies === 0 && (
          <Button type="button" variant="ghost" size="xs" className="ml-auto h-5 text-muted-foreground opacity-0 group-hover:opacity-100 focus-visible:opacity-100" onClick={open}>
            Reply in thread
          </Button>
        )}
      </header>
      <Text text={item.text} />
      {item.runs.map((run) => (
        <Run key={run.id} run={run} chat={chat} me={me} />
      ))}
      <Waiting tags={item.waiting} me={me} author={item.author} chat={chat} request={item.id} busy={busy} mode={mode} />
      {item.author !== me &&
        item.waiting.filter((tag) => tag.person === me).map((tag) => <Approval key={tag.place} chat={chat} request={item.id} place={tag.place} asker={item.author} follows={null} />)}
      {item.follows !== null && item.follows.owner === me && item.follows.state !== "working" && (
        <Approval
          chat={chat}
          request={item.id}
          place={item.follows.place}
          asker={item.author}
          follows={{ mode: item.follows.mode, steps_by: item.follows.steps_by, keep_going: false, runs_in: item.follows.runs_in }}
        />
      )}
      {item.follows !== null && item.follows.owner !== me && item.runs.length === 0 && <p className="text-xs text-muted-foreground">◌ waiting for {item.follows.owner} to run it</p>}
      {item.unknown.map((line) => (
        <p key={line} className="text-xs text-muted-foreground">
          ? {line}
        </p>
      ))}
      {onThread !== null && item.replies > 0 && (
        <Button type="button" variant="link" size="xs" className="h-auto px-0 text-tag" onClick={open}>
          {item.replies} {item.replies === 1 ? "reply" : "replies"}
        </Button>
      )}
    </article>
  );
}
