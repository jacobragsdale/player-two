/** A run under the message it answers: how it stands, what the agent did, the steps it waits on, and its answer. Everything an agent printed is drawn as text; its answer as safe Markdown. */

import { cn } from "cn";
import { useState } from "react";
import type { JSX } from "react";

import { Button } from "@/components/ui/button";

import type { Activity, ChatId, PromptView, RunView, Tag } from "./desk.ts";
import { ask, problem } from "./deskClient.ts";
import { Markdown } from "./Markdown.tsx";

const MODE_WORDS = { "read-only": "read-only", ask: "asks before changes", full: "full" } as const;

/** Where a run is, in words: someone's computer, a WSL distro of it, or one of their shared folders. */
function whereItRuns(run: RunView, me: string): string {
  const whose = run.owner === me ? "your" : `${run.owner}'s`;
  if (run.place !== "machine") {
    return `${whose} ${run.place}`;
  }
  return run.runs_in === "windows" ? `${whose} computer` : `${whose} computer (${run.runs_in.replace(/^wsl:/, "")})`;
}

/** The line that says how a run stands. */
function stateLine(run: RunView, me: string): string {
  const agent = `${run.owner === me ? "Your" : `${run.owner}'s`} agent`;
  const where = whereItRuns(run, me);
  switch (run.state) {
    case "working":
      return `${agent} is working on ${where} · ${MODE_WORDS[run.mode]}`;
    case "done":
      return `${agent} finished on ${where}`;
    case "stopped":
      return `${agent} was stopped on ${where}${run.reason === null ? "" : `: ${run.reason}`}`;
    case "failed":
      return `${agent} failed on ${where}${run.reason === null ? "" : `: ${run.reason}`}`;
  }
}

function Item({ item }: { readonly item: Activity }): JSX.Element {
  const line = (
    <>
      <span className="text-foreground">{item.verb}</span> {item.object}
      {item.outcome !== undefined && <span className="text-muted-foreground"> · {item.outcome}</span>}
    </>
  );
  return (
    <li className="text-xs">
      {item.output === undefined ? (
        <p className="truncate text-muted-foreground">{line}</p>
      ) : (
        <details>
          <summary className="cursor-pointer truncate text-muted-foreground">{line}</summary>
          <pre className="my-1 max-h-60 overflow-auto rounded bg-muted p-2 text-xs whitespace-pre-wrap">{item.output}</pre>
        </details>
      )}
    </li>
  );
}

function Prompt({ prompt, run, chat, me }: { readonly prompt: PromptView; readonly run: RunView; readonly chat: ChatId; readonly me: string }): JSX.Element {
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const answer = (allow: boolean): void => {
    setBusy(true);
    ask({ type: "answer", chat, thread: run.thread, run: run.id, prompt: prompt.id, allow }).then(
      () => {
        setBusy(false);
      },
      (failure: unknown) => {
        setBusy(false);
        setError(problem(failure));
      }
    );
  };
  if (prompt.allow !== null) {
    const by = prompt.by === me ? "you" : (prompt.by ?? "");
    return (
      <p className="text-xs text-muted-foreground">
        {prompt.allow ? "✓" : "✗"} {prompt.what} · {prompt.allow ? "allowed" : "refused"} by {by}
      </p>
    );
  }
  const waiting = run.state === "working";
  return (
    <div className="my-1 rounded-md border border-warn/60 p-2">
      <p className="text-sm">
        The agent wants to: <span className="font-mono text-xs">{prompt.what}</span>
      </p>
      {waiting && run.may_answer && (
        <div className="mt-1 flex gap-2">
          <Button
            type="button"
            size="xs"
            disabled={busy}
            onClick={() => {
              answer(true);
            }}
          >
            Allow
          </Button>
          <Button
            type="button"
            size="xs"
            variant="outline"
            disabled={busy}
            onClick={() => {
              answer(false);
            }}
          >
            Refuse
          </Button>
        </div>
      )}
      {waiting && !run.may_answer && <p className="text-xs text-muted-foreground">waiting for {run.owner} to answer</p>}
      {error !== "" && <p className="text-xs text-destructive">{error}</p>}
    </div>
  );
}

export function Run({ run, chat, me }: { readonly run: RunView; readonly chat: ChatId; readonly me: string }): JSX.Element {
  const [error, setError] = useState("");
  const stop = (): void => {
    ask({ type: "stop", run: run.id }).catch((failure: unknown) => {
      setError(problem(failure));
    });
  };
  return (
    <section className="mt-1 border-l-2 border-agent pl-3" aria-label="the agent's run">
      <div className="flex items-center gap-2">
        <p className={cn("text-xs", run.state === "failed" ? "text-destructive" : "text-muted-foreground")}>
          {run.state === "working" ? "◌ " : ""}
          {stateLine(run, me)}
        </p>
        {run.mine && run.state === "working" && (
          <Button type="button" variant="ghost" size="xs" onClick={stop}>
            Stop
          </Button>
        )}
      </div>
      {error !== "" && <p className="text-xs text-destructive">{error}</p>}
      {run.activity.length > 0 && (
        <ul className="my-1 space-y-0.5">
          {run.activity.map((item) => (
            <Item key={item.id} item={item} />
          ))}
        </ul>
      )}
      {run.prompts.map((prompt) => (
        <Prompt key={prompt.id} prompt={prompt} run={run} chat={chat} me={me} />
      ))}
      {run.result !== null && <Markdown text={run.result} />}
    </section>
  );
}

/** A line for each tag nobody has answered yet, with Withdraw for whoever asked. */
export function Waiting({
  tags,
  me,
  author,
  chat,
  request
}: {
  readonly tags: readonly Tag[];
  readonly me: string;
  readonly author: string;
  readonly chat: ChatId;
  readonly request: number;
}): JSX.Element | null {
  const [error, setError] = useState("");
  if (tags.length === 0) {
    return null;
  }
  const withdraw = (): void => {
    ask({ type: "withdraw", chat, request }).catch((failure: unknown) => {
      setError(problem(failure));
    });
  };
  return (
    <>
      {tags.map((tag) => (
        <p key={`${tag.person}/${tag.place}`} className="flex items-center gap-2 text-xs text-muted-foreground">
          <span>
            ◌{" "}
            {tag.person === me && author === me
              ? `your ${tag.place === "machine" ? "computer" : tag.place} was busy, so this is waiting; send it again when the run before it ends`
              : `waiting for ${tag.person === me ? "you" : tag.person} to approve`}
          </span>
          {author === me && tag.person !== me && (
            <Button type="button" variant="ghost" size="xs" onClick={withdraw}>
              Withdraw
            </Button>
          )}
        </p>
      ))}
      {error !== "" && <p className="text-xs text-destructive">{error}</p>}
    </>
  );
}
