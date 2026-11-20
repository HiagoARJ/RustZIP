// Temas: "Padrão" (Fluent do Windows 11, segue claro/escuro do sistema), sete temas do catálogo
// "Temas modernos — Rust + React" (01, 02, 03, 05, 10, 21, 22) e o Constelação do Attack Shark.
// As cores ficam em themes.css ([data-theme]); aqui ficam o catálogo e os cenários animados.

import "@fontsource/manrope/latin-400.css";
import "@fontsource/manrope/latin-600.css";
import "@fontsource/manrope/latin-800.css";
import "@fontsource/inter/latin-400.css";
import "@fontsource/inter/latin-600.css";
import "@fontsource/space-grotesk/latin-500.css";
import "@fontsource/space-grotesk/latin-700.css";
import "@fontsource/sora/latin-400.css";
import "@fontsource/sora/latin-600.css";
import "@fontsource/sora/latin-800.css";
import "@fontsource/instrument-sans/latin-400.css";
import "@fontsource/instrument-sans/latin-600.css";
import "@fontsource/instrument-sans/latin-700.css";
import "@fontsource/plus-jakarta-sans/latin-400.css";
import "@fontsource/plus-jakarta-sans/latin-600.css";
import "@fontsource/plus-jakarta-sans/latin-800.css";
import "@fontsource/unbounded/latin-500.css";
import "@fontsource/unbounded/latin-800.css";
import "@fontsource/shippori-mincho/latin-500.css";
import "@fontsource/shippori-mincho/latin-700.css";
import "@fontsource/zen-maru-gothic/latin-400.css";
import "@fontsource/zen-maru-gothic/latin-500.css";
import "@fontsource/zen-maru-gothic/latin-700.css";
import "./themes.css";

import { useEffect, useRef, useState } from "react";

export type ThemeId = "padrao" | "liquid" | "aurora" | "ferrugem" | "bento" | "holo" | "mesh" | "sakura" | "constelacao";

export interface ThemeInfo {
  id: ThemeId;
  name: string;
  hint: string;
  /** Mini prévia no seletor. */
  preview: { bg: string; panel: string; accent: string; text: string };
}

export const THEMES: ThemeInfo[] = [
  { id: "padrao", name: "Padrão", hint: "Windows 11 · claro/escuro do sistema", preview: { bg: "linear-gradient(135deg,#f3f3f3 50%,#202020 50%)", panel: "#fbfbfb", accent: "#005fb8", text: "#1b1b1b" } },
  { id: "liquid", name: "Liquid Glass", hint: "Vidro translúcido sobre cores em movimento", preview: { bg: "linear-gradient(135deg,#C9D6FF,#E2C9FF 45%,#FFD1DC)", panel: "rgba(255,255,255,.45)", accent: "#4B3BFF", text: "#1B1740" } },
  { id: "aurora", name: "Aurora", hint: "Escuro com faixas de luz boreal", preview: { bg: "radial-gradient(circle at 20% 20%,#7C5CFF,transparent 55%),radial-gradient(circle at 85% 60%,#22D3EE,transparent 50%),#070B1A", panel: "rgba(13,18,40,.62)", accent: "#67E8F9", text: "#EEF1FF" } },
  { id: "ferrugem", name: "Ferrugem", hint: "Brasa e metal oxidado — homenagem ao Rust", preview: { bg: "radial-gradient(circle at 85% 90%,#FF6A1A,transparent 55%),linear-gradient(160deg,#1A0B06,#47180A)", panel: "rgba(34,14,8,.72)", accent: "#FF8A2B", text: "#FFF1E6" } },
  { id: "bento", name: "Bento Claro", hint: "Blocos brancos arredondados, sombras macias", preview: { bg: "#F1F1EC", panel: "#FFFFFF", accent: "#2F6BFF", text: "#17171A" } },
  { id: "holo", name: "Holográfico", hint: "Disco iridescente atrás de painéis perolados", preview: { bg: "conic-gradient(#9AF0FF,#C9B6FF,#FFBDE6,#FFF1A6,#A8FFD8,#9AF0FF)", panel: "rgba(255,255,255,.86)", accent: "#5A3FE0", text: "#1A1530" } },
  { id: "mesh", name: "Mesh", hint: "Malha de gradiente viva com cartões brancos", preview: { bg: "radial-gradient(circle at 15% 15%,#5B2BFF,transparent 55%),radial-gradient(circle at 90% 10%,#C2185B,transparent 50%),radial-gradient(circle at 15% 95%,#FF6A3D,transparent 55%),radial-gradient(circle at 90% 95%,#00B8D9,transparent 55%),#3A16B8", panel: "#FFFFFF", accent: "#5B2BFF", text: "#14121F" } },
  { id: "sakura", name: "Sakura", hint: "Minimalismo japonês e pétalas caindo", preview: { bg: "linear-gradient(180deg,#FFF8F9,#FCE6EC)", panel: "rgba(255,255,255,.88)", accent: "#B5305F", text: "#3A2A30" } },
  { id: "constelacao", name: "Constelação", hint: "Partículas que seguem o cursor (Attack Shark)", preview: { bg: "radial-gradient(circle at 30% 30%,#1a1f52,#05060d 70%)", panel: "rgba(255,255,255,.14)", accent: "#6d7dff", text: "#eceeff" } },
];

