/** The settings page: where the person's machine runs requests, and the checkouts they share, with ones found under their dev folders offered. WSL is mentioned only to someone who has it. */

import { useCallback, useEffect, useState } from "react";
import type { JSX, SyntheticEvent } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

import type { Folder, PlacesView, RunsIn } from "./desk.ts";
import { ask, problem } from "./deskClient.ts";

const HEADING = "mt-6 mb-2 text-sm font-semibold";

/** Where something runs, in words: Windows, or a distro's name. */
export function where(runsIn: RunsIn): string {
  return runsIn === "windows" ? "Windows" : runsIn.replace(/^wsl:/, "");
}

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

function FolderRow({
  folder,
  wsl,
  action,
  label,
  onAct
}: {
  readonly folder: Folder;
  readonly wsl: boolean;
  readonly action: string;
  readonly label: string;
  readonly onAct: () => void;
}): JSX.Element {
  return (
    <li className="flex items-center gap-2 py-1">
      <span className="font-medium">{folder.name}</span>
      <span className="truncate text-xs text-muted-foreground" title={folder.path}>
        {folder.path}
        {wsl ? ` · ${where(folder.runs_in)}` : ""}
      </span>
      <Button type="button" variant="ghost" size="xs" className="ml-auto" aria-label={label} onClick={onAct}>
        {action}
      </Button>
    </li>
  );
}

export function Settings({ me, onBack }: { readonly me: string; readonly onBack: () => void }): JSX.Element {
  const [places, setPlaces] = useState<PlacesView | null>(null);
  const [suggestions, setSuggestions] = useState<readonly Folder[] | null>(null);
  const [path, setPath] = useState("");
  const [runsIn, setRunsIn] = useState<RunsIn>("windows");
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
  const look = useCallback(() => {
    ask({ type: "suggestions" }).then(
      (response) => {
        if (response.type === "suggestions") {
          setSuggestions(response.folders);
        }
      },
      () => {
        setSuggestions([]);
      }
    );
  }, []);
  useEffect(() => {
    load();
    look();
  }, [load, look]);

  /** Sends a change, then shows the page as it now stands. */
  const change = (request: Parameters<typeof ask>[0], then?: () => void): void => {
    setError("");
    ask(request).then(
      () => {
        then?.();
        load();
        look();
      },
      (failure: unknown) => {
        setError(problem(failure));
      }
    );
  };

  const add = (event: SyntheticEvent<HTMLFormElement>): void => {
    event.preventDefault();
    change({ type: "add_folder", path, runs_in: runsIn }, () => {
      setPath("");
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
      {places.folders.length === 0 && <p className="text-muted-foreground">None yet. Share a checkout so teammates can ask about it as @{me}/name.</p>}
      <ul>
        {places.folders.map((folder) => (
          <FolderRow
            key={folder.name}
            folder={folder}
            wsl={wsl}
            action="Stop sharing"
            label={`Stop sharing ${folder.name}`}
            onAct={() => {
              change({ type: "remove_folder", name: folder.name });
            }}
          />
        ))}
      </ul>
      <form className="mt-2 flex items-center gap-2" onSubmit={add}>
        <Input
          className="flex-1"
          aria-label="Folder to share"
          placeholder={wsl ? "C:\\dev\\repo, or /home/you/dev/repo" : "C:\\dev\\repo"}
          value={path}
          onChange={(event) => {
            setPath(event.target.value);
          }}
        />
        <RunsInPicker label="Which system the folder is on" value={runsIn} distros={places.distros} onChange={setRunsIn} />
        <Button type="submit" disabled={path.trim() === ""}>
          Share
        </Button>
      </form>
      {error !== "" && (
        <p role="alert" className="mt-2 text-destructive">
          {error}
        </p>
      )}

      <h2 className={HEADING}>Found in your dev folders</h2>
      {suggestions === null && <p className="text-muted-foreground">Looking…</p>}
      {suggestions?.length === 0 && <p className="text-muted-foreground">Nothing more to share.</p>}
      <ul>
        {suggestions?.map((folder) => (
          <FolderRow
            key={`${folder.runs_in}:${folder.path}`}
            folder={folder}
            wsl={wsl}
            action="Share"
            label={`Share ${folder.name}`}
            onAct={() => {
              change({ type: "add_folder", path: folder.path, runs_in: folder.runs_in });
            }}
          />
        ))}
      </ul>
    </main>
  );
}
