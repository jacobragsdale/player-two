import { expect, test } from "vitest";

import { title } from "./title.ts";

test("the window is called Player Two", () => {
  expect(title()).toBe("Player Two");
});