export function normalizeTheme(id: string | null | undefined): ThemeId {
  return THEMES.some((t) => t.id === id) ? (id as ThemeId) : "padrao";
}

export function applyTheme(id: ThemeId) {
  const root = document.documentElement;
  if (id === "padrao") delete root.dataset.theme;
  else root.dataset.theme = id;
  window.dispatchEvent(new CustomEvent("rz-theme", { detail: id }));
}

function useCurrentTheme(): ThemeId {
  const [t, setT] = useState<ThemeId>(normalizeTheme(document.documentElement.dataset.theme));
  useEffect(() => {
    const h = (e: Event) => setT((e as CustomEvent<ThemeId>).detail);
    window.addEventListener("rz-theme", h);
    return () => window.removeEventListener("rz-theme", h);
  }, []);
  return t;
}

/** Cenário atrás de toda a interface (os painéis dos temas são translúcidos ou flutuam por cima). */
export function ThemeScene() {
  const theme = useCurrentTheme();
  if (theme === "padrao" || theme === "bento") return null;
  return (
    <div key={theme} className="scene" aria-hidden="true">
      {theme === "liquid" && (
        <>
          <i className="blob float" style={{ left: "-6%", top: "-12%", width: "42vw", height: "42vw", background: "#5B8CFF", opacity: 0.75 }} />
          <i className="blob float rev" style={{ right: "-4%", top: "18%", width: "38vw", height: "38vw", background: "#FF7AC3", opacity: 0.7 }} />
          <i className="blob float" style={{ left: "32%", bottom: "-22%", width: "40vw", height: "40vw", background: "#7AF0D0", opacity: 0.7 }} />
        </>
      )}
      {theme === "aurora" && (
        <>
          <i className="blob drift" style={{ left: "-12%", top: "-25%", width: "70vw", height: "40vw", background: "#7C5CFF", opacity: 0.55, filter: "blur(90px)" }} />
          <i className="blob drift rev" style={{ right: "-16%", top: "10%", width: "62vw", height: "36vw", background: "#22D3EE", opacity: 0.4, filter: "blur(100px)" }} />
          <i className="blob drift" style={{ left: "20%", bottom: "-35%", width: "68vw", height: "38vw", background: "#34F5A4", opacity: 0.3, filter: "blur(110px)" }} />
        </>
      )}
      {theme === "ferrugem" && (
        <>
          <i className="ember pulse" style={{ right: "-12%", bottom: "-25%", width: "60vw", height: "60vw", background: "radial-gradient(circle,#FF6A1A 0%,rgba(255,106,26,0) 70%)" }} />
          <i className="ember pulse rev" style={{ left: "-14%", top: "-28%", width: "48vw", height: "48vw", background: "radial-gradient(circle,#C2410C 0%,rgba(194,65,12,0) 70%)" }} />
        </>
      )}
      {theme === "holo" && (
        <div className="holo-wrap">
          <i className="holo-disc" />
        </div>
      )}
      {theme === "mesh" && (
        <>
          <i className="blob mesh-a" style={{ left: "-18%", top: "-30%", width: "64vw", height: "64vw", background: "#5B2BFF", filter: "blur(80px)" }} />
          <i className="blob mesh-b" style={{ right: "-20%", top: "-25%", width: "60vw", height: "60vw", background: "#C2185B", filter: "blur(80px)" }} />
          <i className="blob mesh-b" style={{ left: "-14%", bottom: "-40%", width: "64vw", height: "64vw", background: "#FF6A3D", filter: "blur(80px)" }} />
          <i className="blob mesh-a" style={{ right: "-10%", bottom: "-42%", width: "60vw", height: "60vw", background: "#00B8D9", filter: "blur(80px)" }} />
        </>
      )}
      {theme === "sakura" && (
        <>
          <i className="sakura-sun" />
          {[0, 1, 2, 3, 4, 5, 6, 7].map((i) => (
            <i key={i} className="petal" style={{ left: `${12 + i * 11}%`, animationDuration: `${11 + (i % 4) * 2}s`, animationDelay: `${(i * 1.7) % 9}s`, width: 10 + (i % 3) * 3, height: 8 + (i % 3) * 2 }} />
          ))}
        </>
      )}
      {theme === "constelacao" && <Constellation />}
    </div>
  );
}

