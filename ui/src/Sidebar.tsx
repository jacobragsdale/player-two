/** The chat list: the person's channels, then everyone else, with a dot for who is here and a count of unread messages. */

import { cn } from "cn";
import type { JSX, ReactNode } from "react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";

import type { ChatId, ChatSummary, Connection, Overview } from "./desk.ts";

const HEADING = "mx-2 mt-3 mb-1 text-[11px] font-semibold tracking-[0.06em] text-muted-foreground uppercase";
const ROW = "h-auto w-full justify-start gap-1.5 px-2 py-1 font-normal";

function Row({ summary, open, onOpen }: { readonly summary: ChatSummary; readonly open: boolean; readonly onOpen: (chat: ChatId) => void }): JSX.Element {
  return (
    <li>
      <Button
        type="button"
        variant="ghost"
        className={cn(ROW, "aria-[current=true]:bg-muted aria-[current=true]:font-semibold")}
        aria-current={open}
        onClick={() => {
          onOpen(summary.chat);
        }}
      >
        {summary.here !== null && (
          <span title={summary.here ? "here now" : "away"} className={cn("size-2 flex-none rounded-full border border-muted-foreground", summary.here && "border-emerald-600 bg-emerald-600")} />
        )}
        <span className={cn("flex-1 truncate text-left", summary.unread > 0 && "font-bold")}>{summary.title}</span>
        {summary.unread > 0 && <Badge>{summary.unread}</Badge>}
      </Button>
    </li>
  );
}

/** How the desk stands with the hub, in words. */
export function connectionLine(connection: Connection): string {
  switch (connection.state) {
    case "not_joined":
      return "not joined";
    case "connecting":
      return "◌ connecting";
    case "connected":
      return "● connected";
    case "offline":
      return `◌ ${connection.reason} · retrying`;
    case "wrong_version":
      return `the hub runs ${connection.hub}`;
  }
}

interface Props {
  readonly overview: Overview;
  readonly open: ChatId | null;
  readonly onOpen: (chat: ChatId) => void;
  /** Buttons under the channels: joining or making one. */
  readonly channelActions: ReactNode;
  readonly onSettings: () => void;
}

export function Sidebar({ overview, open, onOpen, channelActions, onSettings }: Props): JSX.Element {
  const channels = overview.chats.filter((summary) => summary.here === null);
  const people = overview.chats.filter((summary) => summary.here !== null);
  return (
    <nav className="flex h-full min-h-0 flex-col bg-sidebar">
      <div className="flex-1 overflow-y-auto p-2">
        <h2 className={HEADING}>Channels</h2>
        <ul>
          {channels.map((summary) => (
            <Row key={summary.chat} summary={summary} open={summary.chat === open} onOpen={onOpen} />
          ))}
        </ul>
        {channelActions}
        <h2 className={HEADING}>People</h2>
        <ul>
          {people.map((summary) => (
            <Row key={summary.chat} summary={summary} open={summary.chat === open} onOpen={onOpen} />
          ))}
        </ul>
        {people.length === 0 && <p className="mx-2 my-1 text-muted-foreground">Nobody else has joined yet.</p>}
      </div>
      <div className="flex items-center gap-1 border-t px-3 py-1.5">
        <p className="flex-1 truncate text-xs text-muted-foreground" title={connectionLine(overview.connection)}>
          {overview.me ?? ""} · {connectionLine(overview.connection)}
        </p>
        <Button type="button" variant="ghost" size="xs" onClick={onSettings}>
          Settings
        </Button>
      </div>
    </nav>
  );
}
