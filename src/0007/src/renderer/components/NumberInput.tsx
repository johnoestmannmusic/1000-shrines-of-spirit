import { useEffect, useRef, useState, type KeyboardEvent } from "react";

/**
 * Numeric text input that keeps its own buffer and commits on Enter/blur, so
 * the field can be cleared (backspaced) while typing instead of snapping back
 * to a parsed value on every keystroke.
 */
export function NumberInput({
  value,
  min = -Infinity,
  max = Infinity,
  step = 0.01,
  onChange,
  className,
  title,
  ariaLabel,
  disabled,
}: {
  value: number;
  min?: number;
  max?: number;
  step?: number;
  onChange: (value: number) => void;
  className?: string;
  title?: string;
  ariaLabel?: string;
  disabled?: boolean;
}) {
  const [text, setText] = useState(() => String(value));
  const focused = useRef(false);

  useEffect(() => {
    if (!focused.current) setText(String(value));
  }, [value]);

  const commit = () => {
    const trimmed = text.trim();
    const parsed = Number(trimmed);
    if (trimmed === "" || !Number.isFinite(parsed)) {
      setText(String(value));
      return;
    }
    onChange(Math.min(Math.max(parsed, min), max));
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter") event.currentTarget.blur();
  };

  return (
    <input
      type="text"
      inputMode="decimal"
      data-step={step}
      className={`number-input${className ? ` ${className}` : ""}`}
      title={title}
      aria-label={ariaLabel}
      disabled={disabled}
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
