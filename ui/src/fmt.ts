import type { Format } from "./api";

const nf1 = new Intl.NumberFormat("pt-BR", { maximumFractionDigits: 1 });
const nf0 = new Intl.NumberFormat("pt-BR", { maximumFractionDigits: 0 });

export function size(b: number | null | undefined): string {
  if (b == null) return "";
  if (b < 1024) return `${nf0.format(b)} B`;
  if (b < 1024 ** 2) return `${nf0.format(b / 1024)} KB`;
  if (b < 1024 ** 3) return `${nf1.format(b / 1024 ** 2)} MB`;
  return `${nf1.format(b / 1024 ** 3)} GB`;
}

export function date(secs: number | null | undefined): string {
  if (secs == null) return "";
  const d = new Date(secs * 1000);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(d.getDate())}/${p(d.getMonth() + 1)}/${d.getFullYear()} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

export function duration(ms: number): string {
  const s = Math.max(0, Math.round(ms / 1000));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const ss = s % 60;
  const p = (n: number) => String(n).padStart(2, "0");
  return h > 0 ? `${h}:${p(m)}:${p(ss)}` : `${p(m)}:${p(ss)}`;
}

export function percent(x: number): string {
  return `${nf0.format(x * 100)}%`;
}

export function speed(bytesPerSec: number): string {
  return `${size(bytesPerSec)}/s`;
}

export function baseName(p: string): string {
  const parts = p.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? p;
}

export function dirName(p: string): string {
  const i = Math.max(p.lastIndexOf("\\"), p.lastIndexOf("/"));
  return i > 0 ? p.slice(0, i) : p;
}

export function ext(name: string): string {
  const lower = name.toLowerCase();
  for (const d of [".tar.gz", ".tar.xz", ".tar.zst", ".tar.bz2", ".tar.lz4", ".tar.br"]) if (lower.endsWith(d)) return d;
  const i = lower.lastIndexOf(".");
  return i > 0 ? lower.slice(i) : "";
}

export function crc(c: number | null): string {
  return c == null ? "" : (c >>> 0).toString(16).toUpperCase().padStart(8, "0");
}

export const ARCHIVE_EXTS = [
  ".7z", ".zip", ".rar", ".tar", ".gz", ".tgz", ".xz", ".txz", ".zst", ".tzst", ".bz2", ".tbz2", ".lz4", ".br", ".cbz", ".cbr",
  ".jar", ".apk", ".epub", ".001",
];

export function isArchiveName(name: string): boolean {
  const l = name.toLowerCase();
  return ARCHIVE_EXTS.some((e) => l.endsWith(e)) || /\.part\d+\.rar$/.test(l) || /\.r\d\d$/.test(l);
}

export const FORMATS: { id: Format; label: string; hint: string; single?: boolean }[] = [
  { id: "7z", label: "7Z", hint: "Melhor compactação · AES-256 · sólido" },
  { id: "zip", label: "ZIP", hint: "Abre em qualquer sistema" },
  { id: "tar.zst", label: "TAR.ZST", hint: "Muito rápido · ótima taxa" },
  { id: "tar.xz", label: "TAR.XZ", hint: "Compacto · padrão do Linux" },
  { id: "tar.gz", label: "TAR.GZ", hint: "Compatível com tudo no Linux" },
  { id: "tar.bz2", label: "TAR.BZ2", hint: "Antigo, lento" },
  { id: "tar.lz4", label: "TAR.LZ4", hint: "Instantâneo, taxa baixa" },
  { id: "tar.br", label: "TAR.BR", hint: "Brotli · bom para texto" },
  { id: "tar", label: "TAR", hint: "Sem compactação" },
  { id: "zst", label: "ZST", hint: "Um arquivo só", single: true },
  { id: "xz", label: "XZ", hint: "Um arquivo só", single: true },
  { id: "gz", label: "GZ", hint: "Um arquivo só", single: true },
  { id: "bz2", label: "BZ2", hint: "Um arquivo só", single: true },
  { id: "lz4", label: "LZ4", hint: "Um arquivo só", single: true },
  { id: "br", label: "BR", hint: "Um arquivo só", single: true },
];

export const LEVELS: { value: number; label: string }[] = [
  { value: 0, label: "Armazenar" },
  { value: 1, label: "Mais rápida" },
  { value: 3, label: "Rápida" },
  { value: 5, label: "Normal" },
  { value: 7, label: "Boa" },
  { value: 9, label: "Melhor" },
];

export function withExt(path: string, format: Format): string {
  const name = baseName(path);
  const e = ext(name);
  const known = e && (FORMATS.some((f) => `.${f.id}` === e) || e === ".rar");
  const stem = known ? path.slice(0, path.length - e.length) : path;
  return `${stem}.${format}`;
}
