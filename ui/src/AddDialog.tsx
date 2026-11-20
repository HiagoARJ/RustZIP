import { save } from "@tauri-apps/plugin-dialog";
import { BarChart3, FolderOpen, KeyRound, Loader2, Mail } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { api, defaultCreate, isTauri, type CreateOptions, type Estimate, type Format, type JobSpec, type ZipMethod } from "./api";
import * as f from "./fmt";
import { Check, Footer, Label, Section, Tabs } from "./ui";

type Tab = "geral" | "avancado" | "arquivos" | "comentario";

const VOLUME_PRESETS: { label: string; bytes: number }[] = [
  { label: "E-mail (25 MB)", bytes: 25 * 1024 ** 2 },
  { label: "CD (700 MB)", bytes: 700 * 1024 ** 2 },
  { label: "FAT32 (4 GB)", bytes: 4095 * 1024 ** 2 },
  { label: "DVD (4,7 GB)", bytes: 4_700_000_000 },
];

const MAIN: Format[] = ["7z", "zip", "tar.zst", "tar.xz"];

const ZIP_METHODS: { id: ZipMethod; label: string }[] = [
  { id: "deflate", label: "Deflate (compatível)" },
  { id: "zopfli", label: "Deflate Zopfli (menor, lento, compatível)" },
  { id: "zstd", label: "Zstandard (rápido, ZIP moderno)" },
  { id: "xz", label: "XZ (compacto, ZIP moderno)" },
  { id: "bzip2", label: "BZip2" },
];

