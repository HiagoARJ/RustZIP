import { X } from "lucide-react";
import { useEffect, type ReactNode } from "react";

export function Check({ checked, onChange, label, hint, disabled }: {
  checked: boolean;
  onChange: (v: boolean) => void;
  label: ReactNode;
  hint?: ReactNode;
  disabled?: boolean;
}) {
  return (
    <label className={`flex items-start gap-2.5 py-1 ${disabled ? "opacity-50" : ""}`}>
      <input type="checkbox" className="check mt-px" checked={checked} disabled={disabled} onChange={(e) => onChange(e.target.checked)} />
      <span className="leading-[18px]">
        {label}
        {hint && <span className="block text-[12px] text-[var(--faint)]">{hint}</span>}
      </span>
    </label>
  );
}

export function Radio({ checked, onChange, label, hint }: { checked: boolean; onChange: () => void; label: ReactNode; hint?: ReactNode }) {
  return (
    <label className="flex items-start gap-2.5 py-1">
      <input type="radio" className="radio mt-px" checked={checked} onChange={onChange} />
      <span className="leading-[18px]">
        {label}
        {hint && <span className="block text-[12px] text-[var(--faint)]">{hint}</span>}
      </span>
    </label>
  );
}

export function Label({ children, right }: { children: ReactNode; right?: ReactNode }) {
  return (
    <div className="mb-1.5 flex items-center justify-between text-[12px] font-semibold text-[var(--muted)]">
      <span>{children}</span>
      {right}
    </div>
  );
}

export function Section({ title, children, className = "" }: { title?: ReactNode; children: ReactNode; className?: string }) {
  return (
    <section className={`card glass border border-[var(--line)] bg-[var(--panel)] p-3 ${className}`}>
      {title && <h3 className="mb-2 text-[12px] font-semibold uppercase tracking-wide text-[var(--faint)]">{title}</h3>}
      {children}
    </section>
  );
}

export function Tabs<T extends string>({ value, onChange, items }: { value: T; onChange: (v: T) => void; items: { id: T; label: string }[] }) {
  return (
    <div className="flex gap-1 border-b border-[var(--line)] px-4">
      {items.map((t) => (
        <button
          key={t.id}
          onClick={() => onChange(t.id)}
          className={`relative px-3 py-2 ${value === t.id ? "font-semibold" : "text-[var(--muted)] hover:text-[var(--text)]"}`}
        >
          {t.label}
          {value === t.id && <span className="absolute inset-x-3 -bottom-px h-[3px] rounded-full bg-[var(--accent)]" />}
        </button>
      ))}
    </div>
  );
}

/** Diálogo dentro da janela (no gerenciador). */
export function Modal({ title, onClose, children, width = 560 }: { title: ReactNode; onClose: () => void; children: ReactNode; width?: number }) {
  useEffect(() => {
    const k = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", k);
    return () => window.removeEventListener("keydown", k);
  }, [onClose]);
  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/30 p-6" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="rise card-lg glass flex max-h-full flex-col overflow-hidden border border-[var(--line)] bg-[var(--bg)] shadow-[var(--shadow)]" style={{ width }}>
        <div className="flex items-center justify-between px-4 pt-3 pb-1">
          <h2 className="text-[15px] font-semibold">{title}</h2>
          <button className="btn btn-ghost !min-h-7 !px-1.5" onClick={onClose} aria-label="Fechar">
            <X size={16} />
          </button>
        </div>
        {children}
      </div>
    </div>
  );
}

export function Footer({ children }: { children: ReactNode }) {
  return <div className="flex items-center justify-end gap-2 border-t border-[var(--line)] bg-[var(--panel)] px-4 py-3">{children}</div>;
}
