import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type Dispatch,
  type PointerEvent as ReactPointerEvent,
  type RefObject,
  type SetStateAction,
} from "react";
import { api } from "./api";
import type { Game } from "./types";

function orderedIds(games: Game[]) {
  return [...games].sort((a, b) => a.sortOrder - b.sortOrder).map((g) => g.id);
}

type Ghost = {
  id: string;
  x: number;
  y: number;
  w: number;
  h: number;
  name: string;
  cover?: string | null;
  accent?: string | null;
};

/**
 * Pointer-based custom order (not HTML5 DnD).
 * WebView2 / Tauri often break native drag; this matches Nebula's live reorder feel.
 */
export function useCustomOrder(
  enabled: boolean,
  scrollRef: RefObject<HTMLElement | null>,
  games: Game[],
  setGames: Dispatch<SetStateAction<Game[]>>,
) {
  const [dragId, setDragId] = useState<string | null>(null);
  const [ghost, setGhost] = useState<Ghost | null>(null);
  const enabledRef = useRef(enabled);
  const gamesRef = useRef(games);
  const dragIdRef = useRef<string | null>(null);
  const dirtyRef = useRef(false);
  const lastOverRef = useRef<string | null>(null);
  const pointer = useRef({ x: 0, y: 0, active: false, armed: false, ox: 0, oy: 0 });
  const ghostSize = useRef({ w: 160, h: 214 });
  const rafScroll = useRef(0);
  enabledRef.current = enabled;
  gamesRef.current = games;

  const liveMove = useCallback(
    (toId: string) => {
      const fromId = dragIdRef.current;
      if (!fromId || fromId === toId || lastOverRef.current === toId) return;
      lastOverRef.current = toId;
      setGames((prev) => {
        const ids = orderedIds(prev);
        const from = ids.indexOf(fromId);
        const to = ids.indexOf(toId);
        if (from < 0 || to < 0 || from === to) return prev;
        const next = [...ids];
        const [item] = next.splice(from, 1);
        next.splice(to, 0, item);
        dirtyRef.current = true;
        const map = new Map(prev.map((g) => [g.id, g]));
        return next
          .map((id, i) => ({ ...map.get(id)!, sortOrder: i * 10 }))
          .concat(prev.filter((g) => !next.includes(g.id)));
      });
    },
    [setGames],
  );

  const stopScroll = useCallback(() => {
    if (rafScroll.current) cancelAnimationFrame(rafScroll.current);
    rafScroll.current = 0;
  }, []);

  const tickScroll = useCallback(() => {
    const root = scrollRef.current;
    if (!root || !dragIdRef.current) {
      rafScroll.current = 0;
      return;
    }
    const rect = root.getBoundingClientRect();
    const y = pointer.current.y;
    const EDGE = 96;
    const MAX = 34;
    let dy = 0;
    if (y < rect.top + EDGE) {
      const t = Math.min(1, (rect.top + EDGE - y) / EDGE);
      dy = -Math.ceil(MAX * (0.35 + 0.65 * t * t));
    } else if (y > rect.bottom - EDGE) {
      const t = Math.min(1, (y - (rect.bottom - EDGE)) / EDGE);
      dy = Math.ceil(MAX * (0.35 + 0.65 * t * t));
    }
    if (dy) root.scrollTop += dy;
    rafScroll.current = requestAnimationFrame(tickScroll);
  }, [scrollRef]);

  const endDrag = useCallback(async () => {
    stopScroll();
    const dirty = dirtyRef.current;
    const ids = orderedIds(gamesRef.current);
    dragIdRef.current = null;
    lastOverRef.current = null;
    dirtyRef.current = false;
    pointer.current.active = false;
    pointer.current.armed = false;
    setDragId(null);
    setGhost(null);
    document.body.classList.remove("is-reordering");
    if (dirty && ids.length) {
      try {
        await api.reorderGames(ids);
      } catch {
        /* keep local */
      }
    }
  }, [stopScroll]);

  useEffect(() => {
    const onMove = (e: PointerEvent) => {
      if (!pointer.current.armed && !pointer.current.active) return;
      pointer.current.x = e.clientX;
      pointer.current.y = e.clientY;

      if (!pointer.current.active) {
        const dx = e.clientX - pointer.current.ox;
        const dy = e.clientY - pointer.current.oy;
        if (dx * dx + dy * dy < 36) return; // 6px threshold — still a click
        if (!enabledRef.current || !dragIdRef.current) return;
        pointer.current.active = true;
        document.body.classList.add("is-reordering");
        setDragId(dragIdRef.current);
        const g = gamesRef.current.find((x) => x.id === dragIdRef.current);
        setGhost({
          id: dragIdRef.current,
          x: e.clientX,
          y: e.clientY,
          w: ghostSize.current.w,
          h: ghostSize.current.h,
          name: g?.name || "",
          cover: g?.coverUrl || g?.iconUrl,
          accent: g?.accent,
        });
        if (!rafScroll.current) rafScroll.current = requestAnimationFrame(tickScroll);
      } else {
        setGhost((prev) =>
          prev
            ? { ...prev, x: e.clientX, y: e.clientY }
            : prev,
        );
        const el = document.elementFromPoint(e.clientX, e.clientY) as HTMLElement | null;
        const target = el?.closest("[data-gid]") as HTMLElement | null;
        const toId = target?.dataset.gid;
        if (toId) liveMove(toId);
      }
    };

    const onUp = () => {
      if (!pointer.current.armed && !pointer.current.active) return;
      void endDrag();
    };

    window.addEventListener("pointermove", onMove, true);
    window.addEventListener("pointerup", onUp, true);
    window.addEventListener("pointercancel", onUp, true);
    return () => {
      window.removeEventListener("pointermove", onMove, true);
      window.removeEventListener("pointerup", onUp, true);
      window.removeEventListener("pointercancel", onUp, true);
      stopScroll();
    };
  }, [endDrag, liveMove, stopScroll, tickScroll]);

  const onCardPointerDown = useCallback(
    (id: string, e: ReactPointerEvent) => {
      if (!enabled || e.button !== 0) return;
      if ((e.target as HTMLElement).closest(".game-card-play")) return;
      const card = (e.currentTarget as HTMLElement).querySelector(".game-card") as HTMLElement | null;
      if (card) {
        const r = card.getBoundingClientRect();
        ghostSize.current = { w: r.width, h: r.height };
      }
      pointer.current = {
        x: e.clientX,
        y: e.clientY,
        ox: e.clientX,
        oy: e.clientY,
        armed: true,
        active: false,
      };
      dragIdRef.current = id;
      lastOverRef.current = id;
      dirtyRef.current = false;
    },
    [enabled],
  );

  const didDrag = useCallback(() => pointer.current.active, []);

  return { dragId, ghost, onCardPointerDown, didDrag };
}
