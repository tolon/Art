// Beginner / Power User mode (spec §47, §48).
//
// The same application serves all three audiences the spec describes; the mode
// decides how much of the machinery is on show. Beginner mode *hides* advanced
// concepts — it never disables an operation the user could otherwise perform,
// and it never changes what ART actually does.
//
// Rule of thumb for what belongs behind Power User mode:
//   - Anything the spec lists in §47: RDB, partitions, boot blocks, geometry,
//     blocks, sectors, hex, hunks, checksums, raw images.
//   - Numbers a beginner cannot act on: block numbers, confidence percentages.
// And what must stay visible in both:
//   - Health as words, not internals: Healthy / Warning / Problem / Unknown (§48).
//   - Anything that warns about data loss.

import { useSettingsStore } from "@/stores/settingsStore";

/** True when the user has chosen Power User mode. */
export function usePowerMode(): boolean {
  return useSettingsStore((s) => s.settings.uxMode === "power");
}
