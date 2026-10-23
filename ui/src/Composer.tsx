/** The message box: Enter sends, Shift+Enter starts a new line, and a message that could not be sent stays in the box with the reason under it. */

import { useState } from "react";
import type { JSX, KeyboardEvent } from "react";

import { Textarea } from "@/components/ui/textarea";

import type { ChatId } from "./desk.ts";
import { ask, problem } from "./deskClient.ts";

export function Composer({ chat, thread, placeholder }: { readonly chat: ChatId; readonly thread: number | null; readonly placeholder: string }): JSX.Element {
  const [text, setText] = useState("");
  const [sending, setSending] = useState(false);
  const [error, setError] = useState("");

  const send = (): void => {
    if (text.trim() === "" || sending) {
      return;
    }
    setSending(true);
    setError("");
    ask({ type: "send", chat, thread, text }).then(
      () => {
        setSending(false);
        setText("");
      },
      (failure: unknown) => {
        setSending(false);
        setError(problem(failure));
      }
    );
  };

  const onKey = (event: KeyboardEvent<HTMLTextAreaElement>): void => {
    if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
      event.preventDefault();
      send();
    }
  };

  return (
    <div className="border-t p-3">
      <Textarea
        className="max-h-48 min-h-10 resize-none"
        rows={Math.min(8, text.split("\n").length)}
        value={text}
        placeholder={placeholder}
        aria-label={placeholder}
        onChange={(event) => {
          setText(event.target.value);
        }}
        onKeyDown={onKey}
      />
      {error !== "" && (
        <p role="alert" className="mt-1 text-xs text-destructive">
          not sent: {error}
        </p>
      )}
    </div>
  );
}
