import { getCurrentWindow } from "@tauri-apps/api/window";
import { CheckCircle2, FolderOpen, KeyRound, Pause, Play, Square, TriangleAlert, XCircle } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { api, isTauri, onEvent, type DoneEvent, type JobSpec, type Snapshot } from "./api";
import * as f from "./fmt";

export type JobState = "running" | "ok" | "error" | "cancelled";

function titleOf(spec: JobSpec): string {
  switch (spec.kind) {
    case "create":
      return `Compactando para ${f.baseName(spec.out)}`;
    case "extract":
      return `Extraindo ${f.baseName(spec.archive)}`;
    case "test":
      return `Testando ${f.baseName(spec.archive)}`;
    case "modify":
      return `Atualizando ${f.baseName(spec.archive)}`;
    case "convert":
      return `Convertendo para ${f.baseName(spec.out)}`;
  }
}

function withPassword(spec: JobSpec, pw: string): JobSpec {
  switch (spec.kind) {
    case "extract":
      return { ...spec, options: { ...spec.options, password: pw } };
    case "test":
    case "convert":
      return { ...spec, password: pw };
    case "modify":
      return { ...spec, options: { ...spec.options, password: pw } };
    default:
      return spec;
  }
}

/** Uma operação: inicia ao montar, mostra o progresso e o resultado. */
export function JobCard({ spec: initial, onState, onProgress }: {
  spec: JobSpec;
  onState?: (s: JobState, done?: DoneEvent) => void;
  onProgress?: (fraction: number) => void;
}) {
  const [spec, setSpec] = useState(initial);
  const [id, setId] = useState<number | null>(null);
  const [snap, setSnap] = useState<Snapshot | null>(null);
  const [done, setDone] = useState<DoneEvent | null>(null);
  const [askPw, setAskPw] = useState<null | "required" | "wrong">(null);
  const [pw, setPw] = useState("");
  const idRef = useRef<number | null>(null);
  const cbs = useRef({ onState, onProgress });
  cbs.current = { onState, onProgress };

  useEffect(() => {
    let alive = true;
    setDone(null);
    setSnap(null);
    const offP = onEvent<{ id: number; snap: Snapshot }>("job-progress", (e) => {
      if (e.id !== idRef.current) return;
      setSnap(e.snap);
      cbs.current.onProgress?.(e.snap.total ? e.snap.done / e.snap.total : 0);
    });
    const offD = onEvent<DoneEvent>("job-done", (e) => {
      if (e.id !== idRef.current) return;
      setSnap(e.snap);
      if (!e.ok && (e.code === "password_required" || e.code === "wrong_password")) {
        setAskPw(e.code === "password_required" ? "required" : "wrong");
        return;
      }
      setDone(e);
      cbs.current.onState?.(e.ok ? "ok" : e.code === "cancelled" ? "cancelled" : "error", e);
    });
    void api.jobStart(spec).then((jid) => {
      if (!alive) return void api.jobCancel(jid);
      idRef.current = jid;
      setId(jid);
    });
    return () => {
      alive = false;
      offP();
      offD();
    };
  }, [spec]);

  const paused = snap?.paused ?? false;
  const frac = snap && snap.total > 0 ? Math.min(1, snap.done / snap.total) : 0;
  const elapsed = snap?.elapsedMs ?? 0;
  const rate = elapsed > 300 && snap ? snap.done / (elapsed / 1000) : 0;
  const remaining = rate > 0 && snap ? ((snap.total - snap.done) / rate) * 1000 : null;
  const ratio = spec.kind === "create" && snap && snap.done > 0 ? snap.packed / snap.done : null;
  const result = done?.result as Record<string, unknown> | null;
  const target = spec.kind === "create" || spec.kind === "convert" ? (result?.outputs as string[] | undefined)?.[0] ?? spec.out : spec.kind === "extract" ? (result?.dest as string) ?? spec.dest : null;

  if (askPw) {
    return (
      <div className="rise card glass border border-[var(--line)] bg-[var(--panel)] p-4">
        <div className="mb-2 flex items-center gap-2 font-semibold">
          <KeyRound size={16} className="text-[var(--accent)]" /> {askPw === "wrong" ? "Senha incorreta" : "Digite a senha"}
        </div>
        <p className="mb-3 truncate text-[var(--muted)]">{titleOf(spec).replace(/^\w+ /, "")}</p>
        <form
          className="flex gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            setAskPw(null);
            setSpec(withPassword(spec, pw));
          }}
        >
          <input className="field flex-1" type="password" autoFocus value={pw} onChange={(e) => setPw(e.target.value)} />
          <button className="btn btn-primary" type="submit" disabled={!pw}>OK</button>
          <button className="btn" type="button" onClick={() => { setAskPw(null); setDone({ id: id ?? 0, ok: false, result: null, code: "cancelled", message: "Operação cancelada", snap: snap! }); cbs.current.onState?.("cancelled"); }}>
            Cancelar
          </button>
        </form>
      </div>
    );
  }

  return (
    <div className="card glass border border-[var(--line)] bg-[var(--panel)] p-4">
      <div className="flex items-start gap-3">
        <div className="mt-0.5">
          {!done && <img src="/rustzip.avif" alt="" className={`h-8 w-8 ${paused ? "opacity-50" : ""}`} />}
          {done?.ok && <CheckCircle2 size={30} className="text-[var(--ok)]" />}
          {done && !done.ok && (done.code === "cancelled" ? <XCircle size={30} className="text-[var(--faint)]" /> : <TriangleAlert size={30} className="text-[var(--danger)]" />)}
        </div>
        <div className="min-w-0 flex-1">
          <div className="truncate font-semibold">{titleOf(spec)}</div>
          <div className="truncate text-[12px] text-[var(--faint)]" title={snap?.current}>
            {done ? (done.ok ? summary(spec, result, snap) : done.message) : paused ? "Pausado" : snap?.current || "Preparando..."}
          </div>
        </div>
        <div className="text-right text-[20px] font-light tabular-nums">{done?.ok ? "100%" : f.percent(frac)}</div>
      </div>
      <div className={`bar mt-3 ${paused ? "paused" : ""}`}>
        <div style={{ width: `${(done?.ok ? 1 : frac) * 100}%` }} />
      </div>
      <div className="mt-3 grid grid-cols-4 gap-2 text-[12px]">
        <Stat label="Decorrido" value={f.duration(elapsed)} />
        <Stat label="Restante" value={done ? "—" : remaining != null ? f.duration(remaining) : "..."} />
        <Stat label="Velocidade" value={rate ? f.speed(rate) : "..."} />
        {ratio != null ? <Stat label="Taxa" value={f.percent(ratio)} /> : <Stat label="Arquivos" value={snap ? `${snap.filesDone} de ${snap.filesTotal}` : "..."} />}
      </div>
      {!!snap?.warnings.length && (
        <div className="mt-3 max-h-20 overflow-auto rounded border border-[var(--line)] p-2 text-[12px] text-[var(--warn)]">
          {snap.warnings.map((w, i) => <div key={i}>{w}</div>)}
        </div>
      )}
      <div className="mt-3 flex justify-end gap-2">
        {!done && (
          <>
            <button className="btn" disabled={id == null} onClick={() => id != null && api.jobPause(id, !paused)}>
              {paused ? <Play size={14} /> : <Pause size={14} />} {paused ? "Continuar" : "Pausar"}
            </button>
            <button className="btn" disabled={id == null} onClick={() => id != null && api.jobCancel(id)}>
              <Square size={13} /> Cancelar
            </button>
          </>
        )}
        {done?.ok && target && (
          <button className="btn" onClick={() => (spec.kind === "extract" ? api.openPath(target) : api.revealPath(target))}>
            <FolderOpen size={14} /> {spec.kind === "extract" ? "Abrir pasta" : "Mostrar no Explorer"}
          </button>
        )}
      </div>
    </div>
  );
}

