/** The joined window: the chat list on the left, the open chat in the middle, and an open thread on the right. */

import { useEffect, useState } from "react";
import type { JSX } from "react";

import { Button } from "@/components/ui/button";

import { ChannelDialog } from "./Channels.tsx";
import { Chat } from "./Chat.tsx";
import type { ChatId } from "./desk.ts";
import { useDeskEvents, useOverview } from "./hooks.ts";
import { Settings } from "./Settings.tsx";
import { Sidebar } from "./Sidebar.tsx";
import { WrongVersion } from "./WrongVersion.tsx";

/** The app's version, which is the desk's and the hub's too when all is well. */
const APP_VERSION = __APP_VERSION__;

/** Where the chat last open is kept, in this window's own storage. */
const LAST_OPEN = "player-two.open";

/** The chat last open, if the window's storage can say. */
function lastOpen(): string | null {
  try {
    return localStorage.getItem(LAST_OPEN);
  } catch {
    return null;
  }
}

/** Keeps the chat open now, for the next start. Storage that refuses only costs that. */
function keepOpen(chat: ChatId): void {
  try {
    localStorage.setItem(LAST_OPEN, chat);
  } catch {
    // The window then opens on the first chat, as it would on a first start.
  }
}

export function Joined(): JSX.Element {
  const overview = useOverview();
  const [open, setOpen] = useState<ChatId | null>(null);
  const [thread, setThread] = useState<number | null>(null);
  const [choosing, setChoosing] = useState(false);
  const [settings, setSettings] = useState(false);

  // A clicked notification opens its chat, and Settings in the tray's menu opens Settings.
  useDeskEvents((event) => {
    if (event.type === "open") {
      setSettings(false);
      setOpen(event.chat);
      setThread(event.thread ?? null);
    } else if (event.type === "settings") {
      setSettings(true);
    }
  });

  // Until the person picks one, the chat open last time is open, or else the first.
  const chats = overview?.chats;
  useEffect(() => {
    if (open === null && chats !== undefined) {
      const last = lastOpen();
      const chosen = chats.find((summary) => summary.chat === last) ?? chats[0];
      if (chosen !== undefined) {
        setOpen(chosen.chat);
      }
    }
  }, [open, chats]);
  useEffect(() => {
    if (open !== null) {
      keepOpen(open);
    }
  }, [open]);

  if (overview === null) {
    return <main className="flex h-full items-center justify-center text-muted-foreground">Loading…</main>;
  }
  if (overview.connection.state === "wrong_version") {
    return <WrongVersion hub={overview.connection.hub} app={APP_VERSION} />;
  }
  const me = overview.me ?? "";
  if (settings) {
    return (
      <Settings
        me={me}
        onBack={() => {
          setSettings(false);
        }}
      />
    );
  }
  return (
    <main className="grid h-full grid-cols-[15rem_1fr] overflow-hidden">
      <Sidebar
        overview={overview}
        open={open}
        onOpen={(chat) => {
          setOpen(chat);
          setThread(null);
        }}
        onSettings={() => {
          setSettings(true);
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
      <div className="flex min-h-0 min-w-0">
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
            mode={overview.default_mode}
            people={overview.people}
            busy={overview.busy}
          />
        )}
        {open !== null && thread !== null && (
          <div className="flex min-h-0 w-96 flex-none border-l">
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
              mode={overview.default_mode}
              people={overview.people}
              busy={overview.busy}
            />
          </div>
        )}
      </div>
    </main>
  );
}
