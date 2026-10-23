/** Joining, making and leaving channels: the dialog that lists the channels the person is not in, with a field for a new one, and the button that leaves one. */

import { useState } from "react";
import type { JSX, SyntheticEvent } from "react";

import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";

import type { ChannelInfo, ChatId } from "./desk.ts";
import { ask, problem } from "./deskClient.ts";

const ROW = "h-auto w-full justify-start px-2 py-1 font-normal";

interface Props {
  readonly me: string;
  readonly channels: readonly ChannelInfo[];
  /** Called with the chat to open once the person joined or made one, or with null when they closed the dialog. */
  readonly onDone: (chat: ChatId | null) => void;
}

export function ChannelDialog({ me, channels, onDone }: Props): JSX.Element {
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const others = channels.filter((channel) => !channel.members.includes(me));

  const finish = (request: Promise<unknown>, chat: ChatId): void => {
    setBusy(true);
    setError("");
    request.then(
      () => {
        onDone(chat);
      },
      (failure: unknown) => {
        setBusy(false);
        setError(problem(failure));
      }
    );
  };

  const create = (event: SyntheticEvent<HTMLFormElement>): void => {
    event.preventDefault();
    const bare = name.trim().replace(/^#/, "");
    finish(ask({ type: "create_channel", name: bare }), `#${bare}`);
  };

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) {
          onDone(null);
        }
      }}
    >
      <DialogContent className="max-h-[calc(100vh-2rem)] gap-3 overflow-y-auto sm:max-w-sm">
        <DialogHeader>
          <DialogTitle>Join or create a channel</DialogTitle>
          <DialogDescription>Channels are open to everyone on the hub, and whoever joins sees what was said before.</DialogDescription>
        </DialogHeader>
        {others.length > 0 && (
          <ul className="max-h-60 overflow-y-auto">
            {others.map((channel) => (
              <li key={channel.name}>
                <Button
                  type="button"
                  variant="ghost"
                  className={ROW}
                  disabled={busy}
                  onClick={() => {
                    finish(ask({ type: "join_channel", name: channel.name }), `#${channel.name}`);
                  }}
                >
                  #{channel.name}
                  <span className="ml-auto text-xs text-muted-foreground">{channel.members.length} in it</span>
                </Button>
              </li>
            ))}
          </ul>
        )}
        <form className="flex gap-1.5" onSubmit={create}>
          <Input
            className="flex-1"
            aria-label="New channel's name"
            placeholder="new-channel"
            value={name}
            onChange={(event) => {
              setName(event.target.value);
            }}
          />
          <Button type="submit" disabled={busy || name.trim() === ""}>
            Create
          </Button>
        </form>
        {error !== "" && (
          <p role="alert" className="text-xs text-destructive">
            {error}
          </p>
        )}
      </DialogContent>
    </Dialog>
  );
}

/** The button in a channel's header that leaves it, once the person says so. */
export function LeaveChannel({ chat, onLeft }: { readonly chat: ChatId; readonly onLeft: () => void }): JSX.Element {
  const [asking, setAsking] = useState(false);
  const [error, setError] = useState("");
  const leave = (): void => {
    ask({ type: "leave_channel", name: chat.replace(/^#/, "") }).then(
      () => {
        setAsking(false);
        onLeft();
      },
      (failure: unknown) => {
        setError(problem(failure));
      }
    );
  };
  return (
    <>
      <Button
        type="button"
        variant="ghost"
        size="xs"
        className="ml-auto text-muted-foreground"
        onClick={() => {
          setError("");
          setAsking(true);
        }}
      >
        Leave
      </Button>
      <Dialog open={asking} onOpenChange={setAsking}>
        <DialogContent className="gap-3 sm:max-w-sm">
          <DialogHeader>
            <DialogTitle>Leave {chat}?</DialogTitle>
            <DialogDescription>You stop seeing what is said there. You can join again from Join or create.</DialogDescription>
          </DialogHeader>
          <div className="flex justify-end gap-2">
            <Button
              type="button"
              variant="outline"
              onClick={() => {
                setAsking(false);
              }}
            >
              Stay
            </Button>
            <Button type="button" variant="destructive" onClick={leave}>
              Leave
            </Button>
          </div>
          {error !== "" && (
            <p role="alert" className="text-xs text-destructive">
              {error}
            </p>
          )}
        </DialogContent>
      </Dialog>
    </>
  );
}
