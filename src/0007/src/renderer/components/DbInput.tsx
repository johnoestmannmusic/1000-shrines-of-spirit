import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { dbToLinear, formatDb } from "../util";

/**
 * dB value entry that keeps its own text buffer and only commits on Enter or
 * blur, so typing partial values (e.g. "-2") is never reformatted mid-edit.
 */
export function DbInput({
  value,
  maxDb = 0,
  onChange,
}: {
  value: number;
  maxDb?: number;
  onChange: (linear: number) => void;
}) {
  const [text, setText] = useState(() => formatDb(value));
  const focused = useRef(false);

  useEffect(() => {
    if (!focused.current) setText(formatDb(value));
  }, [value]);

  const commit = () => {
    const trimmed = text.trim().toLowerCase();
    if (trimmed === "-inf" || trimmed === "-infinity" || trimmed === "-∞") {
      onChange(0);
      setText("-∞");
      return;
    }
    const parsed = Number(trimmed);
    if (Number.isFinite(parsed)) {
      const max = Math.max(dbToLinear(maxDb), 1e-6);
      onChange(Math.min(Math.max(dbToLinear(parsed), 0), max));
    } else {
      setText(formatDb(value));
    }
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter") event.currentTarget.blur();
  };

  return (
    <input
      className="db-input mono"
      value={text}
      onFocus={() => {
        focused.current = true;
      }}
      onBlur={() => {
        focused.current = false;
        commit();
      }}
      onChange={(e) => setText(e.target.value)}
      onKeyDown={onKeyDown}
    />
  );
}
