/** The joined window: the chat list on the left, the open chat in the middle, and an open thread on the right. */

import { useEffect, useState } from "react";
import type { JSX } from "react";

import { Button } from "@/components/ui/button";

import { ChannelDialog } from "./Channels.tsx";
import { Chat } from "./Chat.tsx";
import type { ChatId } from "./desk.ts";
import { useDeskEvents, useOverview } from "./hooks.ts";
import { Sidebar } from "./Sidebar.tsx";
import { WrongVersion } from "./WrongVersion.tsx";

/** The app's version, which is the desk's and the hub's too when all is well. */
const APP_VERSION = __APP_VERSION__;

export function Joined(): JSX.Element {
  const overview = useOverview();
  const [open, setOpen] = useState<ChatId | null>(null);
  const [thread, setThread] = useState<number | null>(null);
  const [choosing, setChoosing] = useState(false);

  // A clicked notification opens its chat.
  useDeskEvents((event) => {
    if (event.type === "open") {
      setOpen(event.chat);
      setThread(null);
    }
  });

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
  if (overview.connection.state === "wrong_version") {
    return <WrongVersion hub={overview.connection.hub} app={APP_VERSION} />;
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
        channelActions={
          <Button
            type="button"
            variant="ghost"
            className="h-auto w-full justify-start px-2 py-1 font-normal text-muted-foreground"
            onClick={() => {
              setChoosing(true);
            }}
          >
            + Join or create
          </Button>
        }
      />
      {choosing && (
        <ChannelDialog
          me={me}
          channels={overview.channels}
          onDone={(chat) => {
            setChoosing(false);
            if (chat !== null) {
              setOpen(chat);
              setThread(null);
            }
          }}
        />
      )}
      <div className="flex min-w-0">
        {open === null ? (
          <section className="flex flex-1 items-center justify-center text-muted-foreground">{overview.connection.state === "connected" ? "Pick a chat." : "Connecting…"}</section>
        ) : (
          <Chat
            key={open}
            chat={open}
            me={me}
            thread={null}
            onThread={setThread}
            onClose={null}
            onLeft={() => {
              setOpen(null);
              setThread(null);
            }}
          />
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
              onLeft={null}
            />
          </div>
        )}
      </div>
    </main>
  );
}
