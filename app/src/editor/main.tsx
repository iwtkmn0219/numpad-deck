import React from "react";
import ReactDOM from "react-dom/client";
import { initI18n } from "@/i18n";
import { Editor } from "./Editor";
import "../index.css";

initI18n();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Editor />
  </React.StrictMode>,
);
