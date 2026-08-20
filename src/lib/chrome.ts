import type { Settings } from "./types";

export function applyChrome(s: Settings) {
  const root = document.documentElement;
  const theme = s.theme || "aether";
  root.style.setProperty("--accent", s.accent);
  root.dataset.material = s.blur || "acrylic";
  root.dataset.theme = theme;
  root.classList.toggle("solid", s.blur === "solid" || s.blur === "none");
  root.classList.toggle("reduce-motion", !!s.reduceMotion);
}
