/** The settings page: where the person's machine runs requests, and the folders they share, each picked with the folder dialog. WSL is mentioned only to someone who has it. */

import { useCallback, useEffect, useState } from "react";
import type { JSX, SyntheticEvent } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

import { AgentSetup, where } from "./AgentSetup.tsx";
import type { PlacesView, RunsIn, SharedFolder } from "./desk.ts";
import { ask, problem } from "./deskClient.ts";

const HEADING = "mt-6 mb-2 text-sm font-semibold";

/** A choice of Windows or one of the distros, drawn only when there are distros to choose. */
function RunsInPicker({
  value,
  distros,
  onChange,
  label
}: {
  readonly value: RunsIn;
  readonly distros: readonly string[];
  readonly onChange: (runsIn: RunsIn) => void;
  readonly label: string;
}): JSX.Element | null {
  if (distros.length === 0) {
    return null;
  }
  return (
    <select
      aria-label={label}
      value={value}
      onChange={(event) => {
        onChange(event.target.value);
      }}
    >
      <option value="windows">Windows</option>
      {distros.map((distro) => (
        <option key={distro} value={`wsl:${distro}`}>
          {distro}
        </option>
      ))}
    </select>
  );
}

/** One shared folder: its name, which can be changed, where it is, why it cannot run when it cannot, and the button that stops sharing it. */
function FolderRow({ folder, wsl, onRename, onStop }: { readonly folder: SharedFolder; readonly wsl: boolean; readonly onRename: (to: string) => void; readonly onStop: () => void }): JSX.Element {
  const [editing, setEditing] = useState<string | null>(null);
  const save = (event: SyntheticEvent<HTMLFormElement>): void => {
    event.preventDefault();
    if (editing !== null && editing.trim() !== folder.name) {
      onRename(editing);
    }
    setEditing(null);
  };
  return (
    <li className="flex items-center gap-2 py-1">
      {editing === null ? (
        <>
          <span className="font-medium">{folder.name}</span>
          <Button
            type="button"
            variant="ghost"
            size="xs"
            className="text-muted-foreground"
            aria-label={`Rename ${folder.name}`}
            onClick={() => {
              setEditing(folder.name);
            }}
          >
            Rename
          </Button>
        </>
      ) : (
        <form className="flex items-center gap-1" onSubmit={save}>
          <Input
            className="h-7 w-44"
            aria-label={`New name for ${folder.name}`}
            value={editing}
            autoFocus
            onChange={(event) => {
              setEditing(event.target.value);
            }}
            onKeyDown={(event) => {
              if (event.key === "Escape") {
                setEditing(null);
              }
            }}
          />
          <Button type="submit" size="xs">
            Save
          </Button>
        </form>
      )}
      <span className="truncate text-xs text-muted-foreground" title={folder.path}>
        {folder.path}
        {wsl ? ` · ${where(folder.runs_in)}` : ""}
      </span>
      {folder.unavailable !== null && <span className="text-xs text-warn">not available: {folder.unavailable}</span>}
      <Button type="button" variant="ghost" size="xs" className="ml-auto" aria-label={`Stop sharing ${folder.name}`} onClick={onStop}>
        Stop sharing
      </Button>
    </li>
  );
}

export function Settings({ me, onBack }: { readonly me: string; readonly onBack: () => void }): JSX.Element {
  const [places, setPlaces] = useState<PlacesView | null>(null);
  const [picking, setPicking] = useState(false);
  const [error, setError] = useState("");

  const load = useCallback(() => {
    ask({ type: "places" }).then(
      (response) => {
        if (response.type === "places") {
          setPlaces(response);
        }
      },
      (failure: unknown) => {
        setError(problem(failure));
      }
    );
  }, []);
  useEffect(load, [load]);

  /** Sends a change, then shows the page as it now stands. */
  const change = (request: Parameters<typeof ask>[0], then?: () => void): void => {
    setError("");
    ask(request).then(
      () => {
        then?.();
        load();
      },
      (failure: unknown) => {
        then?.();
        setError(problem(failure));
      }
    );
  };

  /** Opens the folder dialog; the desk shares what is picked. */
  const pick = (): void => {
    setPicking(true);
    change({ type: "pick_folder" }, () => {
      setPicking(false);
    });
  };

  if (places === null) {
    return <main className="flex h-full items-center justify-center text-muted-foreground">Loading…</main>;
  }
  const wsl = places.distros.length > 0;
  return (
    <main className="mx-auto h-full w-full max-w-2xl overflow-y-auto p-6">
      <Button type="button" variant="ghost" size="sm" onClick={onBack}>
        ← Back to the chats
      </Button>
      <h1 className="mt-2 text-xl font-bold">Your places</h1>
      <p className="text-muted-foreground">Teammates can ask your agent for help at these places. Nothing runs until you say so.</p>

      <h2 className={HEADING}>Your agent</h2>
      <AgentSetup places={places} onChange={change} onLook={load} first={false} />

      <h2 className={HEADING}>Your machine</h2>
      <p className="flex items-center gap-2">
        <span>@{me}</span>
        {wsl ? (
          <>
            <span className="text-muted-foreground">runs on</span>
            <RunsInPicker
              label="Where @me runs"
              value={places.machine_runs_in}
              distros={places.distros}
              onChange={(value) => {
                change({ type: "set_machine_runs_in", runs_in: value });
              }}
            />
          </>
        ) : (
          <span className="text-muted-foreground">for help with this computer</span>
        )}
      </p>

      <h2 className={HEADING}>Shared folders</h2>
      {places.folders.length === 0 && <p className="text-muted-foreground">None yet. Share a folder so teammates can ask about it as @{me}/name.</p>}
      <ul>
        {places.folders.map((folder) => (
          <FolderRow
            key={folder.name}
            folder={folder}
            wsl={wsl}
            onRename={(to) => {
              change({ type: "rename_folder", name: folder.name, to });
            }}
            onStop={() => {
              change({ type: "remove_folder", name: folder.name });
            }}
          />
        ))}
      </ul>
      <Button type="button" variant="outline" size="sm" className="mt-2" disabled={picking} onClick={pick}>
        {picking ? "Picking…" : "Share a folder…"}
      </Button>
      {places.wsl && <p className="mt-1 text-xs text-muted-foreground">A folder in WSL is under Linux in the dialog&rsquo;s side panel.</p>}
      {error !== "" && (
        <p role="alert" className="mt-2 text-destructive">
          {error}
        </p>
      )}
    </main>
  );
}