export function AddForm({ sources, suggested, email: emailInit = false, onCancel, onStart }: {
  sources: string[];
  suggested: string;
  email?: boolean;
  onCancel: () => void;
  onStart: (spec: JobSpec) => void;
}) {
  const [tab, setTab] = useState<Tab>("geral");
  const [out, setOut] = useState(suggested);
  const [o, setO] = useState<CreateOptions>(() => defaultCreate((suggested.match(/\.(tar\.\w+|\w+)$/)?.[1] as Format) ?? "7z"));
  const [email, setEmail] = useState(emailInit);
  const [usePw, setUsePw] = useState(false);
  const [pw, setPw] = useState("");
  const [pw2, setPw2] = useState("");
  const [showPw, setShowPw] = useState(false);
  const [volNum, setVolNum] = useState("");
  const [volUnit, setVolUnit] = useState(1024 ** 2);
  const [exclude, setExclude] = useState("");
  const [estimates, setEstimates] = useState<Estimate[] | null>(null);
  const [estimating, setEstimating] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void api.settingsGet().then((s) => {
      const format = (suggested.match(/\.(tar\.\w+|\w+)$/)?.[1] as Format) ?? s.defaultFormat;
      setO((x) => ({ ...x, format, level: s.defaultLevel, zipMethod: s.zipMethod, solid: s.solid, smart: s.smart }));
    });
  }, [suggested]);

  const single = sources.length === 1 && !!f.ext(f.baseName(sources[0]));
  const formats = f.FORMATS.filter((x) => !x.single || single);
  const set = <K extends keyof CreateOptions>(k: K, v: CreateOptions[K]) => setO((x) => ({ ...x, [k]: v }));
  const setFormat = (format: Format) => {
    set("format", format);
    setOut((p) => f.withExt(p, format));
  };
  const canPw = o.format === "7z" || o.format === "zip";
  const pwMismatch = usePw && canPw && pw !== pw2;

  const volumeBytes = useMemo(() => {
    const n = parseFloat(volNum.replace(",", "."));
    return Number.isFinite(n) && n > 0 ? Math.round(n * volUnit) : 0;
  }, [volNum, volUnit]);

  async function browse() {
    if (!isTauri) return;
    const p = await save({ defaultPath: out, filters: [{ name: o.format.toUpperCase(), extensions: [o.format.split(".").pop()!] }] });
    if (p) setOut(p);
  }

  async function analyze() {
    setEstimating(true);
    setError(null);
    try {
      setEstimates(await api.estimateFormats(sources));
    } catch (e) {
      setError((e as { message: string }).message);
    } finally {
      setEstimating(false);
    }
  }

  function start() {
    if (pwMismatch) return setError("As senhas não conferem.");
    const options: CreateOptions = {
      ...o,
      password: usePw && canPw && pw ? pw : null,
      encryptNames: usePw && o.format === "7z" && o.encryptNames,
      volumeSize: volumeBytes,
      exclude: exclude.split(/\r?\n/).map((s) => s.trim()).filter(Boolean),
      comment: o.format === "zip" && o.comment ? o.comment : null,
    };
    onStart({ kind: "create", sources, out, options, email });
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <Tabs
        value={tab}
        onChange={setTab}
        items={[
          { id: "geral", label: "Geral" },
          { id: "avancado", label: "Avançado" },
          { id: "arquivos", label: "Arquivos" },
          ...(o.format === "zip" ? [{ id: "comentario" as Tab, label: "Comentário" }] : []),
        ]}
      />
      <div className="min-h-0 flex-1 overflow-auto p-4">
        {tab === "geral" && (
          <div className="flex flex-col gap-3">
            <div>
              <Label>Nome do arquivo</Label>
              <div className="flex gap-2">
                <input className="field flex-1" value={out} onChange={(e) => setOut(e.target.value)} spellCheck={false} />
                <button className="btn" onClick={browse}>
                  <FolderOpen size={15} /> Procurar...
                </button>
              </div>
              <div className="mt-1 truncate text-[12px] text-[var(--faint)]">
                {sources.length === 1 ? f.baseName(sources[0]) : `${sources.length} itens selecionados`}
              </div>
            </div>
            <div className="grid grid-cols-[1fr_1.1fr] gap-3">
              <Section title="Formato">
                <div className="flex flex-col gap-0.5">
                  {formats.filter((x) => MAIN.includes(x.id)).map((x) => (
                    <label key={x.id} className={`flex items-center gap-2.5 rounded px-1.5 py-1 ${o.format === x.id ? "bg-[var(--accent-soft)]" : "hover:bg-[var(--hover)]"}`}>
                      <input type="radio" className="radio" checked={o.format === x.id} onChange={() => setFormat(x.id)} />
                      <span className={`w-16 ${o.format === x.id ? "font-semibold" : ""}`}>{x.label}</span>
                      <span className="truncate text-[12px] text-[var(--faint)]">{x.hint}</span>
                    </label>
                  ))}
                </div>
                <select
                  className={`field mt-2 w-full ${!MAIN.includes(o.format) ? "!border-[var(--accent)]" : ""}`}
                  value={MAIN.includes(o.format) ? "" : o.format}
                  onChange={(e) => e.target.value && setFormat(e.target.value as Format)}
                >
                  <option value="">Outros formatos...</option>
                  {formats.filter((x) => !MAIN.includes(x.id)).map((x) => (
                    <option key={x.id} value={x.id}>{x.label} — {x.hint}</option>
                  ))}
                </select>
              </Section>
              <div className="flex flex-col gap-3">
                <Section title="Compactação">
                  <Label>Nível</Label>
                  <select className="field w-full" value={o.level} onChange={(e) => set("level", Number(e.target.value))}>
                    {f.LEVELS.map((l) => (
                      <option key={l.value} value={l.value}>{l.label}</option>
                    ))}
                  </select>
                  {o.format === "zip" && (
                    <div className="mt-2">
                      <Label>Método</Label>
                      <select className="field w-full" value={o.zipMethod} onChange={(e) => set("zipMethod", e.target.value as ZipMethod)}>
                        {ZIP_METHODS.map((m) => (
                          <option key={m.id} value={m.id}>{m.label}</option>
                        ))}
                      </select>
                    </div>
                  )}
                </Section>
                <Section title="Dividir em volumes">
                  <div className="flex gap-2">
                    <input className="field w-0 flex-1" placeholder="Sem divisão" value={volNum} onChange={(e) => setVolNum(e.target.value)} />
                    <select className="field" value={volUnit} onChange={(e) => setVolUnit(Number(e.target.value))}>
                      <option value={1024 ** 2}>MB</option>
                      <option value={1024 ** 3}>GB</option>
                    </select>
                  </div>
                  <select className="field mt-2 w-full text-[12px]" value="" onChange={(e) => {
                    const b = Number(e.target.value);
                    if (b) { setVolUnit(1024 ** 2); setVolNum(String(Math.round(b / 1024 ** 2))); }
                  }}>
                    <option value="">Tamanhos comuns...</option>
                    {VOLUME_PRESETS.map((v) => (
                      <option key={v.label} value={v.bytes}>{v.label}</option>
                    ))}
                  </select>
                </Section>
              </div>
            </div>
            <Section title="Opções">
              <div className="grid grid-cols-2 gap-x-4">
                <Check checked={o.solid} disabled={o.format !== "7z"} onChange={(v) => set("solid", v)} label="Arquivo sólido" hint="Arquivos parecidos juntos: menor" />
                <Check checked={o.smart} onChange={(v) => set("smart", v)} label="Compactação inteligente" hint="Pula fotos/vídeos já compactados; BCJ em .exe" />
                <Check checked={o.testAfter} onChange={(v) => set("testAfter", v)} label="Testar depois de compactar" />
                <Check checked={o.deleteSources} onChange={(v) => set("deleteSources", v)} label="Mandar originais para a Lixeira" />
                <Check checked={email} onChange={setEmail} label={<span className="inline-flex items-center gap-1.5"><Mail size={14} /> Enviar por e-mail</span>} />
                <Check checked={usePw} disabled={!canPw} onChange={setUsePw} label={<span className="inline-flex items-center gap-1.5"><KeyRound size={14} /> Proteger com senha</span>} hint={canPw ? "AES-256" : "Só em 7Z e ZIP"} />
              </div>
              {usePw && canPw && (
                <div className="rise mt-2 grid grid-cols-2 gap-2 rounded-md border border-[var(--line)] bg-[var(--panel-2)] p-3">
                  <input className="field" type={showPw ? "text" : "password"} placeholder="Senha" value={pw} onChange={(e) => setPw(e.target.value)} autoFocus />
                  <input className={`field ${pwMismatch && pw2 ? "!border-[var(--danger)]" : ""}`} type={showPw ? "text" : "password"} placeholder="Confirmar senha" value={pw2} onChange={(e) => setPw2(e.target.value)} />
                  <Check checked={showPw} onChange={setShowPw} label="Mostrar senha" />
                  <Check checked={o.encryptNames} disabled={o.format !== "7z"} onChange={(v) => set("encryptNames", v)} label="Criptografar nomes dos arquivos" />
                </div>
              )}
            </Section>
          </div>
        )}
        {tab === "avancado" && (
          <div className="flex flex-col gap-3">
            <div className="grid grid-cols-2 gap-3">
              <Section title="Dicionário / janela">
                <select className="field w-full" value={o.dictionaryMb} onChange={(e) => set("dictionaryMb", Number(e.target.value))}>
                  <option value={0}>Automático (pelo nível)</option>
                  {[1, 4, 8, 16, 32, 64, 128, 256, 512].map((m) => (
                    <option key={m} value={m}>{m} MB</option>
                  ))}
                </select>
                <p className="mt-2 text-[12px] text-[var(--faint)]">Maior encontra repetições mais distantes; usa mais memória (≈11× por thread no 7Z).</p>
              </Section>
              <Section title="Threads">
                <select className="field w-full" value={o.threads} onChange={(e) => set("threads", Number(e.target.value))}>
                  <option value={0}>Automático (todos os núcleos)</option>
                  {[1, 2, 4, 6, 8, 12, 16, 24, 32].map((n) => (
                    <option key={n} value={n}>{n}</option>
                  ))}
                </select>
                <p className="mt-2 text-[12px] text-[var(--faint)]">ZIP e 7Z compactam blocos em paralelo; TAR.ZST/XZ usam o modo multithread do compressor.</p>
              </Section>
            </div>
            <Section title="Analisar formatos">
              <div className="flex items-center justify-between gap-3">
                <p className="text-[12px] text-[var(--muted)]">Compacta uma amostra em vários formatos e estima tamanho e tempo do total.</p>
                <button className="btn" onClick={analyze} disabled={estimating}>
                  {estimating ? <Loader2 size={15} className="animate-spin" /> : <BarChart3 size={15} />} Analisar
                </button>
              </div>
              {estimates && <EstimateTable rows={estimates} onPick={(e) => { setFormat(e.format); set("level", e.level); setTab("geral"); }} />}
            </Section>
          </div>
        )}
        {tab === "arquivos" && (
          <Section title="Não incluir">
            <textarea
              className="field h-40 w-full resize-none font-mono text-[12px]"
              placeholder={"*.tmp\nnode_modules\n.git\ncache/*.bin"}
              value={exclude}
              onChange={(e) => setExclude(e.target.value)}
              spellCheck={false}
            />
            <p className="mt-2 text-[12px] text-[var(--faint)]">Um padrão por linha. <code>*</code> e <code>?</code> como curingas; sem barra compara só o nome.</p>
          </Section>
        )}
        {tab === "comentario" && (
          <Section title="Comentário do arquivo ZIP">
            <textarea className="field h-40 w-full resize-none" value={o.comment ?? ""} onChange={(e) => set("comment", e.target.value)} />
          </Section>
        )}
        {error && <div className="mt-3 rounded-md border border-[var(--danger)] px-3 py-2 text-[var(--danger)]">{error}</div>}
      </div>
      <Footer>
        <button className="btn btn-primary min-w-24" onClick={start} disabled={!out.trim() || pwMismatch}>OK</button>
        <button className="btn min-w-24" onClick={onCancel}>Cancelar</button>
      </Footer>
    </div>
  );
}

