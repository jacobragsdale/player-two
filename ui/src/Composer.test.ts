import { expect, test } from "vitest";

import { tagsMe } from "./Composer.tsx";

test("the mode shows only for a text that tags one of your places", () => {
  expect(tagsMe("@alice fix pip", "alice")).toBe(true);
  expect(tagsMe("look at @alice/uploader.", "alice")).toBe(true);
  expect(tagsMe("@alice", "alice")).toBe(true);
  expect(tagsMe("@bob fix pip", "alice")).toBe(false);
  expect(tagsMe("@alicex", "alice")).toBe(false);
  expect(tagsMe("mail alice@example.com", "alice")).toBe(false);
});
