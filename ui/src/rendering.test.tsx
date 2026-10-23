/** Text from other people and their agents is drawn, never run: no script, no element from their HTML, no fetched image, no script address. */

import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, test } from "vitest";

import { Markdown } from "./Markdown.tsx";
import type { RunView } from "./desk.ts";
import { Message } from "./Message.tsx";
import { Run } from "./Run.tsx";
import { Text } from "./Text.tsx";

const HOSTILE = [
  "<script>window.pwned = 1</script>",
  '<img src="x" onerror="window.pwned = 2">',
  "<b>bold?</b>",
  '<a href="javascript:window.pwned = 3">click</a>',
  '<iframe src="https://example.com"></iframe>',
  "<style>body { display: none }</style>"
].join("\n\n");

/** Whether drawn markup holds a real element (not escaped text) a message could use to run, fetch or restyle: such a tag, an event handler in a tag, or a script address. */
function dangerous(markup: string): RegExpMatchArray | null {
  return /<(script|img|iframe|style|b|object|embed|link)\b|<[a-z][^>]*\son\w+=|href="\s*javascript:/i.exec(markup);
}

describe("a person's message", () => {
  test("is drawn as text, whatever it holds", () => {
    const markup = renderToStaticMarkup(<Text text={HOSTILE} />);
    expect(dangerous(markup)).toBeNull();
    expect(markup).toContain("&lt;script&gt;window.pwned = 1&lt;/script&gt;");
  });

  test("is not Markdown", () => {
    const markup = renderToStaticMarkup(<Text text={"**not bold** [link](https://example.com)"} />);
    expect(markup).not.toContain("<strong>");
    expect(markup).not.toContain("<a ");
  });

  test("keeps its author and text inert in the message around it", () => {
    const item = { id: 1, author: '<img src=x onerror="pwned()">', ts: 0, text: HOSTILE, tags: [], replies: 0, runs: [], waiting: [], follows: null, unknown: [] };
    const markup = renderToStaticMarkup(<Message item={item} me="alice" chat="#ops" onThread={null} busy={[]} mode="ask" />);
    expect(dangerous(markup)).toBeNull();
  });
});

describe("an agent's Markdown", () => {
  test("is drawn as elements with raw HTML left as text", () => {
    const markup = renderToStaticMarkup(<Markdown text={`**done**\n\n${HOSTILE}`} />);
    expect(markup).toContain("<strong>done</strong>");
    expect(dangerous(markup)).toBeNull();
    expect(markup).toContain("&lt;script&gt;");
  });

  test("names an image instead of fetching it", () => {
    const markup = renderToStaticMarkup(<Markdown text={"![a chart](https://example.com/track.png)"} />);
    expect(markup).not.toContain("<img");
    expect(markup).toContain("[image: a chart] https://example.com/track.png");
  });

  test("drops script addresses from links", () => {
    const markup = renderToStaticMarkup(<Markdown text={"[safe](https://example.com) [unsafe](javascript:alert(1)) [data](data:text/html,<script>x</script>)"} />);
    expect(markup).toContain('href="https://example.com"');
    expect(markup).not.toMatch(/href="(javascript|data):/);
    expect(markup).toContain('rel="noreferrer"');
  });
});

describe("an agent's run", () => {
  test("draws what it printed, what it asks and what it answered without running any of it", () => {
    const run: RunView = {
      id: 7,
      thread: 6,
      owner: "bob",
      place: "machine",
      runs_in: "windows",
      mode: "ask",
      steps_by: "owner",
      state: "working",
      reason: null,
      activity: [{ id: "t1", verb: "ran", object: "<img src=x onerror=pwned()>", outcome: "exit 0", output: HOSTILE }],
      prompts: [{ id: "p1", what: "<script>window.pwned = 9</script>", allow: null, by: null }],
      result: `**fixed**\n\n${HOSTILE}`,
      may_answer: true,
      mine: false
    };
    const markup = renderToStaticMarkup(<Run run={run} chat="alice+bob" me="alice" />);
    expect(dangerous(markup)).toBeNull();
    expect(markup).toContain("<strong>fixed</strong>");
    expect(markup).toContain("bob&#x27;s agent is working on bob&#x27;s computer");
    expect(markup).toContain("Allow");
  });
});
