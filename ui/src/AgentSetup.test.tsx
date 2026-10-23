import { renderToStaticMarkup } from "react-dom/server";
import { expect, test } from "vitest";

import { AgentSetup } from "./AgentSetup.tsx";
import type { PlacesView } from "./desk.ts";

const nothing: PlacesView = { machine_runs_in: "windows", folders: [], distros: [], agents: [], half_installed: [], wsl: false };

function drawn(places: PlacesView): string {
  return renderToStaticMarkup(
    <AgentSetup
      places={places}
      onChange={() => {
        /* Not reached: nothing is clicked. */
      }}
      onLook={() => {
        /* Not reached either. */
      }}
      first
    />
  );
}

test("with no agent, it asks which one and offers to set up Cursor, saying nothing of WSL to someone without it", () => {
  const page = drawn(nothing);
  expect(page).toContain("Which do you use?");
  expect(page).toContain("Set up Cursor");
  expect(page).toContain("irm &#x27;https://cursor.com/install?win32=true&#x27; | iex");
  expect(page).not.toContain("WSL");
});

test("it says what is missing from an editor, and tells someone with a distro about WSL", () => {
  const page = drawn({ ...nothing, half_installed: ["cursor", "claude"], wsl: true });
  expect(page).toContain("The Cursor editor is installed, but not Cursor");
  expect(page).toContain("npm install -g @agentclientprotocol/claude-agent-acp");
  expect(page).toContain("Using WSL?");
});

test("an agent found is listed with a Check, and no install", () => {
  const page = drawn({ ...nothing, agents: [{ kind: "copilot", runs_in: "windows", chosen: true }] });
  expect(page).toContain("Copilot");
  expect(page).toContain("Check");
  expect(page).not.toContain("Which do you use?");
});
