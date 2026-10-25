import { renderToStaticMarkup } from "react-dom/server";
import { expect, test } from "vitest";

import { Approval } from "./Approval.tsx";

test("a follow-up's card starts with keep going unticked, whoever had it before", () => {
  const markup = renderToStaticMarkup(<Approval chat="#ops" request={5} place="machine" asker="carol" follows={{ runs_in: "windows" }} />);
  expect(markup).toContain("follows up. Run it too?");
  expect(markup).toContain("Let carol keep going in this thread");
  expect(markup).not.toContain("checked");
});