/** Pseudoaleatório fixo: as estrelas nascem sempre nos mesmos lugares. */
function rand(seed: number) {
  const x = Math.sin(seed * 127.1) * 43758.5453;
  return x - Math.floor(x);
}

/** Do Attack Shark (ThemeBackground.tsx): estrelas ligadas por linhas, atraídas pelo cursor. */
function Constellation() {
  const ref = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const canvas = ref.current!;
    const ctx = canvas.getContext("2d")!;
    const still = matchMedia("(prefers-reduced-motion: reduce)").matches;
    let w = 0;
    let h = 0;
    let raf = 0;
    const mouse = { x: -999, y: -999 };
    const pts = Array.from({ length: 80 }, (_, i) => ({
      x: rand(i + 1),
      y: rand(i + 200),
      vx: (rand(i + 400) - 0.5) * 0.00035,
      vy: (rand(i + 600) - 0.5) * 0.00035,
      r: 0.8 + rand(i + 800) * 1.6,
    }));
    const resize = () => {
      const dpr = devicePixelRatio || 1;
      w = innerWidth;
      h = innerHeight;
      canvas.width = w * dpr;
      canvas.height = h * dpr;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    };
    const move = (e: MouseEvent) => {
      mouse.x = e.clientX;
      mouse.y = e.clientY;
    };
    const frame = () => {
      ctx.clearRect(0, 0, w, h);
      for (const p of pts) {
        p.x = (p.x + p.vx + 1) % 1;
        p.y = (p.y + p.vy + 1) % 1;
      }
      for (let i = 0; i < pts.length; i++) {
        const a = pts[i];
        let ax = a.x * w;
        let ay = a.y * h;
        const md = Math.hypot(ax - mouse.x, ay - mouse.y);
        const near = Math.max(0, 1 - md / 180);
        ax += (mouse.x - ax) * near * 0.08;
        ay += (mouse.y - ay) * near * 0.08;
        for (let j = i + 1; j < pts.length; j++) {
          const b = pts[j];
          const d = Math.hypot(ax - b.x * w, ay - b.y * h);
          if (d < 130) {
            ctx.strokeStyle = `rgba(140,155,255,${(1 - d / 130) * (0.16 + near * 0.5)})`;
            ctx.lineWidth = 1;
            ctx.beginPath();
            ctx.moveTo(ax, ay);
            ctx.lineTo(b.x * w, b.y * h);
            ctx.stroke();
          }
        }
        ctx.fillStyle = `rgba(200,208,255,${0.55 + near * 0.45})`;
        ctx.beginPath();
        ctx.arc(ax, ay, a.r + near * 1.5, 0, Math.PI * 2);
        ctx.fill();
      }
      if (!still && !document.hidden) raf = requestAnimationFrame(frame);
    };
    const visible = () => {
      cancelAnimationFrame(raf);
      if (!document.hidden) raf = requestAnimationFrame(frame);
    };
    resize();
    raf = requestAnimationFrame(frame);
    addEventListener("resize", resize);
    addEventListener("mousemove", move);
    document.addEventListener("visibilitychange", visible);
    return () => {
      cancelAnimationFrame(raf);
      removeEventListener("resize", resize);
      removeEventListener("mousemove", move);
      document.removeEventListener("visibilitychange", visible);
    };
  }, []);
  return (
    <>
      <div className="constellation-glow" />
      <canvas ref={ref} style={{ width: "100%", height: "100%", display: "block" }} />
    </>
  );
}
