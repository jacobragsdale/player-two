/** The joined window: the chat list on the left, the open chat in the middle, and an open thread on the right. */

import { useEffect, useState } from "react";
import type { JSX } from "react";

import { Chat } from "./Chat.tsx";
import type { ChatId } from "./desk.ts";
import { useOverview } from "./hooks.ts";
import { Sidebar } from "./Sidebar.tsx";

export function Joined(): JSX.Element {
  const overview = useOverview();
  const [open, setOpen] = useState<ChatId | null>(null);
  const [thread, setThread] = useState<number | null>(null);

  // Until the person picks one, the first chat is open.
  const first = overview?.chats[0]?.chat ?? null;
  useEffect(() => {
    if (open === null && first !== null) {
      setOpen(first);
    }
  }, [open, first]);

  if (overview === null) {
    return <main className="flex h-full items-center justify-center text-muted-foreground">Loading…</main>;
  }
  const me = overview.me ?? "";
  return (
    <main className="grid h-full grid-cols-[15rem_1fr]">
      <Sidebar
        overview={overview}
        open={open}
        onOpen={(chat) => {
          setOpen(chat);
          setThread(null);
        }}
        channelActions={null}
      />
      <div className="flex min-w-0">
        {open === null ? (
          <section className="flex flex-1 items-center justify-center text-muted-foreground">{overview.connection.state === "connected" ? "Pick a chat." : "Connecting…"}</section>
        ) : (
          <Chat key={open} chat={open} me={me} thread={null} onThread={setThread} onClose={null} />
        )}
        {open !== null && thread !== null && (
          <div className="flex w-96 flex-none border-l">
            <Chat
              key={`${open}/${String(thread)}`}
              chat={open}
              me={me}
              thread={thread}
              onThread={setThread}
              onClose={() => {
                setThread(null);
              }}
            />
          </div>
        )}
      </div>
    </main>
  );
}
