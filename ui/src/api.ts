// Ponte com o backend Tauri. Fora do Tauri (navegador comum, páginas de teste visual) responde
// com dados simulados, sem tocar em arquivos.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export type Format =
  | "zip" | "7z" | "rar" | "tar" | "tar.gz" | "tar.xz" | "tar.zst" | "tar.bz2" | "tar.lz4" | "tar.br"
  | "gz" | "xz" | "zst" | "bz2" | "lz4" | "br";

export type ZipMethod = "deflate" | "zopfli" | "zstd" | "xz" | "bzip2";
export type Overwrite = "overwrite" | "skip" | "rename" | "newer";

export interface Entry {
  path: string;
  isDir: boolean;
  size: number;
  packed: number | null;
  modified: number | null;
  crc: number | null;
  encrypted: boolean;
  method: string | null;
}

export interface ArchiveInfo {
  path: string;
  format: Format;
  entries: Entry[];
  solid: boolean;
  encryptedNames: boolean;
  comment: string | null;
  sizeOnDisk: number;
  volumes: number;
  hasRecovery: boolean;
  locked: boolean;
  blocks: number | null;
}

export interface FsEntry {
  name: string;
  path: string;
  isDir: boolean;
  size: number;
  modified: number | null;
  hidden: boolean;
}

export interface DirListing {
  path: string;
  parent: string | null;
  entries: FsEntry[];
}

export interface Place {
  label: string;
  path: string;
  kind: string;
  free: number | null;
  total: number | null;
}

export interface TypeInfo {
  icon: string | null;
  name: string;
}

export interface CreateOptions {
  format: Format;
  level: number;
  zipMethod: ZipMethod;
  password: string | null;
  encryptNames: boolean;
  threads: number;
  solid: boolean;
  dictionaryMb: number;
  volumeSize: number;
  deleteSources: boolean;
  smart: boolean;
  comment: string | null;
  exclude: string[];
  testAfter: boolean;
}

export interface ExtractOptions {
  password?: string | null;
  overwrite: Overwrite;
  selection: string[];
  stripPrefix?: string | null;
  threads: number;
}

export interface Edit {
  delete: string[];
  rename: [string, string][];
  add: string[];
  addTo: string;
}

export type JobSpec =
  | { kind: "create"; sources: string[]; out: string; options: CreateOptions; email?: boolean }
  | { kind: "extract"; archive: string; dest: string; options: ExtractOptions; smartHere?: boolean; openAfter?: boolean }
  | { kind: "test"; archive: string; password: string | null }
  | { kind: "modify"; archive: string; edit: Edit; options: CreateOptions }
  | { kind: "convert"; archive: string; out: string; password: string | null; options: CreateOptions };

export interface Snapshot {
  total: number;
  done: number;
  packed: number;
  filesTotal: number;
  filesDone: number;
  current: string;
  paused: boolean;
  elapsedMs: number;
  warnings: string[];
}

export interface DoneEvent {
  id: number;
  ok: boolean;
  result: Record<string, unknown> | null;
  code: string | null;
  message: string | null;
  snap: Snapshot;
}

export type Task =
  | { mode: "browse"; path: string }
  | { mode: "add"; sources: string[]; email: boolean; suggested: string }
  | { mode: "extract"; archive: string; dest: string }
  | { mode: "run"; title: string; jobs: JobSpec[] };

export interface Settings {
  defaultFormat: Format;
  defaultLevel: number;
  zipMethod: ZipMethod;
  solid: boolean;
  smart: boolean;
  smartExtractHere: boolean;
  openFolderAfterExtract: boolean;
  theme: string;
  lastFolder: string | null;
  menuAddDialog: boolean;
  menuAddQuick: boolean;
  menuEmail: boolean;
  menuExtract: boolean;
  menuTest: boolean;
}

export interface Integration {
  classic: boolean;
  modern: boolean;
  installDir: string;
  dllPresent: boolean;
}

export interface Estimate {
  format: Format;
  level: number;
  label: string;
  ratio: number;
  mbPerS: number;
  estimatedBytes: number;
  estimatedMs: number;
}

export interface CmdError {
  code: string;
  message: string;
}

