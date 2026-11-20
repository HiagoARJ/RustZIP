import { LogicalSize } from "@tauri-apps/api/dpi";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { StrictMode, useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { AddForm } from "./AddDialog";
import { api, isTauri, type JobSpec, type Task } from "./api";
import { ExtractForm } from "./ExtractDialog";
import { RunJobs } from "./JobView";
import { Manager } from "./Manager";
import "./styles.css";
import { applyTheme, normalizeTheme, ThemeScene } from "./theme";
import { onEvent } from "./api";

function close() {
  if (isTauri) void getCurrentWindow().close();
}

/** Janela aberta pelo menu do Explorer: primeiro o diálogo, depois o progresso. */
function DialogThenRun({ title, children }: { title: string; children: (start: (s: JobSpec) => void) => React.ReactNode }) {
  const [spec, setSpec] = useState<JobSpec | null>(null);
  useEffect(() => {
    if (spec && isTauri) {
      // Do diálogo para o progresso: a janela encolhe para o tamanho do cartão.
      void getCurrentWindow().setSize(new LogicalSize(560, 300)).catch(() => undefined);
    }
  }, [spec]);
  if (spec) return <RunJobs title={title} jobs={[spec]} />;
  // Nos temas, o diálogo fica num painel sobre o cenário (texto nunca direto sobre o fundo).
  return (
    <div className="app-shell flex h-full flex-col">
      <div className="app-pane glass flex min-h-0 flex-1 flex-col bg-[var(--bg)]">{children(setSpec)}</div>
    </div>
  );
}

function App() {
  const [task, setTask] = useState<Task | null>(null);
  useEffect(() => {
    // O tema vale para todas as janelas: aplicado ao abrir e quando as Opções mudam.
    const lab = new URLSearchParams(location.search).get("theme");
    void api.settingsGet().then((s) => applyTheme(normalizeTheme(lab ?? s.theme)));
    return onEvent<string>("theme-changed", (id) => applyTheme(normalizeTheme(id)));
  }, []);
  useEffect(() => {
    void api.taskGet().then((t) => {
      setTask(t ?? { mode: "browse", path: "C:\\" });
      if (isTauri) requestAnimationFrame(() => void getCurrentWindow().show());
    });
  }, []);
  if (!task) return <ThemeScene />;
  return (
    <>
      <ThemeScene />
      <View task={task} />
    </>
  );
}

function View({ task }: { task: Task }) {
  switch (task.mode) {
    case "browse":
      return <Manager initial={task.path} />;
    case "add":
      return (
        <DialogThenRun title="Compactando">
          {(start) => <AddForm sources={task.sources} suggested={task.suggested} email={task.email} onCancel={close} onStart={start} />}
        </DialogThenRun>
      );
    case "extract":
      return (
        <DialogThenRun title="Extraindo">
          {(start) => <ExtractForm archive={task.archive} dest={task.dest} onCancel={close} onStart={start} />}
        </DialogThenRun>
      );
    case "run":
      return <RunJobs title={task.title} jobs={task.jobs} />;
  }
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
