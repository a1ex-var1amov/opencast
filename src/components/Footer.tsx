interface Props {
  /** [label, key] pairs, rightmost last. */
  hints: [string, string][];
  left?: React.ReactNode;
}

export default function Footer({ hints, left }: Props) {
  return (
    <div className="footer">
      <div className="footer-left">{left ?? <span className="brand">OpenCast</span>}</div>
      <div className="footer-hints">
        {hints.map(([label, key]) => (
          <span className="hint" key={`${label}-${key}`}>
            {label} <kbd>{key}</kbd>
          </span>
        ))}
      </div>
    </div>
  );
}