export function toError(e: unknown): CmdError {
  if (typeof e === "string") {
    try {
      const v = JSON.parse(e);
      if (v && typeof v.code === "string") return v;
    } catch {
      /* texto simples */
    }
    return { code: "error", message: e };
  }
  if (e && typeof e === "object" && "message" in e) return { code: "error", message: String((e as Error).message) };
  return { code: "error", message: String(e) };
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauri) return mock(cmd, args ?? {}) as Promise<T>;
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    throw toError(e);
  }
}

export const api = {
  taskGet: () => call<Task | null>("task_get"),
  jobStart: (spec: JobSpec) => call<number>("job_start", { spec }),
  jobPause: (id: number, paused: boolean) => call<void>("job_pause", { id, paused }),
  jobCancel: (id: number) => call<void>("job_cancel", { id }),
  listDir: (path: string) => call<DirListing>("list_dir", { path }),
  places: () => call<Place[]>("places"),
  typeInfos: (keys: string[]) => call<Record<string, TypeInfo>>("type_infos", { keys }),
  archiveInfo: (path: string, password?: string | null) => call<ArchiveInfo>("archive_info", { path, password: password ?? null }),
  detect: (path: string) => call<Format | null>("detect", { path }),
  settingsGet: () => call<Settings>("settings_get"),
  settingsSet: (value: Settings) => call<void>("settings_set", { value }),
  openPath: (path: string) => call<void>("open_path", { path }),
  revealPath: (path: string) => call<void>("reveal_path", { path }),
  suggestOut: (sources: string[], format: Format) => call<string>("suggest_out", { sources, format }),
  defaultExtractDir: (archive: string) => call<string>("default_extract_dir", { archive }),
  previewEntry: (archive: string, entry: string, password?: string | null) =>
    call<string>("preview_entry", { archive, entry, password: password ?? null }),
  integrationStatus: () => call<Integration>("integration_status"),
  integrationSet: (classic?: boolean, modern?: boolean) =>
    call<Integration>("integration_set", { classic: classic ?? null, modern: modern ?? null }),
  estimateFormats: (sources: string[]) => call<Estimate[]>("estimate_formats", { sources }),
};

export function onEvent<T>(name: string, cb: (payload: T) => void): () => void {
  if (!isTauri) {
    const h = (e: Event) => cb((e as CustomEvent<T>).detail);
    window.addEventListener(`mock:${name}`, h);
    return () => window.removeEventListener(`mock:${name}`, h);
  }
  let un: UnlistenFn | null = null;
  let dead = false;
  void listen<T>(name, (e) => cb(e.payload)).then((u) => (dead ? u() : (un = u)));
  return () => {
    dead = true;
    un?.();
  };
}

// --------------------------------------------------------------------- dados simulados

const now = Math.floor(Date.now() / 1000);
const DOCS = "C:\\Users\\Voce\\Documents";

const mockFs: Record<string, FsEntry[]> = {
  [DOCS]: [
    { name: "Projetos", path: `${DOCS}\\Projetos`, isDir: true, size: 0, modified: now - 3600, hidden: false },
    { name: "Fotos 2026", path: `${DOCS}\\Fotos 2026`, isDir: true, size: 0, modified: now - 86400, hidden: false },
    { name: "Relatório anual.pdf", path: `${DOCS}\\Relatório anual.pdf`, isDir: false, size: 2_483_112, modified: now - 7200, hidden: false },
    { name: "Projeto Ação.7z", path: `${DOCS}\\Projeto Ação.7z`, isDir: false, size: 48_660_112, modified: now - 600, hidden: false },
    { name: "backup-servidor.tar.zst", path: `${DOCS}\\backup-servidor.tar.zst`, isDir: false, size: 812_440_001, modified: now - 172800, hidden: false },
    { name: "Ícone 3D de arquivos compactados multicoloridos.png", path: `${DOCS}\\Ícone 3D de arquivos compactados multicoloridos.png`, isDir: false, size: 1_475_889, modified: now - 300, hidden: false },
    { name: "planilha.xlsx", path: `${DOCS}\\planilha.xlsx`, isDir: false, size: 88_214, modified: now - 99999, hidden: false },
    { name: "notas.txt", path: `${DOCS}\\notas.txt`, isDir: false, size: 3_120, modified: now - 50000, hidden: false },
  ],
};

