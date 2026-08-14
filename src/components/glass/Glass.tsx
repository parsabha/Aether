import { useRef, type CSSProperties, type ReactNode, type MouseEvent } from "react";

export function Glass({
  children,
  className = "",
  style,
  interactive = true,
  onClick,
}: {
  children?: ReactNode;
  className?: string;
  style?: CSSProperties;
  interactive?: boolean;
  onClick?: (e: MouseEvent) => void;
}) {
  const ref = useRef<HTMLDivElement>(null);

  const onMove = (e: MouseEvent) => {
    const el = ref.current;
    if (!el || !interactive) return;
    const r = el.getBoundingClientRect();
    const x = ((e.clientX - r.left) / r.width) * 100;
    const y = ((e.clientY - r.top) / r.height) * 100;
    el.style.setProperty("--mx", `${x}%`);
    el.style.setProperty("--my", `${y}%`);
  };

  return (
    <div
      ref={ref}
      className={`glass ${interactive ? "glass-interactive" : ""} ${className}`}
      style={style}
      onMouseMove={onMove}
      onClick={onClick}
    >
      {children}
    </div>
  );
}
