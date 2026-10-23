/** One message: who wrote it and when, what it says, and how many replies its thread has. */

import type { JSX } from "react";

import { Button } from "@/components/ui/button";

import type { ChatId, Item } from "./desk.ts";
import { Run, Waiting } from "./Run.tsx";
import { Text } from "./Text.tsx";

/** The time of day, or the date for anything before today. */
export function when(ts: number): string {
  const at = new Date(ts);
  const today = new Date().toDateString() === at.toDateString();
  return today ? at.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" }) : at.toLocaleString(undefined, { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });
}

export function Message({ item, me, chat, onThread }: { readonly item: Item; readonly me: string; readonly chat: ChatId; readonly onThread: ((root: number) => void) | null }): JSX.Element {
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
      <Waiting tags={item.waiting} me={me} author={item.author} />
      {onThread !== null && item.replies > 0 && (
        <Button type="button" variant="link" size="xs" className="h-auto px-0 text-tag" onClick={open}>
          {item.replies} {item.replies === 1 ? "reply" : "replies"}
        </Button>
      )}
    </article>
  );
}