export function EstimateTable({ rows, onPick }: { rows: Estimate[]; onPick?: (e: Estimate) => void }) {
  const best = Math.min(...rows.map((r) => r.estimatedBytes));
  const fastest = Math.min(...rows.map((r) => r.estimatedMs));
  return (
    <table className="mt-3 w-full text-[12px]">
      <thead className="text-left text-[var(--faint)]">
        <tr>
          <th className="py-1 font-normal">Formato</th>
          <th className="font-normal">Tamanho estimado</th>
          <th className="font-normal">Taxa</th>
          <th className="font-normal">Tempo</th>
          <th />
        </tr>
      </thead>
      <tbody>
        {rows.map((r) => (
          <tr key={r.label} className="border-t border-[var(--line)]">
            <td className="py-1.5">{r.label}</td>
            <td className={r.estimatedBytes === best ? "font-semibold text-[var(--ok)]" : ""}>{f.size(r.estimatedBytes)}</td>
            <td>{f.percent(r.ratio)}</td>
            <td className={r.estimatedMs === fastest ? "font-semibold text-[var(--ok)]" : ""}>{r.estimatedMs < 1000 ? "< 1 s" : f.duration(r.estimatedMs)}</td>
            <td className="text-right">{onPick && <button className="btn btn-ghost !min-h-6 !px-2 text-[var(--accent)]" onClick={() => onPick(r)}>Usar</button>}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
