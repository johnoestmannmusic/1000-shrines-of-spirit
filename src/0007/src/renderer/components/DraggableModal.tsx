import { useRef, useState, type PointerEvent as ReactPointerEvent, type ReactNode } from "react";

/**
 * Backdrop + draggable panel used by every modal, matching the movable
 * Sampler/Spectral editor window.
 */
export function DraggableModal({
  title,
  onClose,
  children,
  width,
}: {
  title: string;
  onClose: () => void;
  children: ReactNode;
  width?: number;
}) {
  const [position, setPosition] = useState({ x: 0, y: 0 });
  const drag = useRef<{ dx: number; dy: number } | null>(null);

  const onPointerDown = (event: ReactPointerEvent<HTMLDivElement>) => {
    if ((event.target as HTMLElement).closest("button")) return;
    drag.current = { dx: event.clientX - position.x, dy: event.clientY - position.y };
    event.currentTarget.setPointerCapture(event.pointerId);
  };
  const onPointerMove = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (!drag.current) return;
    setPosition({
      x: event.clientX - drag.current.dx,
      y: event.clientY - drag.current.dy,
    });
  };
  const onPointerUp = () => {
    drag.current = null;
  };

  return (
    <div
      className="modal-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div
        className="modal draggable-modal"
        style={{
          transform: `translate(${position.x}px, ${position.y}px)`,
          width: width ? `${width}px` : undefined,
        }}
        onMouseDown={(event) => event.stopPropagation()}
      >
        <div
          className="modal-title draggable-title"
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
        >
          <span>{title}</span>
          <button
            className="close-btn"
            onPointerDown={(event) => event.stopPropagation()}
            onClick={onClose}
          >
            ✕
          </button>
        </div>
        {children}
      </div>
    </div>
  );
}
