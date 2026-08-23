// F3 — view a file inside an image, read-only (brief §4 operation matrix).
//
// Text when it is text, hex when it is not. The distinction is a NUL byte:
// Amiga text is Latin-1 with LF line endings and no NULs, and a binary shown
// as text is a screenful of replacement characters that tells the user nothing.
//
// Latin-1, not UTF-8. Decoding an Amiga text file as UTF-8 turns every
// accented character into a question mark — the file is not broken, the
// decoder was.

import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";

import { volumeReadHead } from "@/lib/volumeWrite";
import { errorText } from "@/lib/errorText";

type Mode = "text" | "hex";

/** Amiga text is Latin-1: one byte per character, no exceptions. */
function decodeLatin1(bytes: Uint8Array): string {
  let out = "";
  for (const byte of bytes) out += String.fromCharCode(byte);
  return out;
}

/** A NUL byte means binary. Nothing else is as reliable and as cheap. */
function looksBinary(bytes: Uint8Array): boolean {
  return bytes.some((byte) => byte === 0);
}

function hexDump(bytes: Uint8Array): string {
  const lines: string[] = [];
  for (let offset = 0; offset < bytes.length; offset += 16) {
    const row = bytes.slice(offset, offset + 16);
    const hex = Array.from(row)
      .map((byte) => byte.toString(16).padStart(2, "0"))
      .join(" ")
      .padEnd(47, " ");
    const text = Array.from(row)
      .map((byte) => (byte >= 0x20 && byte !== 0x7f ? String.fromCharCode(byte) : "."))
      .join("");
    lines.push(`${offset.toString(16).padStart(8, "0")}  ${hex}  ${text}`);
  }
  return lines.join("\n");
}

export function FileViewer({
  path,
  volumeIndex,
  entryBlock,
  name,
  onClose,
}: {
  path: string;
  volumeIndex: number;
  entryBlock: number;
  name: string;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const [bytes, setBytes] = useState<Uint8Array | null>(null);
  const [mode, setMode] = useState<Mode>("text");
  const [error, setError] = useState<string | null>(null);
  const [truncated, setTruncated] = useState(false);

  useEffect(() => {
    let cancelled = false;
    const limit = 256 * 1024;

    volumeReadHead(path, volumeIndex, entryBlock, limit)
      .then((data) => {
        if (cancelled) return;
        const buffer = new Uint8Array(data);
        setBytes(buffer);
        setTruncated(buffer.length === limit);
        setMode(looksBinary(buffer) ? "hex" : "text");
      })
      .catch((e) => !cancelled && setError(errorText(t, e)));

    return () => {
      cancelled = true;
    };
  }, [path, volumeIndex, entryBlock]);

  const body = bytes
    ? mode === "text"
      ? decodeLatin1(bytes)
      : hexDump(bytes)
    : "";

  return (
    <div
      role="dialog"
      aria-label={t("components.fileViewer.ariaLabel", { name })}
      className="card"
      style={{
        position: "fixed",
        top: "50%",
        left: "50%",
        transform: "translate(-50%, -50%)",
        zIndex: 50,
        width: 780,
        maxWidth: "94vw",
        maxHeight: "88vh",
        display: "flex",
        flexDirection: "column",
        boxShadow: "0 8px 32px rgba(0,0,0,.5)",
      }}
    >
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          gap: 8,
          flexWrap: "wrap",
        }}
      >
        <strong style={{ wordBreak: "break-all" }}>{name}</strong>
        <div style={{ display: "flex", gap: 6 }}>
          <button
            className={`btn${mode === "text" ? " btn-primary" : ""}`}
            style={{ fontSize: 12 }}
            onClick={() => setMode("text")}
          >
            {t("components.fileViewer.text")}
          </button>
          <button
            className={`btn${mode === "hex" ? " btn-primary" : ""}`}
            style={{ fontSize: 12 }}
            onClick={() => setMode("hex")}
          >
            {t("components.fileViewer.hex")}
          </button>
          <button className="btn" style={{ fontSize: 12 }} onClick={onClose}>
            {t("common.close")}
          </button>
        </div>
      </div>

      <div className="faint" style={{ fontSize: 11, marginTop: 4 }}>
        {bytes
          ? t("components.fileViewer.bytesShown", { amount: bytes.length.toLocaleString() })
          : t("components.fileViewer.reading")}
        {truncated && t("components.fileViewer.restNotLoaded")}
        {mode === "text" && t("components.fileViewer.decodedLatin1")}
        {t("components.fileViewer.readOnly")}
      </div>

      {error && (
        <div className="badge badge-err" style={{ display: "block", marginTop: 8 }}>
          {error}
        </div>
      )}

      <pre
        style={{
          flex: 1,
          overflow: "auto",
          marginTop: 8,
          marginBottom: 0,
          fontSize: 11,
          whiteSpace: mode === "text" ? "pre-wrap" : "pre",
          wordBreak: mode === "text" ? "break-word" : "normal",
          minHeight: 200,
        }}
      >
        {body}
      </pre>
    </div>
  );
}
