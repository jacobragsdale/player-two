/**
 * The message box: Enter sends, Shift+Enter starts a new line, and a message that could not be sent stays in the box with the reason under it. Under it, the mode the person's own agent runs at when
 * they tag it, which they can switch in one click; the switch becomes their default.
 */

import { useState } from "react";
import type { JSX, KeyboardEvent } from "react";

import { Textarea } from "@/components/ui/textarea";

import type { ChatId, Mode } from "./desk.ts";
import { ask, problem } from "./deskClient.ts";

const MODES: readonly { readonly mode: Mode; readonly label: string; readonly meaning: string }[] = [
  { mode: "read-only", label: "Read-only", meaning: "Your agent may read and search, and change nothing." },
  { mode: "ask", label: "Ask", meaning: "Each change or command your agent wants to make waits for your yes." },
  { mode: "full", label: "Full", meaning: "Your agent may do what it needs without asking." }
];

/** The mode the person's own agent runs at, as three buttons. */
function ModeSwitch({ mode, onMode }: { readonly mode: Mode; readonly onMode: (mode: Mode) => void }): JSX.Element {
  return (
    <div className="mt-1.5 flex items-center gap-1 text-xs text-muted-foreground" role="radiogroup" aria-label="Your agent's mode">
      <span>Your agent:</span>
      {MODES.map((each) => (
        <button
          key={each.mode}
          type="button"
          role="radio"
          aria-checked={mode === each.mode}
          title={each.meaning}
          className="rounded px-1.5 py-0.5 hover:bg-muted aria-checked:bg-muted aria-checked:font-semibold aria-checked:text-foreground"
          onClick={() => {
            onMode(each.mode);
          }}
        >
          {each.label}
        </button>
      ))}
    </div>
  );
}

interface Props {
  readonly chat: ChatId;
  readonly thread: number | null;
  readonly placeholder: string;
  /** The mode the person's own agent runs at: the default, until they switch it. */
  readonly mode: Mode;
}

export function Composer({ chat, thread, placeholder, mode: usual }: Props): JSX.Element {
  const [text, setText] = useState("");
  const [mode, setMode] = useState<Mode>(usual);
  const [sending, setSending] = useState(false);
  const [error, setError] = useState("");

  const send = (): void => {
    if (text.trim() === "" || sending) {
      return;
    }
    setSending(true);
    setError("");
    ask({ type: "send", chat, thread, text, mode: mode === usual ? null : mode }).then(
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
      <ModeSwitch mode={mode} onMode={setMode} />
    </div>
  );
}
