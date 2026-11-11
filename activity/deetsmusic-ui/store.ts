// The one place the cards read the host's state and send actions. It sits on a Transport and
// knows nothing about where the state comes from.

import { allowed, type Action, type HostState, type Track, type Transport } from "../deetsmusic-protocol";
import { toast } from "./toast";

export interface Store {
  state(): HostState | null;
  /** Calls `cb` now (when there is a state) and on every change. Returns the unsubscribe. */
  watch(cb: (s: HostState) => void): () => void;
  /** May this person do this kind of action now? */
  can(kind: Action["kind"]): boolean;
  /** Send an action. A refusal shows the host's line as a toast. */
  act(action: Action): Promise<boolean>;
  search(term: string): Promise<Track[]>;
}

export function makeStore(transport: Transport): Store {
  let current: HostState | null = null;
  const subs = new Set<(s: HostState) => void>();

  transport.subscribe((s) => {
    current = s;
    for (const cb of subs) cb(s);
  });

  return {
    state: () => current,
    watch(cb) {
      subs.add(cb);
      if (current) cb(current);
      return () => subs.delete(cb);
    },
    can: (kind) => !!current && allowed(current, kind),
    async act(action) {
      try {
        const r = await transport.act(action);
        if (!r.ok) toast(r.reason);
        else for (const n of r.notices ?? []) toast(n, "info");
        return r.ok;
      } catch {
        toast("The host did not answer. Try again.");
        return false;
      }
    },
    search: (term) => transport.search(term),
  };
}
