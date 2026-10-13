import { useEffect, useRef, useState } from "react";
import type { Command } from "../api";
import Footer from "./Footer";

interface Props {
  command: Command;
  busy: boolean;
  onSubmit: (args: string[]) => void;
  onBack: () => void;
}

export default function ArgsView({ command, busy, onSubmit, onBack }: Props) {
  const specs = command.arguments ?? [];
  const [values, setValues] = useState<string[]>(() => specs.map(() => ""));
  const refs = useRef<(HTMLInputElement | null)[]>([]);

  useEffect(() => refs.current[0]?.focus(), []);

  const submit = () => {
    const missing = specs.findIndex((s, i) => !s.optional && !values[i].trim());
    if (missing >= 0) {
      refs.current[missing]?.focus();
      return;
    }
    onSubmit(values);
  };

  const onKeyDown = (i: number) => (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      onBack();
    } else if (e.key === "Enter") {
      e.preventDefault();
      submit();
    } else if (e.key === "Backspace" && !values[i] && i === 0) {
      e.preventDefault();
      onBack();
    }
  };

  return (
    <>
      <div className="searchbar args-bar">
        <button className="back" onClick={onBack} aria-label="Back">
          ←
        </button>
        <span className="args-icon">{command.icon ?? "›"}</span>
        <span className="args-title">{command.title}</span>
        {specs.map((s, i) => (
          <input
            key={i}
            ref={(el) => {
              refs.current[i] = el;
            }}
            className="arg-input"
            type={s.secure ? "password" : "text"}
            placeholder={s.placeholder + (s.optional ? " (optional)" : "")}
            value={values[i]}
            spellCheck={false}
            onChange={(e) => setValues((v) => v.map((x, j) => (j === i ? e.target.value : x)))}
            onKeyDown={onKeyDown(i)}
          />
        ))}
        {busy && <span className="spinner" aria-label="Working" />}
      </div>
      <div className="list list-hint">
        <div className="empty">
          Fill in {specs.length === 1 ? "the field" : "the fields"} above and press Enter to run{" "}
          <strong>{command.title}</strong>.
        </div>
      </div>
      <Footer
        hints={[
          ["Run", "↵"],
          ["Next field", "⇥"],
          ["Back", "esc"],
        ]}
      />
    </>
  );
}
