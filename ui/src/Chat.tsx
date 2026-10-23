/** The open chat, or a thread of it: who is in it, its messages with a line where the unread ones start, and the message box. */

import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { JSX } from "react";

import { Button } from "@/components/ui/button";

import { Composer } from "./Composer.tsx";
import type { ChatId } from "./desk.ts";
import { ask, problem } from "./deskClient.ts";
import { useChat } from "./hooks.ts";
import { Message } from "./Message.tsx";

const NEAR_BOTTOM = 80; // Pixels from the end within which new messages keep the view at the end.

interface Props {
  readonly chat: ChatId;
  readonly me: string;
  /** The thread shown, by its first message; null for the chat's top level. */
  readonly thread: number | null;
  readonly onThread: (root: number) => void;
  readonly onClose: (() => void) | null;
}

/** The chat's name, or "Thread", with who else is in a channel, and the button that closes a thread. */
function Header({ title, others, onClose }: { readonly title: string; readonly others: readonly string[] | null; readonly onClose: (() => void) | null }): JSX.Element {
  return (
    <header className="flex items-center gap-2 border-b px-4 py-2">
      <h2 className="font-semibold">{title}</h2>
      {others !== null && <span className="truncate text-xs text-muted-foreground">{others.length === 0 ? "just you" : `you, ${others.join(", ")}`}</span>}
      {onClose !== null && (
        <Button type="button" variant="ghost" size="xs" className="ml-auto" onClick={onClose} aria-label="Close the thread">
          ✕
        </Button>
      )}
    </header>
  );
}

/** The button that fetches the page before the oldest message shown. */
function Older({ chat }: { readonly chat: ChatId }): JSX.Element {
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const older = (): void => {
    setLoading(true);
    ask({ type: "older", chat }).then(
      () => {
        setLoading(false);
      },
      (failure: unknown) => {
        setLoading(false);
        setError(problem(failure));
      }
    );
  };
  return (
    <div className="px-4 pb-2">
      <Button type="button" variant="ghost" size="xs" disabled={loading} onClick={older}>
        {loading ? "Loading…" : "Show older messages"}
      </Button>
      {error !== "" && <p className="text-xs text-destructive">{error}</p>}
    </div>
  );
}

export function Chat({ chat, me, thread, onThread, onClose }: Props): JSX.Element {
  const view = useChat(chat, thread);
  const list = useRef<HTMLDivElement>(null);
  const atBottom = useRef(true);
  const count = view?.items.length ?? 0;

  // Keep to the end as messages arrive, unless the person scrolled up to read.
  useLayoutEffect(() => {
    const element = list.current;
    if (element !== null && atBottom.current) {
      element.scrollTop = element.scrollHeight;
    }
  }, [count, chat, thread]);

  // What is on screen while the window has focus is read.
  const unread = view?.first_unread ?? null;
  useEffect(() => {
    const read = (): void => {
      if (unread !== null && document.hasFocus()) {
        ask({ type: "read", chat }).catch((failure: unknown) => {
          console.error(failure);
        });
      }
    };
    read();
    window.addEventListener("focus", read);
    return () => {
      window.removeEventListener("focus", read);
    };
  }, [chat, unread]);

  if (view === null) {
    return <section className="flex flex-1 items-center justify-center text-muted-foreground">Loading…</section>;
  }
  const others = view.members.filter((member) => member !== me);
  return (
    <section className="flex min-w-0 flex-1 flex-col" aria-label={thread === null ? view.title : "thread"}>
      <Header title={thread === null ? view.title : "Thread"} others={thread === null && view.chat.startsWith("#") ? others : null} onClose={onClose} />
      <div
        ref={list}
        className="flex-1 overflow-y-auto py-2"
        onScroll={(event) => {
          const element = event.currentTarget;
          atBottom.current = element.scrollHeight - element.scrollTop - element.clientHeight < NEAR_BOTTOM;
        }}
      >
        {thread === null && view.has_older && <Older chat={chat} />}
        {view.items.length === 0 && <p className="px-4 text-muted-foreground">No messages yet. Say hello.</p>}
        {view.items.map((item) => (
          <div key={item.id}>
            {item.id === view.first_unread && (
              <p className="mx-4 my-1 flex items-center gap-2 text-xs text-warn before:flex-1 before:border-t before:border-warn after:flex-1 after:border-t after:border-warn">new</p>
            )}
            <Message item={item} me={me} onThread={thread === null ? onThread : null} />
          </div>
        ))}
      </div>
      <Composer chat={chat} thread={thread} placeholder={thread === null ? `Message ${view.title}` : "Reply in the thread"} />
    </section>
  );
}