const mockArchive: ArchiveInfo = {
  path: `${DOCS}\\Projeto Ação.7z`,
  format: "7z",
  solid: true,
  encryptedNames: false,
  comment: null,
  sizeOnDisk: 48_660_112,
  volumes: 1,
  hasRecovery: false,
  locked: false,
  blocks: 12,
  entries: [
    { path: "Projeto Ação", isDir: true, size: 0, packed: 0, modified: now - 900, crc: null, encrypted: false, method: null },
    { path: "Projeto Ação/docs", isDir: true, size: 0, packed: 0, modified: now - 900, crc: null, encrypted: false, method: null },
    { path: "Projeto Ação/docs/dados.csv", isDir: false, size: 1_288_890, packed: null, modified: now - 1000, crc: 0x1a2b3c4d, encrypted: false, method: "LZMA2" },
    { path: "Projeto Ação/docs/manual.pdf", isDir: false, size: 3_002_331, packed: null, modified: now - 4000, crc: 0x99887766, encrypted: false, method: "LZMA2" },
    { path: "Projeto Ação/app.exe", isDir: false, size: 1_400_000, packed: 512_000, modified: now - 2000, crc: 0x0badf00d, encrypted: false, method: "LZMA2 + BCJ_X86" },
    { path: "Projeto Ação/foto.jpg", isDir: false, size: 50_000, packed: 50_000, modified: now - 3000, crc: 0x12345678, encrypted: false, method: "COPY" },
    { path: "Projeto Ação/leia-me.txt", isDir: false, size: 55_000, packed: null, modified: now - 100, crc: 0xcafebabe, encrypted: false, method: "LZMA2" },
  ],
};

const mockSettings: Settings = {
  defaultFormat: "7z",
  defaultLevel: 5,
  zipMethod: "deflate",
  solid: true,
  smart: true,
  smartExtractHere: true,
  openFolderAfterExtract: false,
  theme: "padrao",
  lastFolder: DOCS,
  menuAddDialog: true,
  menuAddQuick: true,
  menuEmail: true,
  menuExtract: true,
  menuTest: true,
};

let mockJob = 0;

function mockTask(): Task {
  const q = new URLSearchParams(location.search);
  const mode = q.get("mode") ?? "browse";
  if (mode === "add")
    return {
      mode: "add",
      sources: [`${DOCS}\\Ícone 3D de arquivos compactados multicoloridos.png`],
      email: q.has("email"),
      suggested: `${DOCS}\\Ícone 3D de arquivos compactados multicoloridos.7z`,
    };
  if (mode === "extract") return { mode: "extract", archive: mockArchive.path, dest: `${DOCS}\\Projeto Ação` };
  if (mode === "run")
    return {
      mode: "run",
      title: "Compactando",
      jobs: [
        { kind: "create", sources: [`${DOCS}\\Projetos`], out: `${DOCS}\\Projetos.7z`, options: defaultCreate("7z") },
        { kind: "extract", archive: `${DOCS}\\backup-servidor.tar.zst`, dest: DOCS, options: { overwrite: "overwrite", selection: [], threads: 0 } },
      ],
    };
  return { mode: "browse", path: q.get("path") ?? DOCS };
}

export function defaultCreate(format: Format): CreateOptions {
  return {
    format,
    level: 5,
    zipMethod: "deflate",
    password: null,
    encryptNames: false,
    threads: 0,
    solid: true,
    dictionaryMb: 0,
    volumeSize: 0,
    deleteSources: false,
    smart: true,
    comment: null,
    exclude: [],
    testAfter: false,
  };
}

