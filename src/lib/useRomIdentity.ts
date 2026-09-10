// What Kickstart a file is — asked once, from one place (four-tab design
// § 3.3). Until 2026-09-09 this effect lived in `OsInstall.tsx`; the
// Kickstart field now sits on tab 3 while the plan on tab 1 still needs the
// ROM's name for a component's condition line, and two copies of one effect
// is two answers to one question.

import { useEffect, useState } from "react";

import { pistormIdentifyRom, type RomInfo } from "@/lib/pistorm";

export interface RomIdentity {
  rom: RomInfo | null;
  /** The core could not read the file as a Kickstart. Never true while
   *  `rom` is set: the two endings are exclusive by construction. */
  unreadable: boolean;
}

export function useRomIdentity(path: string | null): RomIdentity {
  const [rom, setRom] = useState<RomInfo | null>(null);
  const [unreadable, setUnreadable] = useState(false);

  useEffect(() => {
    if (!path) {
      setRom(null);
      setUnreadable(false);
      return;
    }
    let cancelled = false;
    pistormIdentifyRom(path)
      .then((r) => {
        if (cancelled) return;
        setRom(r);
        setUnreadable(false);
      })
      .catch(() => {
        if (cancelled) return;
        setRom(null);
        setUnreadable(true);
      });
    return () => {
      cancelled = true;
    };
  }, [path]);

  return { rom, unreadable };
}