function summary(spec: JobSpec, r: Record<string, unknown> | null, snap: Snapshot | null): string {
  if (!r) return "Concluído";
  // Número do relatório; sem ele, o que o progresso mediu (nunca "NaN").
  const num = (v: unknown, fallback = 0) => (typeof v === "number" && Number.isFinite(v) ? v : fallback);
  if (spec.kind === "create" || spec.kind === "convert" || spec.kind === "modify") {
    const i = num(r.inputBytes, snap?.total ?? 0);
    const o = num(r.outputBytes, snap?.packed ?? 0);
    const parts = (r.outputs as string[] | undefined)?.length ?? 1;
    return `${f.size(i)} → ${f.size(o)} (${i ? f.percent(o / i) : "—"})${parts > 1 ? ` em ${parts} volumes` : ""} · ${f.duration(num(r.elapsedMs, snap?.elapsedMs))}`;
  }
  const files = num(r.files, snap?.filesDone);
  const bytes = num(r.bytes, snap?.done);
  if (spec.kind === "test") return `Nenhum erro encontrado · ${files} arquivos, ${f.size(bytes)}`;
  return `${files} arquivos, ${f.size(bytes)} em ${f.duration(num(r.elapsedMs, snap?.elapsedMs))}`;
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <div className="text-[var(--faint)]">{label}</div>
      <div className="tabular-nums">{value}</div>
    </div>
  );
}

/** Janela de tarefa: uma ou mais operações; fecha sozinha quando tudo dá certo sem avisos. */
export function RunJobs({ title, jobs }: { title: string; jobs: JobSpec[] }) {
  const [states, setStates] = useState<JobState[]>(() => jobs.map(() => "running"));
  const [fracs, setFracs] = useState<number[]>(() => jobs.map(() => 0));
  const [warned, setWarned] = useState(false);

  useEffect(() => {
    if (!isTauri) return;
    const running = states.some((s) => s === "running");
    const avg = fracs.reduce((a, b) => a + b, 0) / Math.max(1, fracs.length);
    void getCurrentWindow().setTitle(running ? `${Math.round(avg * 100)}% ${title}` : title);
    if (!running && states.every((s) => s === "ok") && !warned) {
      const t = setTimeout(() => void getCurrentWindow().close(), 1400);
      return () => clearTimeout(t);
    }
  }, [states, fracs, title, warned]);

  return (
    <div className="flex h-full flex-col gap-3 overflow-auto p-4">
      {jobs.map((j, i) => (
        <JobCard
          key={i}
          spec={j}
          onProgress={(x) => setFracs((v) => v.map((y, k) => (k === i ? x : y)))}
          onState={(s, d) => {
            if (d?.snap.warnings.length) setWarned(true);
            setStates((v) => v.map((y, k) => (k === i ? s : y)));
          }}
        />
      ))}
      {states.every((s) => s !== "running") && (
        <div className="flex justify-end">
          <button className="btn min-w-24" onClick={() => (isTauri ? getCurrentWindow().close() : undefined)}>Fechar</button>
        </div>
      )}
    </div>
  );
}