async function mock(cmd: string, a: Record<string, unknown>): Promise<unknown> {
  await new Promise((r) => setTimeout(r, 40));
  switch (cmd) {
    case "task_get":
      return mockTask();
    case "list_dir": {
      const path = String(a.path);
      const entries = mockFs[path] ?? [];
      const parent = path.includes("\\") ? path.slice(0, path.lastIndexOf("\\")) : null;
      return { path, parent: parent && parent.length > 2 ? parent : null, entries };
    }
    case "places":
      return [
        { label: "Área de Trabalho", path: "C:\\Users\\Voce\\Desktop", kind: "desktop", free: null, total: null },
        { label: "Documentos", path: DOCS, kind: "documents", free: null, total: null },
        { label: "Downloads", path: "C:\\Users\\Voce\\Downloads", kind: "downloads", free: null, total: null },
        { label: "Disco Local (C:)", path: "C:\\", kind: "drive", free: 210e9, total: 1000e9 },
      ];
    case "type_infos":
      return Object.fromEntries(
        (a.keys as string[]).map((k) => [k, { icon: null, name: k === "<folder>" ? "Pasta de arquivos" : `Arquivo ${k.slice(1).toUpperCase()}` }]),
      );
    case "archive_info":
      return mockArchive;
    case "detect": {
      const p = String(a.path).toLowerCase();
      return p.endsWith(".7z") ? "7z" : p.endsWith(".tar.zst") ? "tar.zst" : p.endsWith(".zip") ? "zip" : null;
    }
    case "settings_get":
      return mockSettings;
    case "suggest_out": {
      const s = (a.sources as string[])[0];
      return s.replace(/\.[^.\\]+$/, "") + "." + String(a.format);
    }
    case "default_extract_dir":
      return String(a.archive).replace(/\.[^.\\]+$/, "");
    case "integration_status":
    case "integration_set":
      return { classic: true, modern: false, installDir: "C:\\Users\\Voce\\Documents\\RustZIP", dllPresent: true };
    case "estimate_formats":
      return [
        { format: "zip", level: 5, label: "ZIP · Normal (compatível)", ratio: 0.38, mbPerS: 230, estimatedBytes: 67e6, estimatedMs: 800 },
        { format: "tar.zst", level: 3, label: "TAR.ZST · Rápido", ratio: 0.33, mbPerS: 410, estimatedBytes: 58e6, estimatedMs: 450 },
        { format: "tar.zst", level: 7, label: "TAR.ZST · Bom", ratio: 0.28, mbPerS: 86, estimatedBytes: 49.5e6, estimatedMs: 2270 },
        { format: "7z", level: 1, label: "7Z · Mais rápido", ratio: 0.31, mbPerS: 100, estimatedBytes: 54.6e6, estimatedMs: 1780 },
        { format: "7z", level: 5, label: "7Z · Normal", ratio: 0.27, mbPerS: 20, estimatedBytes: 48.6e6, estimatedMs: 9030 },
        { format: "7z", level: 9, label: "7Z · Melhor", ratio: 0.25, mbPerS: 4.2, estimatedBytes: 45.1e6, estimatedMs: 42500 },
      ];
    case "job_start": {
      const id = ++mockJob;
      const total = 180e6;
      let done = 0;
      const t0 = Date.now();
      const fail = new URLSearchParams(location.search).get("fail");
      const timer = setInterval(() => {
        done = Math.min(total, done + total / 25);
        const snap: Snapshot = {
          total, done, packed: done * 0.3, filesTotal: 8775, filesDone: Math.floor((8775 * done) / total),
          current: "node_modules/.pnpm/typescript@5.9.3/node_modules/typescript/lib/typescript.js",
          paused: false, elapsedMs: Date.now() - t0, warnings: [],
        };
        window.dispatchEvent(new CustomEvent("mock:job-progress", { detail: { id, snap } }));
        if (done >= total || (fail && done > total / 3)) {
          clearInterval(timer);
          const ev: DoneEvent = fail
            ? { id, ok: false, result: null, code: fail, message: fail === "password_required" ? "O arquivo está protegido por senha" : "Falhou", snap }
            : { id, ok: true, result: { outputs: [`${DOCS}\\Projetos.7z`], inputBytes: total, outputBytes: total * 0.27, files: 8775, elapsedMs: 4200, dest: DOCS }, code: null, message: null, snap };
          window.dispatchEvent(new CustomEvent("mock:job-done", { detail: ev }));
        }
      }, 160);
      return id;
    }
    default:
      return null;
  }
}
