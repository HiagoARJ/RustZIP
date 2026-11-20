import { Loader2, MousePointerClick, Palette, ShieldCheck } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { emit } from "@tauri-apps/api/event";
import { api, isTauri, type Format, type Integration, type Settings } from "./api";
import { applyTheme, normalizeTheme, THEMES, type ThemeId } from "./theme";
import * as f from "./fmt";
import { Check, Footer, Label, Section } from "./ui";

export function SettingsForm({ onClose }: { onClose: () => void }) {
  const [s, setS] = useState<Settings | null>(null);
  const [integ, setInteg] = useState<Integration | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [msg, setMsg] = useState<string | null>(null);

  // O tema que está na tela ao abrir: é o que volta se fechar sem salvar.
  const [original] = useState<ThemeId>(() => normalizeTheme(document.documentElement.dataset.theme));
  const saved = useRef(false);
  const originalRef = useRef<ThemeId>(original);
  originalRef.current = original;
  // Fechou sem salvar (Esc, X, clique fora): desfaz a prévia do tema.
  useEffect(() => () => {
    if (!saved.current) applyTheme(originalRef.current);
  }, []);
  useEffect(() => {
    void api.settingsGet().then(setS);
    void api.integrationStatus().then(setInteg);
  }, []);

  if (!s) return <div className="p-6 text-[var(--muted)]">Carregando...</div>;
  const set = <K extends keyof Settings>(k: K, v: Settings[K]) => setS({ ...s, [k]: v });

  async function toggle(kind: "classic" | "modern", on: boolean) {
    setBusy(kind);
    setMsg(null);
    try {
      setInteg(await api.integrationSet(kind === "classic" ? on : undefined, kind === "modern" ? on : undefined));
    } catch (e) {
      setMsg((e as { message: string }).message);
    } finally {
      setBusy(null);
    }
  }

  function pickTheme(id: ThemeId) {
    setS({ ...s!, theme: id });
    applyTheme(id);
  }

  function cancel() {
    applyTheme(original);
    onClose();
  }

  async function saveAll() {
    await api.settingsSet(s!);
    saved.current = true;
    // As outras janelas abertas trocam de tema na hora.
    if (isTauri) void emit("theme-changed", s!.theme);
    onClose();
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-auto p-4">
        <Section title={<span className="inline-flex items-center gap-1.5"><Palette size={13} /> Tema</span>}>
          <div className="grid grid-cols-3 gap-2">
            {THEMES.map((t) => {
              const on = normalizeTheme(s.theme) === t.id;
              return (
                <button
                  key={t.id}
                  onClick={() => pickTheme(t.id)}
                  className={`card overflow-hidden border text-left transition ${on ? "border-[var(--accent)] ring-2 ring-[var(--accent)]" : "border-[var(--line)] hover:border-[var(--line-strong)]"}`}
                  aria-pressed={on}
                >
                  <div className="relative h-14" style={{ background: t.preview.bg }}>
                    <div className="absolute inset-x-3 bottom-2 top-3 flex items-end gap-1.5 rounded-md p-1.5" style={{ background: t.preview.panel, boxShadow: "0 4px 12px rgba(0,0,0,.15)" }}>
                      <span className="h-1.5 flex-1 rounded-full opacity-60" style={{ background: t.preview.text }} />
                      <span className="h-3 w-7 rounded-full" style={{ background: t.preview.accent }} />
                    </div>
                  </div>
                  <div className="bg-[var(--panel-2)] px-2.5 py-1.5">
                    <div className="font-semibold">{t.name}{t.id === "padrao" && <span className="ml-1 text-[11px] font-normal text-[var(--faint)]">(padrão)</span>}</div>
                    <div className="truncate text-[11px] text-[var(--faint)]" title={t.hint}>{t.hint}</div>
                  </div>
                </button>
              );
            })}
          </div>
        </Section>
        <Section title={<span className="inline-flex items-center gap-1.5"><MousePointerClick size={13} /> Menu de contexto do Explorer</span>}>
          <div className="grid grid-cols-2 gap-3">
            <IntegrationRow
              title="Menu do Windows 11"
              hint="Submenu RustZIP no menu principal (pacote com identidade; pede administrador uma vez para confiar no certificado)."
              on={integ?.modern ?? false}
              busy={busy === "modern"}
              onToggle={(v) => toggle("modern", v)}
            />
            <IntegrationRow
              title='Menu clássico ("Mostrar mais opções")'
              hint='Submenu e "Abrir com RustZIP" para 7z, zip, rar, tar... Com o menu do Windows 11 ligado, ele já aparece aqui também.'
              on={integ?.classic ?? false}
              busy={busy === "classic"}
              onToggle={(v) => toggle("classic", v)}
            />
          </div>
          {msg && <p className="mt-2 text-[12px] text-[var(--danger)]">{msg}</p>}
          {integ && !integ.dllPresent && <p className="mt-2 text-[12px] text-[var(--warn)]">rustzip_shell.dll não está em {integ.installDir}.</p>}
          <div className="mt-3 grid grid-cols-2 gap-x-4 border-t border-[var(--line)] pt-2">
            <Check checked={s.menuAddDialog} onChange={(v) => set("menuAddDialog", v)} label="Adicionar para o arquivo..." />
            <Check checked={s.menuAddQuick} onChange={(v) => set("menuAddQuick", v)} label='Adicionar para "nome.ext"' />
            <Check checked={s.menuEmail} onChange={(v) => set("menuEmail", v)} label="Comprimir e enviar por e-mail" />
            <Check checked={s.menuExtract} onChange={(v) => set("menuExtract", v)} label="Extrair (arquivos, aqui, para pasta)" />
            <Check checked={s.menuTest} onChange={(v) => set("menuTest", v)} label="Testar arquivo" />
          </div>
        </Section>
        <div className="grid grid-cols-2 gap-3">
          <Section title='Padrão de "Adicionar para"'>
            <Label>Formato</Label>
            <select className="field w-full" value={s.defaultFormat} onChange={(e) => set("defaultFormat", e.target.value as Format)}>
              {f.FORMATS.filter((x) => !x.single).map((x) => (
                <option key={x.id} value={x.id}>{x.label} — {x.hint}</option>
              ))}
            </select>
            <div className="mt-2">
              <Label>Nível</Label>
              <select className="field w-full" value={s.defaultLevel} onChange={(e) => set("defaultLevel", Number(e.target.value))}>
                {f.LEVELS.map((l) => (
                  <option key={l.value} value={l.value}>{l.label}</option>
                ))}
              </select>
            </div>
            <div className="mt-1">
              <Check checked={s.smart} onChange={(v) => set("smart", v)} label="Compactação inteligente" />
              <Check checked={s.solid} onChange={(v) => set("solid", v)} label="Arquivo sólido (7Z)" />
            </div>
          </Section>
          <Section title="Extração">
            <Check
              checked={s.smartExtractHere}
              onChange={(v) => set("smartExtractHere", v)}
              label='"Extrair aqui" sem bagunça'
              hint="Cria uma pasta quando o arquivo tem vários itens soltos"
            />
            <Check checked={s.openFolderAfterExtract} onChange={(v) => set("openFolderAfterExtract", v)} label="Abrir a pasta ao terminar" />
            <div className="mt-3 flex items-start gap-2 rounded-md bg-[var(--accent-soft)] p-2 text-[12px]">
              <ShieldCheck size={15} className="mt-px flex-none text-[var(--accent)]" />
              <span>Caminhos perigosos ("../", unidades, nomes reservados do Windows) são bloqueados na extração.</span>
            </div>
          </Section>
        </div>
      </div>
      <Footer>
        <button className="btn btn-primary min-w-24" onClick={saveAll}>Salvar</button>
        <button className="btn min-w-24" onClick={cancel}>Cancelar</button>
      </Footer>
    </div>
  );
}

function IntegrationRow({ title, hint, on, busy, onToggle }: { title: string; hint: string; on: boolean; busy: boolean; onToggle: (v: boolean) => void }) {
  return (
    <div className="rounded-md border border-[var(--line)] bg-[var(--panel-2)] p-3">
      <div className="flex items-center justify-between gap-2">
        <span className="font-semibold">{title}</span>
        <button
          className={`relative h-5 w-10 flex-none rounded-full border transition-colors ${on ? "border-[var(--accent)] bg-[var(--accent)]" : "border-[var(--line-strong)] bg-transparent"}`}
          onClick={() => onToggle(!on)}
          disabled={busy}
          aria-pressed={on}
        >
          {busy ? (
            <Loader2 size={12} className="absolute top-[3px] left-[13px] animate-spin" />
          ) : (
            <span className={`absolute top-[3px] h-3 w-3 rounded-full transition-all ${on ? "left-[23px] bg-[var(--accent-text)]" : "left-[4px] bg-[var(--muted)]"}`} />
          )}
        </button>
      </div>
      <p className="mt-1 text-[12px] text-[var(--faint)]">{hint}</p>
    </div>
  );
}
