import { getCurrentWebview } from "@tauri-apps/api/webview";
import {
  ArrowUp, BarChart3, ChevronRight, CircleCheckBig, Eye, FilePlus2, FileArchive, FolderOpen, HardDrive, Info, KeyRound,
  Lock, PackageOpen, RefreshCw, Repeat, Search, Settings2, Trash2, Download, Monitor, FileText, Image as ImageIcon,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { AddForm, EstimateTable } from "./AddDialog";
import { api, defaultCreate, isTauri, onEvent, type ArchiveInfo, type Estimate, type Format, type JobSpec, type Place, type TypeInfo } from "./api";
import { ExtractForm } from "./ExtractDialog";
import * as f from "./fmt";
import { JobCard } from "./JobView";
import { SettingsForm } from "./SettingsDialog";
import { Footer, Label, Modal, Section } from "./ui";

type Loc = { kind: "fs"; path: string } | { kind: "archive"; path: string; inner: string };

interface Row {
  key: string;
  name: string;
  isDir: boolean;
  size: number;
  packed: number | null;
  modified: number | null;
  crc: number | null;
  method: string | null;
  encrypted: boolean;
  /** Caminho no disco (fs) ou da entrada (arquivo compactado). */
  full: string;
  isArchive: boolean;
}

type SortKey = "name" | "size" | "packed" | "type" | "modified" | "crc";

type Dialog =
  | { kind: "add"; sources: string[]; suggested: string; format?: Format }
  | { kind: "extract"; selection: string[] }
  | { kind: "settings" }
  | { kind: "info" }
  | { kind: "estimate"; sources: string[] }
  | { kind: "convert" }
  | { kind: "rename"; from: string }
  | { kind: "delete"; entries: string[] }
  | { kind: "password"; path: string; wrong: boolean }
  | { kind: "jobs" };

const sep = (p: string) => (p.endsWith("\\") ? p : `${p}\\`);

function archiveRows(info: ArchiveInfo, inner: string, query: string): Row[] {
  const prefix = inner ? `${inner}/` : "";
  const q = query.trim().toLowerCase();
  if (q) {
    return info.entries
      .filter((e) => !e.isDir && e.path.toLowerCase().includes(q))
      .map((e) => ({ key: e.path, name: e.path, isDir: false, size: e.size, packed: e.packed, modified: e.modified, crc: e.crc, method: e.method, encrypted: e.encrypted, full: e.path, isArchive: false }));
  }
  const dirs = new Map<string, Row>();
  const files: Row[] = [];
  for (const e of info.entries) {
    if (!e.path.startsWith(prefix) || e.path === inner) continue;
    const rest = e.path.slice(prefix.length);
    const slash = rest.indexOf("/");
    if (slash < 0 && !e.isDir) {
      files.push({ key: e.path, name: rest, isDir: false, size: e.size, packed: e.packed, modified: e.modified, crc: e.crc, method: e.method, encrypted: e.encrypted, full: e.path, isArchive: false });
      continue;
    }
    const name = slash < 0 ? rest : rest.slice(0, slash);
    const full = prefix + name;
    const d = dirs.get(name) ?? { key: full, name, isDir: true, size: 0, packed: 0, modified: null, crc: null, method: null, encrypted: false, full, isArchive: false };
    if (!e.isDir) {
      d.size += e.size;
      d.packed = d.packed != null && e.packed != null ? d.packed + e.packed : null;
      d.encrypted ||= e.encrypted;
    }
    if (e.isDir && e.path === full) d.modified = e.modified;
    else d.modified = Math.max(d.modified ?? 0, e.modified ?? 0) || null;
    dirs.set(name, d);
  }
  return [...dirs.values(), ...files];
}

export function Manager({ initial }: { initial: string }) {
  const [loc, setLoc] = useState<Loc>({ kind: "fs", path: initial });
  const [fsRows, setFsRows] = useState<Row[]>([]);
  const [parent, setParent] = useState<string | null>(null);
  const [info, setInfo] = useState<ArchiveInfo | null>(null);
  const [password, setPassword] = useState<string | null>(null);
  const [places, setPlaces] = useState<Place[]>([]);
  const [types, setTypes] = useState<Record<string, TypeInfo>>({});
  const [sel, setSel] = useState<Set<string>>(new Set());
  const [anchor, setAnchor] = useState<string | null>(null);
  const [sort, setSort] = useState<{ key: SortKey; desc: boolean }>({ key: "name", desc: false });
  const [query, setQuery] = useState("");
  const [address, setAddress] = useState(initial);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [dialog, setDialog] = useState<Dialog | null>(null);
  const [jobs, setJobs] = useState<{ id: number; spec: JobSpec; state: string }[]>([]);
  const [dropping, setDropping] = useState(false);
  const jobSeq = useRef(0);
  const listRef = useRef<HTMLDivElement>(null);

  const load = useCallback(async (to: Loc, pw: string | null = password) => {
    setLoading(true);
    setError(null);
    setQuery("");
    try {
      if (to.kind === "fs") {
        const l = await api.listDir(to.path);
        setParent(l.parent);
        setInfo(null);
        setFsRows(
          l.entries
            .filter((e) => !e.hidden)
            .map((e) => ({ key: e.path, name: e.name, isDir: e.isDir, size: e.size, packed: null, modified: e.modified, crc: null, method: null, encrypted: false, full: e.path, isArchive: !e.isDir && f.isArchiveName(e.name) })),
        );
        setAddress(l.path);
      } else {
        if (!info || info.path !== to.path) {
          const i = await api.archiveInfo(to.path, pw);
          setInfo(i);
        }
        setAddress(to.inner ? `${to.path}\\${to.inner.replaceAll("/", "\\")}` : to.path);
      }
      setLoc(to);
      setSel(new Set());
      listRef.current?.scrollTo({ top: 0 });
    } catch (e) {
      const err = e as { code: string; message: string };
      if (to.kind === "archive" && (err.code === "password_required" || err.code === "wrong_password")) {
        setDialog({ kind: "password", path: to.path, wrong: err.code === "wrong_password" });
      } else {
        setError(err.message);
      }
    } finally {
      setLoading(false);
    }
  }, [info, password]);

  useEffect(() => {
    void openPath(initial);
    void api.places().then(setPlaces);
    // Páginas de teste visual (fora do Tauri): `?dialog=settings|estimate|password` abre direto.
    const lab = isTauri ? null : new URLSearchParams(location.search).get("dialog");
    if (lab === "settings") setDialog({ kind: "settings" });
    if (lab === "estimate") setDialog({ kind: "estimate", sources: [initial] });
    if (lab === "password") setDialog({ kind: "password", path: initial, wrong: false });
    return onEvent<string>("navigate", (p) => void openPath(p));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Arquivos arrastados do Explorer: para dentro do arquivo aberto, ou para um novo arquivo.
  useEffect(() => {
    if (!isTauri) return;
    let un: (() => void) | undefined;
    void getCurrentWebview()
      .onDragDropEvent((e) => {
        if (e.payload.type === "over" || e.payload.type === "enter") setDropping(true);
        else if (e.payload.type === "leave") setDropping(false);
        else if (e.payload.type === "drop") {
          setDropping(false);
          const paths = e.payload.paths;
          if (!paths.length) return;
          if (loc.kind === "archive" && info && info.format !== "rar") {
            runJob({ kind: "modify", archive: loc.path, edit: { delete: [], rename: [], add: paths, addTo: loc.inner }, options: { ...defaultCreate(info.format), password } });
          } else {
            void api.settingsGet().then(async (s) => setDialog({ kind: "add", sources: paths, suggested: await api.suggestOut(paths, s.defaultFormat) }));
          }
        }
      })
      .then((u) => (un = u));
    return () => un?.();
  });

  // Teste visual: `?dialog=info|convert|delete|rename|extract|add|jobs` depois que o arquivo abriu.
  useEffect(() => {
    if (isTauri || !info) return;
    const lab = new URLSearchParams(location.search).get("dialog");
    const e = info.entries.find((x) => !x.isDir);
    if (lab === "info") setDialog({ kind: "info" });
    if (lab === "convert") setDialog({ kind: "convert" });
    if (lab === "delete" && e) setDialog({ kind: "delete", entries: [e.path] });
    if (lab === "rename" && e) setDialog({ kind: "rename", from: e.path });
    if (lab === "extract") setDialog({ kind: "extract", selection: [] });
    if (lab === "jobs") runJob({ kind: "test", archive: info.path, password: null });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [info]);

  const rows = useMemo(() => {
    const base = loc.kind === "fs" ? fsRows.filter((r) => !query || r.name.toLowerCase().includes(query.toLowerCase())) : info ? archiveRows(info, loc.inner, query) : [];
    const dir = sort.desc ? -1 : 1;
    const typeName = (r: Row) => (r.isDir ? "" : types[f.ext(r.name)]?.name ?? f.ext(r.name));
    const cmp = (a: Row, b: Row): number => {
      if (a.isDir !== b.isDir) return a.isDir ? -1 : 1;
      switch (sort.key) {
        case "size": return (a.size - b.size) * dir;
        case "packed": return ((a.packed ?? -1) - (b.packed ?? -1)) * dir;
        case "modified": return ((a.modified ?? 0) - (b.modified ?? 0)) * dir;
        case "crc": return ((a.crc ?? 0) - (b.crc ?? 0)) * dir;
        case "type": return typeName(a).localeCompare(typeName(b), "pt-BR") * dir || a.name.localeCompare(b.name, "pt-BR", { numeric: true });
        default: return a.name.localeCompare(b.name, "pt-BR", { numeric: true, sensitivity: "base" }) * dir;
      }
    };
    return [...base].sort(cmp);
  }, [loc, fsRows, info, query, sort, types]);

  // Ícones e nomes de tipo do Windows, por extensão.
  useEffect(() => {
    const keys = new Set<string>();
    for (const r of rows) {
      if (r.isDir) keys.add("<folder>");
      else {
        const e = f.ext(r.name);
        if (loc.kind === "fs" && [".exe", ".lnk", ".ico", ".url"].includes(e)) keys.add(r.full);
        else if (e) keys.add(e);
      }
    }
    const missing = [...keys].filter((k) => !(k in types));
    if (missing.length) void api.typeInfos(missing).then((t) => setTypes((old) => ({ ...old, ...t })));
  }, [rows, loc.kind, types]);

  const selected = rows.filter((r) => sel.has(r.key));
  const inArchive = loc.kind === "archive";
  const readOnly = inArchive && info?.format === "rar";

  async function openPath(p: string) {
    const fmt = await api.detect(p).catch(() => null);
    if (fmt) {
      setPassword(null);
      setInfo(null);
      void load({ kind: "archive", path: p, inner: "" }, null);
    } else void load({ kind: "fs", path: p });
  }

  function activate(r: Row) {
    if (loc.kind === "fs") {
      if (r.isDir) void load({ kind: "fs", path: r.full });
      else if (r.isArchive) void openPath(r.full);
      else void api.openPath(r.full);
    } else if (r.isDir) {
      void load({ ...loc, inner: r.full });
    } else {
      void api.previewEntry(loc.path, r.full, password).catch((e) => setError((e as { message: string }).message));
    }
  }

  function up() {
    if (loc.kind === "archive") {
      if (loc.inner) void load({ ...loc, inner: loc.inner.includes("/") ? loc.inner.slice(0, loc.inner.lastIndexOf("/")) : "" });
      else void load({ kind: "fs", path: f.dirName(loc.path) });
    } else if (parent) void load({ kind: "fs", path: parent });
  }

  function clickRow(r: Row, e: React.MouseEvent) {
    if (e.shiftKey && anchor) {
      const keys = rows.map((x) => x.key);
      const [a, b] = [keys.indexOf(anchor), keys.indexOf(r.key)].sort((x, y) => x - y);
      setSel(new Set(keys.slice(a, b + 1)));
    } else if (e.ctrlKey) {
      const n = new Set(sel);
      n.has(r.key) ? n.delete(r.key) : n.add(r.key);
      setSel(n);
      setAnchor(r.key);
    } else {
      setSel(new Set([r.key]));
      setAnchor(r.key);
    }
  }

  function runJob(spec: JobSpec) {
    const id = ++jobSeq.current;
    setJobs((j) => [...j, { id, spec, state: "running" }]);
    setDialog({ kind: "jobs" });
  }

  function jobFinished(id: number, state: string) {
    setJobs((j) => j.map((x) => (x.id === id ? { ...x, state } : x)));
    if (state === "ok") {
      if (loc.kind === "archive") {
        setInfo(null);
        void api.archiveInfo(loc.path, password).then(setInfo).catch(() => undefined);
      } else void load(loc);
    }
  }

  // ------------------------------------------------------------------ ações

  async function actAdd() {
    if (inArchive) return;
    const sources = selected.length ? selected.map((r) => r.full) : [];
    if (!sources.length) return setError("Selecione arquivos ou pastas para adicionar.");
    const s = await api.settingsGet();
    setDialog({ kind: "add", sources, suggested: await api.suggestOut(sources, s.defaultFormat) });
  }

  function actExtract() {
    if (loc.kind === "archive") return setDialog({ kind: "extract", selection: selected.map((r) => r.full) });
    const arch = selected.find((r) => r.isArchive);
    if (arch) void openPath(arch.full).then(() => setDialog({ kind: "extract", selection: [] }));
  }

  function actTest() {
    if (loc.kind === "archive") return runJob({ kind: "test", archive: loc.path, password });
    for (const r of selected.filter((x) => x.isArchive)) runJob({ kind: "test", archive: r.full, password: null });
  }

  function actDelete() {
    if (loc.kind !== "archive" || readOnly || !selected.length) return;
    setDialog({ kind: "delete", entries: selected.map((r) => r.full) });
  }

  const onKey = (e: React.KeyboardEvent) => {
    if ((e.target as HTMLElement).tagName === "INPUT") return;
    if (e.key === "Enter" && selected[0]) activate(selected[0]);
    else if (e.key === "Backspace") up();
    else if (e.key === "Delete") actDelete();
    else if (e.key === "F2" && inArchive && !readOnly && selected[0]) setDialog({ kind: "rename", from: selected[0].full });
    else if (e.key === "F5") void load(loc);
    else if (e.key.toLowerCase() === "a" && e.ctrlKey) {
      e.preventDefault();
      setSel(new Set(rows.map((r) => r.key)));
    } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const i = rows.findIndex((r) => r.key === anchor);
      const n = rows[Math.max(0, Math.min(rows.length - 1, i + (e.key === "ArrowDown" ? 1 : -1)))];
      if (n) {
        setSel(new Set([n.key]));
        setAnchor(n.key);
        document.getElementById(`row-${n.key}`)?.scrollIntoView({ block: "nearest" });
      }
    }
  };

  const totalSize = rows.reduce((a, r) => a + (r.isDir ? 0 : r.size), 0);
  const selSize = selected.reduce((a, r) => a + r.size, 0);
  const crumbs = loc.kind === "fs" ? loc.path.split("\\").filter(Boolean) : [...f.baseName(loc.path).split("\\"), ...(loc.inner ? loc.inner.split("/") : [])];

  return (
    <div className="app-shell flex h-full flex-col outline-none" tabIndex={0} onKeyDown={onKey}>
      {/* barra de ferramentas */}
      <div className="app-pane glass flex items-end gap-0.5 border-b border-[var(--line)] bg-[var(--panel)] px-2 pt-1.5 pb-1">
        <Tool icon={<FilePlus2 size={26} className="text-[var(--i-add)]" />} label="Adicionar" onClick={actAdd} disabled={inArchive || !selected.length} />
        <Tool icon={<PackageOpen size={26} className="text-[var(--i-extract)]" />} label="Extrair para" onClick={actExtract} disabled={!inArchive && !selected.some((r) => r.isArchive)} />
        <Tool icon={<CircleCheckBig size={26} className="text-[var(--i-test)]" />} label="Testar" onClick={actTest} disabled={!inArchive && !selected.some((r) => r.isArchive)} />
        <Tool icon={<Eye size={26} className="text-[var(--i-view)]" />} label="Visualizar" onClick={() => selected[0] && activate(selected[0])} disabled={!selected.length || selected[0].isDir} />
        <Tool icon={<Trash2 size={26} className="text-[var(--i-del)]" />} label="Excluir" onClick={actDelete} disabled={!inArchive || readOnly || !selected.length} />
        <Sep />
        <Tool icon={<Repeat size={26} className="text-[var(--i-conv)]" />} label="Converter" onClick={() => setDialog({ kind: "convert" })} disabled={!inArchive} />
        <Tool icon={<Info size={26} className="text-[var(--i-info)]" />} label="Informações" onClick={() => setDialog({ kind: "info" })} disabled={!inArchive} />
        <Tool icon={<BarChart3 size={26} className="text-[var(--i-analyze)]" />} label="Analisar" onClick={() => setDialog({ kind: "estimate", sources: selected.map((r) => r.full) })} disabled={inArchive || !selected.length} />
        <div className="flex-1" />
        <Tool icon={<Settings2 size={26} className="text-[var(--muted)]" />} label="Opções" onClick={() => setDialog({ kind: "settings" })} />
        <img src="/rustzip.avif" alt="RustZIP" className="mx-2 mb-1 h-11 w-11 self-center" />
      </div>

      {/* endereço */}
      <div className="app-pane glass flex items-center gap-2 border-b border-[var(--line)] bg-[var(--panel)] px-2 py-1.5">
        <button className="btn btn-ghost !px-2" onClick={up} title="Acima (Backspace)" disabled={loc.kind === "fs" && !parent}>
          <ArrowUp size={16} />
        </button>
        <button className="btn btn-ghost !px-2" onClick={() => void load(loc)} title="Atualizar (F5)">
          <RefreshCw size={15} className={loading ? "animate-spin" : ""} />
        </button>
        <form className="field flex min-w-0 flex-1 items-center gap-1 !py-1" onSubmit={(e) => { e.preventDefault(); void openPath(address.trim()); }}>
          {inArchive ? <FileArchive size={15} className="flex-none text-[var(--accent)]" /> : <FolderOpen size={15} className="flex-none text-[var(--i-folder)]" />}
          <input className="min-w-0 flex-1 bg-transparent outline-none" value={address} onChange={(e) => setAddress(e.target.value)} spellCheck={false} />
        </form>
        <div className="field flex w-56 items-center gap-1.5 !py-1">
          <Search size={14} className="text-[var(--faint)]" />
          <input className="min-w-0 flex-1 bg-transparent outline-none" placeholder={inArchive ? "Localizar no arquivo" : "Filtrar esta pasta"} value={query} onChange={(e) => setQuery(e.target.value)} />
        </div>
      </div>

      <div className="app-row flex min-h-0 flex-1">
        {/* locais */}
        <nav className="app-pane glass w-48 flex-none overflow-auto border-r border-[var(--line)] bg-[var(--panel)] p-1.5">
          {places.map((p) => (
            <button
              key={p.path}
              onClick={() => void load({ kind: "fs", path: p.path })}
              className={`flex w-full items-center gap-2 rounded px-2 py-1.5 text-left hover:bg-[var(--hover)] ${loc.kind === "fs" && sep(loc.path) === sep(p.path) ? "bg-[var(--accent-soft)] font-semibold" : ""}`}
              title={p.total ? `${f.size(p.free)} livres de ${f.size(p.total)}` : p.path}
            >
              {placeIcon(p.kind)}
              <span className="min-w-0 flex-1 truncate">{p.label}</span>
            </button>
          ))}
        </nav>

        <div className="app-pane glass relative flex min-w-0 flex-1 flex-col bg-[var(--panel-2)]">
          {inArchive && info && <ArchiveBanner info={info} crumbs={crumbs} onCrumb={(i) => load({ ...(loc as Extract<Loc, { kind: "archive" }>), inner: crumbs.slice(1, i + 1).join("/") })} />}
          <div ref={listRef} className="min-h-0 flex-1 overflow-auto" onMouseDown={(e) => e.target === e.currentTarget && setSel(new Set())}>
            <table className="w-full table-fixed border-collapse">
              <colgroup>
                <col />
                <col className="w-24" />
                {inArchive && <col className="w-24" />}
                <col className="w-40" />
                <col className="w-36" />
                {inArchive && <col className="w-24" />}
                {inArchive && <col className="w-32" />}
              </colgroup>
              <thead className="glass sticky top-0 z-10 bg-[var(--panel-2)] text-left text-[12px] text-[var(--muted)]">
                <tr className="border-b border-[var(--line)]">
                  <Th k="name" sort={sort} setSort={setSort} className="pl-3">Nome</Th>
                  <Th k="size" sort={sort} setSort={setSort} right>Tamanho</Th>
                  {inArchive && <Th k="packed" sort={sort} setSort={setSort} right>Compactado</Th>}
                  <Th k="type" sort={sort} setSort={setSort}>Tipo</Th>
                  <Th k="modified" sort={sort} setSort={setSort}>Modificado</Th>
                  {inArchive && <Th k="crc" sort={sort} setSort={setSort}>CRC32</Th>}
                  {inArchive && <th className="px-2 py-1.5 font-normal">Método</th>}
                </tr>
              </thead>
              <tbody>
                {(loc.kind === "archive" || parent) && !query && (
                  <tr className="hover:bg-[var(--hover)]" onDoubleClick={up}>
                    <td className="py-[3px] pl-3" colSpan={inArchive ? 7 : 4}>
                      <span className="inline-flex items-center gap-2 text-[var(--muted)]">
                        <ArrowUp size={15} /> ..
                      </span>
                    </td>
                  </tr>
                )}
                {rows.map((r) => {
                  const t = r.isDir ? types["<folder>"] : types[loc.kind === "fs" && [".exe", ".lnk", ".ico", ".url"].includes(f.ext(r.name)) ? r.full : f.ext(r.name)];
                  return (
                    <tr
                      key={r.key}
                      id={`row-${r.key}`}
                      className={`${sel.has(r.key) ? "row-sel" : "hover:bg-[var(--hover)]"}`}
                      onMouseDown={(e) => clickRow(r, e)}
                      onDoubleClick={() => activate(r)}
                    >
                      <td className="py-[3px] pl-3 pr-2">
                        <span className="flex items-center gap-2">
                          {t?.icon ? <img src={t.icon} alt="" className="h-4 w-4 flex-none" /> : r.isArchive ? <img src="/rustzip.avif" alt="" className="h-4 w-4 flex-none" /> : <span className="h-4 w-4 flex-none rounded-sm bg-[var(--line)]" />}
                          <span className="truncate" title={r.name}>{r.name}</span>
                          {r.encrypted && <Lock size={12} className="flex-none text-[var(--warn)]" />}
                        </span>
                      </td>
                      <td className="px-2 text-right tabular-nums whitespace-nowrap text-[var(--muted)]">{r.isDir && !inArchive ? "" : f.size(r.size)}</td>
                      {inArchive && <td className="px-2 text-right tabular-nums whitespace-nowrap text-[var(--muted)]">{r.packed == null ? "" : f.size(r.packed)}</td>}
                      <td className="truncate px-2 whitespace-nowrap text-[var(--muted)]">{r.isDir ? (t?.name ?? "Pasta") : (t?.name ?? f.ext(r.name).slice(1).toUpperCase())}</td>
                      <td className="px-2 tabular-nums whitespace-nowrap text-[var(--muted)]">{f.date(r.modified)}</td>
                      {inArchive && <td className="px-2 font-mono text-[12px] text-[var(--faint)]">{f.crc(r.crc)}</td>}
                      {inArchive && <td className="truncate px-2 text-[12px] text-[var(--faint)]" title={r.method ?? ""}>{r.method}</td>}
                    </tr>
                  );
                })}
              </tbody>
            </table>
            {!rows.length && !loading && <div className="p-10 text-center text-[var(--faint)]">{query ? "Nada encontrado." : "Pasta vazia."}</div>}
          </div>
          {dropping && (
            <div className="pointer-events-none absolute inset-2 grid place-items-center rounded-xl border-2 border-dashed border-[var(--accent)] bg-[var(--accent-soft)]/80 text-[15px] font-semibold text-[var(--accent)]">
              {inArchive && !readOnly ? `Adicionar a ${f.baseName(loc.path)}` : "Solte para compactar"}
            </div>
          )}
        </div>
      </div>

      {/* status */}
      <div className="app-pane glass flex items-center gap-3 border-t border-[var(--line)] bg-[var(--panel)] px-3 py-1 text-[12px] text-[var(--muted)]">
        {error ? (
          <span className="truncate text-[var(--danger)]">{error}</span>
        ) : selected.length ? (
          <span>Selecionado: {selected.length} {selected.length === 1 ? "item" : "itens"}, {f.size(selSize)}</span>
        ) : (
          <span>Total: {rows.filter((r) => r.isDir).length} pastas e {f.size(totalSize)} em {rows.filter((r) => !r.isDir).length} arquivos</span>
        )}
        <div className="flex-1" />
        {jobs.some((j) => j.state === "running") && (
          <button className="btn btn-ghost !min-h-5 !px-2 text-[var(--accent)]" onClick={() => setDialog({ kind: "jobs" })}>
            <RefreshCw size={12} className="animate-spin" /> {jobs.filter((j) => j.state === "running").length} em andamento
          </button>
        )}
      </div>

      {dialog?.kind === "add" && (
        <Modal title="Nome e parâmetros do arquivo" width={660} onClose={() => setDialog(null)}>
          <AddForm sources={dialog.sources} suggested={dialog.format ? f.withExt(dialog.suggested, dialog.format) : dialog.suggested} onCancel={() => setDialog(null)} onStart={runJob} />
        </Modal>
      )}
      {dialog?.kind === "extract" && loc.kind === "archive" && (
        <ExtractFormModal archive={loc.path} selection={dialog.selection} stripPrefix={dialog.selection.length ? loc.inner || null : null} password={password} onClose={() => setDialog(null)} onStart={runJob} />
      )}
      {dialog?.kind === "settings" && (
        <Modal title="Opções do RustZIP" width={720} onClose={() => setDialog(null)}>
          <SettingsForm onClose={() => setDialog(null)} />
        </Modal>
      )}
      {dialog?.kind === "info" && info && <InfoModal info={info} onClose={() => setDialog(null)} />}
      {dialog?.kind === "estimate" && (
        <EstimateModal sources={dialog.sources} onClose={() => setDialog(null)} onPick={async (e) => {
          const out = await api.suggestOut(dialog.sources, e.format);
          setDialog({ kind: "add", sources: dialog.sources, suggested: out, format: e.format });
        }} />
      )}
      {dialog?.kind === "convert" && loc.kind === "archive" && (
        <ConvertModal archive={loc.path} password={password} onClose={() => setDialog(null)} onStart={runJob} />
      )}
      {dialog?.kind === "rename" && loc.kind === "archive" && info && (
        <PromptModal title="Renomear" initial={dialog.from.split("/").pop()!} onClose={() => setDialog(null)} onOk={(name) => {
          const to = dialog.from.includes("/") ? `${dialog.from.slice(0, dialog.from.lastIndexOf("/"))}/${name}` : name;
          runJob({ kind: "modify", archive: loc.path, edit: { delete: [], rename: [[dialog.from, to]], add: [], addTo: "" }, options: { ...defaultCreate(info.format), password } });
        }} />
      )}
      {dialog?.kind === "delete" && loc.kind === "archive" && info && (
        <Modal title="Excluir do arquivo" width={440} onClose={() => setDialog(null)}>
          <div className="px-4 pb-4">Excluir {dialog.entries.length === 1 ? `"${dialog.entries[0].split("/").pop()}"` : `${dialog.entries.length} itens`} de {f.baseName(loc.path)}? Só o arquivo compactado é alterado.</div>
          <Footer>
            <button className="btn btn-primary min-w-24" onClick={() => runJob({ kind: "modify", archive: loc.path, edit: { delete: dialog.entries, rename: [], add: [], addTo: "" }, options: { ...defaultCreate(info.format), password } })}>Excluir</button>
            <button className="btn min-w-24" onClick={() => setDialog(null)}>Cancelar</button>
          </Footer>
        </Modal>
      )}
      {dialog?.kind === "password" && (
        <PromptModal
          title={dialog.wrong ? "Senha incorreta" : "Arquivo protegido por senha"}
          password
          initial=""
          icon={<KeyRound size={16} className="text-[var(--accent)]" />}
          onClose={() => setDialog(null)}
          onOk={(pw) => {
            setPassword(pw);
            void load({ kind: "archive", path: dialog.path, inner: "" }, pw);
          }}
        />
      )}
      {dialog?.kind === "jobs" && (
        <Modal title="Operações" width={560} onClose={() => setDialog(null)}>
          <div className="flex max-h-[70vh] flex-col gap-3 overflow-auto px-4 pb-4">
            {jobs.map((j) => (
              <JobCard key={j.id} spec={j.spec} onState={(s) => jobFinished(j.id, s)} />
            ))}
          </div>
        </Modal>
      )}
    </div>
  );
}

function placeIcon(kind: string): ReactNode {
  const cls = "flex-none";
  switch (kind) {
    case "desktop": return <Monitor size={15} className={`${cls} text-[var(--i-desktop)]`} />;
    case "documents": return <FileText size={15} className={`${cls} text-[var(--i-docs)]`} />;
    case "downloads": return <Download size={15} className={`${cls} text-[var(--i-down)]`} />;
    case "pictures": return <ImageIcon size={15} className={`${cls} text-[var(--i-pics)]`} />;
    default: return <HardDrive size={15} className={`${cls} text-[var(--muted)]`} />;
  }
}

function Tool({ icon, label, onClick, disabled }: { icon: ReactNode; label: string; onClick: () => void; disabled?: boolean }) {
  return (
    <button className="tool" onClick={onClick} disabled={disabled}>
      {icon}
      <span>{label}</span>
    </button>
  );
}

function Sep() {
  return <div className="mx-1 mb-2 h-10 w-px self-end bg-[var(--line)]" />;
}

function Th({ k, sort, setSort, children, right, className = "" }: {
  k: SortKey;
  sort: { key: SortKey; desc: boolean };
  setSort: (s: { key: SortKey; desc: boolean }) => void;
  children: ReactNode;
  right?: boolean;
  className?: string;
}) {
  const active = sort.key === k;
  return (
    <th
      className={`px-2 py-1.5 font-normal whitespace-nowrap hover:bg-[var(--hover)] ${right ? "text-right" : ""} ${className}`}
      onClick={() => setSort({ key: k, desc: active ? !sort.desc : false })}
    >
      {children}
      {active && <span className="ml-1 text-[10px]">{sort.desc ? "▼" : "▲"}</span>}
    </th>
  );
}

function ArchiveBanner({ info, crumbs, onCrumb }: { info: ArchiveInfo; crumbs: string[]; onCrumb: (i: number) => void }) {
  const files = info.entries.filter((e) => !e.isDir);
  const total = files.reduce((a, e) => a + e.size, 0);
  return (
    <div className="flex items-center gap-2 border-b border-[var(--line)] bg-[var(--accent-soft)] px-3 py-1.5 text-[12px]">
      <img src="/rustzip.avif" alt="" className="h-5 w-5" />
      <div className="flex min-w-0 flex-1 items-center gap-1">
        {crumbs.map((c, i) => (
          <span key={i} className="flex min-w-0 items-center gap-1">
            {i > 0 && <ChevronRight size={12} className="flex-none text-[var(--faint)]" />}
            <button className="truncate hover:underline" onClick={() => onCrumb(i)}>{c}</button>
          </span>
        ))}
      </div>
      <span className="whitespace-nowrap text-[var(--muted)]">
        {info.format.toUpperCase()}
        {info.solid && " · sólido"}
        {info.volumes > 1 && ` · ${info.volumes} volumes`}
        {info.encryptedNames && " · nomes criptografados"}
        {` · ${files.length} arquivos · ${f.size(total)} → ${f.size(info.sizeOnDisk)}`}
        {total > 0 && ` (${f.percent(info.sizeOnDisk / total)})`}
        {info.format === "rar" && " · somente leitura"}
      </span>
    </div>
  );
}

function ExtractFormModal(props: { archive: string; selection: string[]; stripPrefix: string | null; password: string | null; onClose: () => void; onStart: (s: JobSpec) => void }) {
  const [dest, setDest] = useState<string | null>(null);
  useEffect(() => {
    void api.defaultExtractDir(props.archive).then(setDest);
  }, [props.archive]);
  if (!dest) return null;
  return (
    <Modal title="Caminho e opções de extração" width={640} onClose={props.onClose}>
      <ExtractForm archive={props.archive} dest={dest} selection={props.selection} stripPrefix={props.stripPrefix} password={props.password} onCancel={props.onClose} onStart={props.onStart} />
    </Modal>
  );
}

function InfoModal({ info, onClose }: { info: ArchiveInfo; onClose: () => void }) {
  const files = info.entries.filter((e) => !e.isDir);
  const total = files.reduce((a, e) => a + e.size, 0);
  const methods = [...new Set(files.map((e) => e.method).filter(Boolean))];
  const rows: [string, ReactNode][] = [
    ["Arquivo", info.path],
    ["Formato", `${info.format.toUpperCase()}${info.solid ? " (sólido)" : ""}`],
    ["Arquivos", `${files.length} arquivos em ${info.entries.length - files.length} pastas`],
    ["Tamanho original", f.size(total)],
    ["Tamanho compactado", f.size(info.sizeOnDisk)],
    ["Taxa", total ? f.percent(info.sizeOnDisk / total) : "—"],
    ["Métodos", methods.join(", ") || "—"],
    ["Volumes", String(info.volumes)],
    ["Criptografia", files.some((e) => e.encrypted) ? (info.encryptedNames ? "Dados e nomes" : "Dados") : "Nenhuma"],
  ];
  if (info.blocks != null) rows.push(["Blocos 7z", `${info.blocks} (extração paralela por bloco)`]);
  if (info.format === "rar") rows.push(["Registro de recuperação", info.hasRecovery ? "Sim" : "Não"], ["Travado", info.locked ? "Sim" : "Não"]);
  return (
    <Modal title="Informações do arquivo" width={560} onClose={onClose}>
      <div className="px-4 pb-4">
        <table className="w-full">
          <tbody>
            {rows.map(([k, v]) => (
              <tr key={k} className="border-b border-[var(--line)] last:border-0">
                <td className="w-44 py-1.5 text-[var(--muted)]">{k}</td>
                <td className="py-1.5 break-all select-text">{v}</td>
              </tr>
            ))}
          </tbody>
        </table>
        {info.comment && (
          <Section title="Comentário" className="mt-3">
            <pre className="max-h-40 overflow-auto font-mono text-[12px] whitespace-pre-wrap select-text">{info.comment}</pre>
          </Section>
        )}
      </div>
    </Modal>
  );
}

function EstimateModal({ sources, onClose, onPick }: { sources: string[]; onClose: () => void; onPick: (e: Estimate) => void }) {
  const [rows, setRows] = useState<Estimate[] | null>(null);
  const [err, setErr] = useState<string | null>(null);
  useEffect(() => {
    void api.estimateFormats(sources).then(setRows).catch((e) => setErr((e as { message: string }).message));
  }, [sources]);
  return (
    <Modal title="Analisar formatos" width={600} onClose={onClose}>
      <div className="px-4 pb-4">
        <p className="text-[var(--muted)]">Amostra de {sources.length === 1 ? f.baseName(sources[0]) : `${sources.length} itens`} compactada em cada formato; valores estimados para o total.</p>
        {err && <p className="mt-3 text-[var(--danger)]">{err}</p>}
        {!rows && !err && <div className="mt-6 flex items-center justify-center gap-2 text-[var(--muted)]"><RefreshCw size={15} className="animate-spin" /> Analisando...</div>}
        {rows && <EstimateTable rows={rows} onPick={onPick} />}
      </div>
    </Modal>
  );
}

function ConvertModal({ archive, password, onClose, onStart }: { archive: string; password: string | null; onClose: () => void; onStart: (s: JobSpec) => void }) {
  const [format, setFormat] = useState<Format>("7z");
  const [level, setLevel] = useState(5);
  const out = f.withExt(archive, format);
  return (
    <Modal title="Converter arquivo" width={520} onClose={onClose}>
      <div className="grid grid-cols-2 gap-3 px-4 pb-4">
        <div>
          <Label>Novo formato</Label>
          <select className="field w-full" value={format} onChange={(e) => setFormat(e.target.value as Format)}>
            {f.FORMATS.filter((x) => !x.single).map((x) => <option key={x.id} value={x.id}>{x.label} — {x.hint}</option>)}
          </select>
        </div>
        <div>
          <Label>Nível</Label>
          <select className="field w-full" value={level} onChange={(e) => setLevel(Number(e.target.value))}>
            {f.LEVELS.map((l) => <option key={l.value} value={l.value}>{l.label}</option>)}
          </select>
        </div>
        <p className="col-span-2 truncate text-[12px] text-[var(--faint)]">Destino: {out}</p>
      </div>
      <Footer>
        <button className="btn btn-primary min-w-24" disabled={out === archive} onClick={() => onStart({ kind: "convert", archive, out, password, options: { ...defaultCreate(format), level } })}>Converter</button>
        <button className="btn min-w-24" onClick={onClose}>Cancelar</button>
      </Footer>
    </Modal>
  );
}

function PromptModal({ title, initial, onOk, onClose, password, icon }: { title: string; initial: string; onOk: (v: string) => void; onClose: () => void; password?: boolean; icon?: ReactNode }) {
  const [v, setV] = useState(initial);
  return (
    <Modal title={<span className="inline-flex items-center gap-2">{icon}{title}</span>} width={420} onClose={onClose}>
      <form className="px-4 pb-4" onSubmit={(e) => { e.preventDefault(); if (v) { onClose(); onOk(v); } }}>
        <input className="field w-full" type={password ? "password" : "text"} autoFocus value={v} onChange={(e) => setV(e.target.value)} />
        <div className="mt-3 flex justify-end gap-2">
          <button className="btn btn-primary min-w-24" type="submit" disabled={!v}>OK</button>
          <button className="btn min-w-24" type="button" onClick={onClose}>Cancelar</button>
        </div>
      </form>
    </Modal>
  );
}

