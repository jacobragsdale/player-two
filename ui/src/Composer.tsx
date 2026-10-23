/**
 * The message box: Enter sends, Shift+Enter starts a new line, and a message that could not be sent stays in the box with the reason under it. Under it, the mode the person's own agent runs at when
 * they tag it, which they can switch in one click; the switch becomes their default.
 */

import { useRef, useState } from "react";
import type { JSX, KeyboardEvent } from "react";

import { Textarea } from "@/components/ui/textarea";

import type { ChatId, Mode, Person } from "./desk.ts";
import { ask, problem } from "./deskClient.ts";
import { candidates, typedTag } from "./tagList.ts";

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
  /** Everyone who can be tagged, and who is in this chat. */
  readonly people: readonly Person[];
  readonly members: readonly string[];
}

/** The list that opens on `@`: what can be tagged, matching what was typed. */
function TagList({ tags, picked, onPick }: { readonly tags: readonly string[]; readonly picked: number; readonly onPick: (tag: string) => void }): JSX.Element {
  return (
    <ul role="listbox" aria-label="Who to tag" className="absolute bottom-full left-0 mb-1 w-72 rounded-md border bg-popover p-1 text-sm shadow-md">
      {tags.map((tag, at) => (
        <li key={tag} role="option" aria-selected={at === picked}>
          <button
            type="button"
            className="w-full rounded px-2 py-1 text-left aria-selected:bg-muted"
            aria-selected={at === picked}
            onMouseDown={(event) => {
              // Keep the focus in the box.
              event.preventDefault();
              onPick(tag);
            }}
          >
            @{tag}
          </button>
        </li>
      ))}
    </ul>
  );
}

export function Composer({ chat, thread, placeholder, mode: usual, people, members }: Props): JSX.Element {
  const [text, setText] = useState("");
  const [caret, setCaret] = useState(0);
  const [picked, setPicked] = useState(0);
  const [closed, setClosed] = useState(false);
  const box = useRef<HTMLTextAreaElement>(null);
  const typed = closed ? null : typedTag(text.slice(0, caret));
  const tags = typed === null ? [] : candidates(typed, people, members);

  /** Puts the chosen tag in place of the part typed, with a space after it. */
  const pick = (tag: string): void => {
    if (typed === null) {
      return;
    }
    const start = caret - typed.length;
    const next = `${text.slice(0, start)}${tag} ${text.slice(caret)}`;
    setText(next);
    setCaret(start + tag.length + 1);
    setPicked(0);
    requestAnimationFrame(() => {
      box.current?.setSelectionRange(start + tag.length + 1, start + tag.length + 1);
    });
  };
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
    if (tags.length > 0 && listKey(event)) {
      return;
    }
    if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
      event.preventDefault();
      send();
    }
  };

  /** The list's own keys while it is open: arrows move, Enter or Tab picks, Esc closes. True when the key was the list's. */
  const listKey = (event: KeyboardEvent<HTMLTextAreaElement>): boolean => {
    const choose = (): void => {
      const tag = tags[picked];
      if (tag !== undefined) {
        pick(tag);
      }
    };
    const keys: Readonly<Record<string, () => void>> = {
      ArrowDown: () => {
        setPicked((picked + 1) % tags.length);
      },
      ArrowUp: () => {
        setPicked((picked - 1 + tags.length) % tags.length);
      },
      Enter: choose,
      Tab: choose,
      Escape: () => {
        setClosed(true);
      }
    };
    const action = Object.hasOwn(keys, event.key) ? keys[event.key] : undefined;
    if (action === undefined || event.shiftKey) {
      return false;
    }
    event.preventDefault();
    action();
    return true;
  };

  return (
    <div className="relative border-t p-3">
      {tags.length > 0 && <TagList tags={tags} picked={Math.min(picked, tags.length - 1)} onPick={pick} />}
      <Textarea
        ref={box}
        className="max-h-48 min-h-10 resize-none"
        rows={Math.min(8, text.split("\n").length)}
        value={text}
        placeholder={placeholder}
        aria-label={placeholder}
        onChange={(event) => {
          setText(event.target.value);
          setCaret(event.target.selectionStart);
          setClosed(false);
          setPicked(0);
        }}
        onSelect={(event) => {
          setCaret(event.currentTarget.selectionStart);
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
