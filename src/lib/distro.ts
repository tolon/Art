// Typed wrappers for the distro registry — the OS Builder's profiles.
//
// **ART never downloads a distro image** (`ART-research-distro-profiles.md`
// §2). There is no fetch function here and there will not be one: `homepage` is
// where the *user* goes, and ART then accepts a local file. The same rule ART
// already applies to Kickstart ROMs.

import { invoke } from "@tauri-apps/api/core";

/** What the user has to do about the licence before ART can help. */
export type LicenceModel = "free-grey" | "user-licensed" | "art-baseline";

export type Acquisition = "user-supplies-image" | "art-builds";

export type ImageFormat = "raw-img" | "seven-zip-img" | "build-recipe";

export type BaseOs = "os32" | "os39" | "none-declared";

export interface RomRequirement {
  /** `"3.1"` or `"3.2"` — matched against what ROM Manager identified. */
  family: string;
  /** The name the ROM takes on the card, written into `initramfs`. */
  drop_name: string;
}

export interface MultibootSlot {
  config_set_name: string;
}

export interface DistroProfile {
  id: string;
  name: string;
  homepage: string;
  licence_model: LicenceModel;
  acquisition: Acquisition;
  image_format: ImageFormat;
  min_card_gb: number;
  base_os: BaseOs;
  rom_requirement: RomRequirement | null;
  default_cmdline_tokens: string[];
  multiboot: MultibootSlot;
  packages: string[];
  /** i18n keys, not sentences. */
  post_install_notes: string[];
  /** `false` renders as Coming Later rather than vanishing (§96). */
  available: boolean;
}

export type CardProblem = { kind: "too-small"; needs_gb: number; has_gb: number };

export interface SuppliedImage {
  path: string;
  size_bytes: number;
  is_file: boolean;
}

export async function distroProfiles(): Promise<DistroProfile[]> {
  return invoke<DistroProfile[]>("distro_profiles");
}

export async function distroCheckCard(
  id: string,
  cardBytes: number
): Promise<CardProblem | null> {
  return invoke<CardProblem | null>("distro_check_card", { id, cardBytes });
}

/** Whether an identified ROM belongs with this profile's base OS. `null` when
 *  there is nothing to say — an unrecognised ROM has no opinion attached. */
export async function distroRomFamilyMatches(
  id: string,
  romVersion: string
): Promise<boolean | null> {
  return invoke<boolean | null>("distro_rom_family_matches", { id, romVersion });
}

export async function distroMeasureImage(path: string): Promise<SuppliedImage> {
  return invoke<SuppliedImage>("distro_measure_image", { path });
}
