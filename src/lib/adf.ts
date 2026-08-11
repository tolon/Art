// Typed wrappers for the ADF Studio commands.

import { invoke } from "@tauri-apps/api/core";

export interface AdfInfo {
  volume_name: string;
  fs_type: "ofs" | "ffs";
  international: boolean;
  dir_cache: boolean;
  bootable: boolean;
  checksum_valid: boolean;
  capacity_bytes: number;
  used_bytes: number;
  free_bytes: number;
  file_count: number;
  directory_count: number;
  root_block: number;
}

export interface AdfEntry {
  name: string;
  kind: "file" | "directory";
  byte_size: number;
  comment: string;
  unix_date: number;
  header_block: number;
  parent: number;
  /** `hsparwed`, already formatted Rust-side — see `PanelEntry.attrs`. */
  attrs: string;
}

export type HealthStatus = "healthy" | "warning" | "problem";

export interface ValidationFinding {
  severity: HealthStatus;
  code: string;
  message: string;
}

export interface ValidationReport {
  status: HealthStatus;
  findings: ValidationFinding[];
}

/**
 * Returned by every operation that modifies an ADF on disk.
 *
 * `backup_path` is where the previous version was preserved before the change
 * was committed — surface it to the user so a mistaken edit is always
 * recoverable (spec §92).
 */
export interface MutationOutcome {
  info: AdfInfo;
  backup_path: string | null;
}

export async function adfOpen(path: string): Promise<AdfInfo> {
  return invoke<AdfInfo>("adf_open", { path });
}

export async function adfList(path: string, dirBlock?: number): Promise<AdfEntry[]> {
  return invoke<AdfEntry[]>("adf_list", { path, dirBlock: dirBlock ?? null });
}

export async function adfValidate(path: string): Promise<ValidationReport> {
  return invoke<ValidationReport>("adf_validate", { path });
}

export async function adfExtractFile(path: string, headerBlock: number): Promise<number[]> {
  return invoke<number[]>("adf_extract_file", { path, headerBlock });
}

export async function adfCreateBlank(
  path: string,
  volumeName: string,
  fsType: "ofs" | "ffs",
  bootable: boolean
): Promise<AdfInfo> {
  return invoke<AdfInfo>("adf_create_blank", {
    path,
    volumeName,
    fsType,
    bootable,
  });
}

export async function adfAddFile(
  path: string,
  dirBlock: number | undefined,
  sourceFilePath: string,
  targetName?: string
): Promise<MutationOutcome> {
  return invoke<MutationOutcome>("adf_add_file", {
    path,
    dirBlock: dirBlock ?? null,
    sourceFilePath,
    targetName: targetName ?? null,
  });
}

export async function adfCreateDirectory(
  path: string,
  parentBlock: number | undefined,
  name: string
): Promise<MutationOutcome> {
  return invoke<MutationOutcome>("adf_create_directory", {
    path,
    parentBlock: parentBlock ?? null,
    name,
  });
}

export async function adfDeleteEntry(
  path: string,
  parentBlock: number | undefined,
  headerBlock: number
): Promise<MutationOutcome> {
  return invoke<MutationOutcome>("adf_delete_entry", {
    path,
    parentBlock: parentBlock ?? null,
    headerBlock,
  });
}

export async function adfRenameEntry(
  path: string,
  parentBlock: number | undefined,
  headerBlock: number,
  newName: string
): Promise<MutationOutcome> {
  return invoke<MutationOutcome>("adf_rename_entry", {
    path,
    parentBlock: parentBlock ?? null,
    headerBlock,
    newName,
  });
}
