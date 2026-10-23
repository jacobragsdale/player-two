import { renderToStaticMarkup } from "react-dom/server";
import { expect, test } from "vitest";

import { ago, PersonLine } from "./Chat.tsx";

test("a time ago is said in round words", () => {
  const now = 10 * 86_400_000;
  expect(ago(now - 20_000, now)).toBe("just now");
  expect(ago(now - 5 * 60_000, now)).toBe("5 min ago");
  expect(ago(now - 2 * 3_600_000, now)).toBe("2 h ago");
  expect(ago(now - 86_400_000, now)).toBe("a day ago");
  expect(ago(now - 3 * 86_400_000, now)).toBe("3 days ago");
});

test("a direct chat says whether the person is here and what they share", () => {
  const here = renderToStaticMarkup(
    <PersonLine
      person={{
        name: "bob",
        here: true,
        places: [
          { name: "machine", kind: "machine" },
          { name: "uploader", kind: "repo" }
        ]
      }}
    />
  );
  expect(here).toContain("here now · shares @bob, @bob/uploader");
  const away = renderToStaticMarkup(<PersonLine person={{ name: "carol", here: false, last_seen: Date.now() - 2 * 3_600_000, places: [] }} />);
  expect(away).toContain("last seen 2 h ago · no agent set up yet");
});
