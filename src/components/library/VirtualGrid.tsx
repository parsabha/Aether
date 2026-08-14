import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";

/**
 * Portrait store covers are 3:4 (width:height).
 * `aspect` is height/width of the art area; `metaH` is the title strip below.
 */
export function VirtualGrid<T>({
  items,
  minCardWidth = 148,
  gap = 14,
  aspect = 4 / 3,
  metaH = 52,
  overscan = 1,
  renderItem,
  scrollRef,
  getKey,
}: {
  items: T[];
  minCardWidth?: number;
  gap?: number;
  /** Art height ÷ width (3:4 cover → 4/3). */
  aspect?: number;
  metaH?: number;
  overscan?: number;
  renderItem: (item: T, index: number) => ReactNode;
  scrollRef: React.RefObject<HTMLElement | null>;
  getKey?: (item: T, index: number) => string | number;
}) {
  const hostRef = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(900);
  const [scrollTop, setScrollTop] = useState(0);
  const [viewH, setViewH] = useState(700);

  useEffect(() => {
    const el = scrollRef.current;
    const host = hostRef.current;
    if (!el || !host) return;
    const onScroll = () => setScrollTop(el.scrollTop);
    el.addEventListener("scroll", onScroll, { passive: true });
    const ro = new ResizeObserver(() => {
      setWidth(host.clientWidth);
      setViewH(el.clientHeight);
    });
    ro.observe(host);
    ro.observe(el);
    setWidth(host.clientWidth);
    setViewH(el.clientHeight);
    return () => {
      el.removeEventListener("scroll", onScroll);
      ro.disconnect();
    };
  }, [scrollRef]);

  const cols = Math.max(2, Math.floor((width + gap) / (minCardWidth + gap)));
  const cardW = (width - gap * (cols - 1)) / cols;
  const cardH = cardW * aspect + metaH;
  const rowH = cardH + gap;
  const rows = Math.ceil(items.length / cols);
  const totalH = rows * rowH;
  const startRow = Math.max(0, Math.floor(scrollTop / rowH) - overscan);
  const endRow = Math.min(rows, Math.ceil((scrollTop + viewH) / rowH) + overscan);

  const visible = useMemo(() => {
    const out: { item: T; index: number; x: number; y: number; w: number; h: number }[] = [];
    for (let row = startRow; row < endRow; row++) {
      for (let col = 0; col < cols; col++) {
        const index = row * cols + col;
        if (index >= items.length) break;
        out.push({
          item: items[index],
          index,
          x: col * (cardW + gap),
          y: row * rowH,
          w: cardW,
          h: cardH,
        });
      }
    }
    return out;
  }, [items, startRow, endRow, cols, cardW, cardH, gap, rowH]);

  return (
    <div ref={hostRef} className="virtual-grid" style={{ height: totalH, position: "relative" }}>
      {visible.map(({ item, index, x, y, w, h }) => (
        <div
          key={getKey ? getKey(item, index) : index}
          className="virtual-cell"
          style={{
            position: "absolute",
            left: x,
            top: y,
            width: w,
            height: h,
          }}
        >
          {renderItem(item, index)}
        </div>
      ))}
    </div>
  );
}
