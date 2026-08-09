// Universal Drag & Drop Manager — frontend half.
//
// The Rust side is the source of truth for detection and planning. This module
// only subscribes to native webview drag/drop events and forwards dropped
// paths to `analyze_paths`. Each module that wants drop support passes a
// callback in; there is exactly one global listener (no per-module systems).

import { getCurrentWebview } from "@tauri-apps/api/webview";
import { analyzePaths } from "@/lib/api";
import type { DroppedAnalysis } from "@/types";

export type DropPhase = "enter" | "over" | "leave" | "drop";

export interface DropHandler {
  onPhase?: (phase: DropPhase) => void;
  onDrop: (analyses: DroppedAnalysis[]) => void;
  onError?: (message: string) => void;
}

/**
 * Register the global drag & drop listener. Returns an unlisten function.
 *
 * Call this once (e.g. in the Layout). Components push their handlers via
 * `useDropZone`, which can be expanded later for per-target routing.
 */
export async function setupDragDrop(handler: DropHandler): Promise<() => void> {
  let unlisten = await getCurrentWebview().onDragDropEvent(async (event) => {
    const payload = event.payload;
    switch (payload.type) {
      case "enter":
      case "over":
        handler.onPhase?.(payload.type);
        break;
      case "leave":
        handler.onPhase?.("leave");
        break;
      case "drop": {
        handler.onPhase?.("drop");
        if (!payload.paths || payload.paths.length === 0) return;
        try {
          const analyses = await analyzePaths(payload.paths);
          handler.onDrop(analyses);
        } catch (e) {
          handler.onError?.(String(e));
        }
        break;
      }
      default:
        break;
    }
  });
  return unlisten;
}
