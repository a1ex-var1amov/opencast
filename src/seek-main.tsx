import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import Seek from "./Seek";
import "./styles.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <Seek />
  </StrictMode>,
);
