import { useEffect, useRef } from "react";
import { api } from "../api";
import type { Flash } from "../App";
import Footer from "./Footer";

interface Props {
  title: string;
  text: string;
  onBack: () => void;
  flash: Flash;
}

export default function OutputView({ title, text, onBack, flash }: Props) {
  const root = useRef<HTMLDivElement>(null);
  useEffect(() => root.current?.focus(), []);

  const copy = async () => {
    await api.copy(text);
    flash("Copied output");
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      onBack();
    } else if (e.key === "Enter" || ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "c" && !window.getSelection()?.toString())) {
      e.preventDefault();
      void copy();
    }
  };

  return (
    <div className="view" tabIndex={0} ref={root} onKeyDown={onKeyDown}>
      <div className="searchbar">
        <button className="back" onClick={onBack} aria-label="Back">
          ←
        </button>
        <span className="args-title">{title}</span>
      </div>
      <pre className="output">{text || "(no output)"}</pre>
      <Footer
        hints={[
          ["Copy", "↵"],
          ["Back", "esc"],
        ]}
      />
    </div>
  );
}
