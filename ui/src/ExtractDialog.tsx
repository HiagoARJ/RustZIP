import { open } from "@tauri-apps/plugin-dialog";
import { FolderOpen } from "lucide-react";
import { useEffect, useState } from "react";
import { api, isTauri, type JobSpec, type Overwrite } from "./api";
import * as f from "./fmt";
import { Check, Footer, Label, Radio, Section } from "./ui";

export function ExtractForm({ archive, dest: destInit, selection = [], stripPrefix = null, password = null, onCancel, onStart }: {
  archive: string;
  dest: string;
  selection?: string[];
  stripPrefix?: string | null;
  password?: string | null;
  onCancel: () => void;
  onStart: (spec: JobSpec) => void;
}) {
  const [dest, setDest] = useState(destInit);
  const [overwrite, setOverwrite] = useState<Overwrite>("overwrite");
  const [openAfter, setOpenAfter] = useState(false);
  const [pw, setPw] = useState(password ?? "");
  const folder = f.dirName(archive);

  useEffect(() => {
    void api.settingsGet().then((s) => setOpenAfter(s.openFolderAfterExtract));
  }, []);

  async function browse() {
    if (!isTauri) return;
    const p = await open({ directory: true, defaultPath: dest });
    if (typeof p === "string") setDest(p);
  }

  function start() {
    onStart({
      kind: "extract",
      archive,
      dest,
      options: { overwrite, selection, stripPrefix, password: pw || null, threads: 0 },
      openAfter,
    });
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="min-h-0 flex-1 overflow-auto p-4">
        <Label>Pasta de destino</Label>
        <div className="flex gap-2">
          <input className="field flex-1" value={dest} onChange={(e) => setDest(e.target.value)} spellCheck={false} />
          <button className="btn" onClick={browse}>
            <FolderOpen size={15} /> Procurar...
          </button>
        </div>
        <div className="mt-2 flex flex-wrap gap-1.5 text-[12px]">
          <button className="btn !min-h-6 !px-2" onClick={() => setDest(folder)}>Pasta do arquivo</button>
          <button className="btn !min-h-6 !px-2" onClick={() => void api.defaultExtractDir(archive).then(setDest)}>
            Subpasta "{f.baseName(archive).replace(/(\.tar)?\.[^.]+$/, "")}"
          </button>
        </div>
        <div className="mt-3 grid grid-cols-2 gap-3">
          <Section title="Se o arquivo já existir">
            <Radio checked={overwrite === "overwrite"} onChange={() => setOverwrite("overwrite")} label="Substituir" />
            <Radio checked={overwrite === "newer"} onChange={() => setOverwrite("newer")} label="Só se o do arquivo for mais novo" />
            <Radio checked={overwrite === "rename"} onChange={() => setOverwrite("rename")} label='Manter os dois ("nome (2)")' />
            <Radio checked={overwrite === "skip"} onChange={() => setOverwrite("skip")} label="Pular" />
          </Section>
          <Section title="Opções">
            <Check checked={openAfter} onChange={setOpenAfter} label="Abrir a pasta ao terminar" />
            <div className="mt-2">
              <Label>Senha (se houver)</Label>
              <input className="field w-full" type="password" value={pw} onChange={(e) => setPw(e.target.value)} />
            </div>
            {selection.length > 0 && <p className="mt-2 text-[12px] text-[var(--faint)]">{selection.length} item(ns) selecionado(s)</p>}
          </Section>
        </div>
      </div>
      <Footer>
        <button className="btn btn-primary min-w-24" onClick={start} disabled={!dest.trim()}>OK</button>
        <button className="btn min-w-24" onClick={onCancel}>Cancelar</button>
      </Footer>
    </div>
  );
}
