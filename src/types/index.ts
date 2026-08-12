// Shared types mirroring the Rust core's serde models.
// Keep these in sync with src-tauri/src/core/**.

// These strings are a contract with `core/detect.rs`, and they are spelled out
// there per variant rather than derived by `rename_all` — which would have
// produced `floppyimage`, or `hard-disk-image` where this says
// `harddisk-image`. `serde_name_matches_as_str` in that module is what keeps
// the two sides honest; without it, a `===` here type-checks and never matches.
export type FormatCategory =
  | "floppy-image"
  | "harddisk-image"
  | "optical-image"
  | "archive"
  // A Commodore 8-bit disk, tape or program. Its own category rather than
  // `floppy-image`: a D64 routed to the Amiga floppy actions would be offered
  // ADF Studio and "copy to Gotek".
  | "commodore-8bit"
  | "rom"
  | "directory"
  | "unknown";

export interface Detection {
  category: FormatCategory;
  format_hint: string;
  confidence: number;
  size: number;
  is_dir: boolean;
}

export type Safety =
  | "read_only"
  | "safe"
  | "requires_backup"
  | "destructive"
  | "experimental";

export type WorkflowCategory =
  | "recommended"
  | "standard"
  | "advanced";

export type Confidence = "HIGH" | "MEDIUM" | "LOW" | "UNKNOWN";

/**
 * How an action is carried out once the user picks it.
 *
 * The engine decides this, not the UI — a component should switch on `kind`
 * rather than pattern-matching workflow ids (spec §95: no engine knowledge in
 * React).
 */
export type WorkflowKind =
  | { kind: "navigate"; route: string }
  | { kind: "execute" };

export interface WorkflowInfo {
  id: string;
  name: string;
  description: string;
  category: WorkflowCategory;
  safety: Safety;
  priority: number;
  /** False when the action is planned but not implemented — show "Coming Later". */
  available: boolean;
  kind: WorkflowKind;
}

export interface WorkflowOutcome {
  workflow_id: string;
  success: boolean;
  message: string;
  /** Present when the workflow verified its own result: true = PASS. */
  verification: boolean | null;
}

export interface Recommendation {
  info: WorkflowInfo;
  confidence: Confidence;
  reason: string;
}

export interface Plan {
  detection: Detection;
  recommendations: Recommendation[];
  candidates: WorkflowInfo[];
}

export interface DroppedAnalysis {
  path: string;
  ok: boolean;
  plan?: Plan;
  error?: string;
}

export interface AppInfo {
  name: string;
  version: string;
  platform: string;
}

export interface RecentFile {
  id: number;
  path: string;
  name: string;
  kind: string;
  opened_at: number;
}
