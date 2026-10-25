/** A run under the message it answers: how it stands, what the agent did, and its answer. Everything an agent printed is drawn as text; its answer as safe Markdown. */

import { cn } from "cn";
import { useState } from "react";
import type { JSX } from "react";

import { Button } from "@/components/ui/button";

import type { Activity, ChatId, RunView, Tag } from "./desk.ts";
import { ask, problem } from "./deskClient.ts";
import { Markdown } from "./Markdown.tsx";

/** How many of its latest steps a working run shows. */
const LATEST = 5;

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
      return `${agent} is working on ${where}`;
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

/** What a run did: its latest steps while it works and none once it ends, with the rest a click away. */
function Steps({ activity, working }: { readonly activity: readonly Activity[]; readonly working: boolean }): JSX.Element | null {
  const [all, setAll] = useState(false);
  const shown = all ? activity : working ? activity.slice(-LATEST) : [];
  if (activity.length === 0) {
    return null;
  }
  return (
    <>
      {shown.length > 0 && (
        <ul className="my-1 space-y-0.5">
          {shown.map((item) => (
            <Item key={item.id} item={item} />
          ))}
        </ul>
      )}
      {shown.length < activity.length && (
        <Button
          type="button"
          variant="link"
          size="xs"
          className="h-auto px-0 text-muted-foreground"
          onClick={() => {
            setAll(true);
          }}
        >
          Show all {activity.length} {activity.length === 1 ? "step" : "steps"}
        </Button>
      )}
      {all && (working ? activity.length > LATEST : activity.length > 0) && (
        <Button
          type="button"
          variant="link"
          size="xs"
          className="h-auto px-0 text-muted-foreground"
          onClick={() => {
            setAll(false);
          }}
        >
          Show fewer
        </Button>
      )}
    </>
  );
}

export function Run({ run, chat, me }: { readonly run: RunView; readonly chat: ChatId; readonly me: string }): JSX.Element {
  const [error, setError] = useState("");
  const stop = (): void => {
    ask({ type: "stop", chat, thread: run.thread, run: run.id }).catch((failure: unknown) => {
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
        {run.may_stop && run.state === "working" && (
          <Button type="button" variant="ghost" size="xs" onClick={stop}>
            Stop
          </Button>
        )}
      </div>
      {error !== "" && <p className="text-xs text-destructive">{error}</p>}
      <Steps activity={run.activity} working={run.state === "working"} />
      {run.result !== null && <Markdown text={run.result} />}
    </section>
  );
}

/** The line under a tag of the person's own that found its place busy: Run now, once the place is free. */
function Busy({ tag, chat, request, busy }: { readonly tag: Tag; readonly chat: ChatId; readonly request: number; readonly busy: boolean }): JSX.Element {
  const [error, setError] = useState("");
  const place = tag.place === "machine" ? "computer" : tag.place;
  const runNow = (): void => {
    setError("");
    ask({ type: "run_now", chat, request, place: tag.place }).catch((failure: unknown) => {
      setError(problem(failure));
    });
  };
  return (
    <>
      <p className="flex items-center gap-2 text-xs text-muted-foreground">
        <span>◌ {busy ? `your ${place} is busy, so this is waiting until the run before it ends` : `your ${place} was busy, so this waited`}</span>
        {!busy && (
          <Button type="button" variant="ghost" size="xs" onClick={runNow}>
            Run now
          </Button>
        )}
      </p>
      {error !== "" && <p className="text-xs text-destructive">{error}</p>}
    </>
  );
}

interface WaitingProps {
  readonly tags: readonly Tag[];
  readonly me: string;
  readonly author: string;
  readonly chat: ChatId;
  readonly request: number;
  /** The person's places with a run going now. */
  readonly busy: readonly string[];
}

/** A line for each tag nobody has answered yet, with Withdraw for whoever asked, and Run now for a tag of the person's own that found its place busy. */
export function Waiting({ tags, me, author, chat, request, busy }: WaitingProps): JSX.Element | null {
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
      {tags.map((tag) =>
        tag.person === me && author === me ? (
          <Busy key={`${tag.person}/${tag.place}`} tag={tag} chat={chat} request={request} busy={busy.includes(tag.place)} />
        ) : (
          <p key={`${tag.person}/${tag.place}`} className="flex items-center gap-2 text-xs text-muted-foreground">
            <span>◌ waiting for {tag.person === me ? "you" : tag.person} to approve</span>
            {author === me && (
              <Button type="button" variant="ghost" size="xs" onClick={withdraw}>
                Withdraw
              </Button>
            )}
          </p>
        )
      )}
      {error !== "" && <p className="text-xs text-destructive">{error}</p>}
    </>
  );
}
