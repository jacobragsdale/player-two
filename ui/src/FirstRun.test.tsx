import { renderToStaticMarkup } from "react-dom/server";
import { expect, test } from "vitest";

import { FirstRun } from "./FirstRun.tsx";

test("the first-run page asks for the code and offers the Windows user name", () => {
  const page = renderToStaticMarkup(
    <FirstRun
      suggested="jacob-ragsdale"
      onJoined={() => {
        /* Not reached: nothing is clicked. */
      }}
    />
  );
  expect(page).toContain("Join code");
  expect(page).toContain('value="jacob-ragsdale"');
  expect(page).toContain("Someone on your team sent you a join code");
});
