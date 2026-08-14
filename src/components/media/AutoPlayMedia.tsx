import { useEffect, useRef, useState } from "react";

/** Always-playing media — video loops while mounted; off-screen unmount handles GPU budget. */
export function AutoPlayMedia({
  src,
  kind,
  poster,
  className,
  alt = "",
}: {
  src?: string | null;
  kind: string;
  poster?: string | null;
  className?: string;
  alt?: string;
}) {
  const ref = useRef<HTMLVideoElement>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    setFailed(false);
    const v = ref.current;
    if (!v) return;
    const play = () => {
      v.muted = true;
      const p = v.play();
      if (p) p.catch(() => {});
    };
    v.addEventListener("canplay", play);
    play();
    return () => v.removeEventListener("canplay", play);
  }, [src]);

  if (!src || failed) {
    return <div className={`media-fallback ${className || ""}`} />;
  }

  if (kind === "video" || /\.(webm|mp4|mov)(\?|$)/i.test(src)) {
    return (
      <video
        ref={ref}
        className={className}
        src={src}
        poster={poster || undefined}
        muted
        loop
        autoPlay
        playsInline
        preload="auto"
        draggable={false}
        onError={() => setFailed(true)}
      />
    );
  }

  // Never preferred — only if webm not ready yet
  return (
    <img
      className={className}
      src={src}
      alt={alt}
      draggable={false}
      onError={() => setFailed(true)}
    />
  );
}
