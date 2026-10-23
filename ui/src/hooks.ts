import { useCallback, useEffect, useRef, useState } from "react";

import type { ChatId, ChatView, Event, Overview } from "./desk.ts";
import { ask, onDesk } from "./deskClient.ts";

/** Calls `handler` with each event the desk sends, for as long as the component is there. */
export function useDeskEvents(handler: (event: Event) => void): void {
  const latest = useRef(handler);
  useEffect(() => {
    latest.current = handler;
  });
  useEffect(
    () =>
      onDesk((event) => {
        latest.current(event);
      }),
    []
  );
}

/** The chat list, the people and the connection, kept up to date. */
export function useOverview(): Overview | null {
  const [overview, setOverview] = useState<Overview | null>(null);
  const load = useCallback(() => {
    ask({ type: "overview" }).then(
      (response) => {
        if (response.type === "overview") {
          setOverview(response);
        }
      },
      (error: unknown) => {
        console.error(error);
      }
    );
  }, []);
  useEffect(load, [load]);
  useDeskEvents((event) => {
    if (event.type === "overview") {
      load();
    }
  });
  return overview;
}

/** One chat, or one thread of it, kept up to date. */
export function useChat(chat: ChatId, thread: number | null): ChatView | null {
  const [view, setView] = useState<ChatView | null>(null);
  const load = useCallback(() => {
    ask({ type: "chat", chat, thread }).then(
      (response) => {
        if (response.type === "chat") {
          setView(response);
        }
      },
      // The chats load just after the window does; the overview's change asks again.
      () => {
        setView(null);
      }
    );
  }, [chat, thread]);
  useEffect(load, [load]);
  useDeskEvents((event) => {
    if ((event.type === "chat" && event.chat === chat) || event.type === "overview") {
      load();
    }
  });
  return view;
}
