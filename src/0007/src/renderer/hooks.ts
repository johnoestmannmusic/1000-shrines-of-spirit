import { useEffect, useRef } from "react";

/**
 * Runs `callback` on animation frames. Pass `fps` to throttle (e.g. 15 for
 * live meters/playheads) so high-frequency panels stop thrashing React
 * reconciliation and scrolling stays smooth.
 */
export function useAnimationFrame(callback: (dt: number) => void, fps = 0): void {
  const ref = useRef(callback);
  ref.current = callback;
  useEffect(() => {
    let raf = 0;
    let last = performance.now();
    let accumulator = 0;
    const interval = fps > 0 ? 1000 / fps : 0;
    const loop = (now: number) => {
      const dt = (now - last) / 1000;
      last = now;
      if (interval === 0) {
        ref.current(dt);
      } else {
        accumulator += dt * 1000;
        if (accumulator >= interval) {
          const step = accumulator / 1000;
          accumulator = 0;
          ref.current(step);
        }
      }
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(raf);
  }, [fps]);
}
