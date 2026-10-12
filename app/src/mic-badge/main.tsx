import React from "react";
import ReactDOM from "react-dom/client";
import { initI18n } from "@/i18n";
import { MicBadge } from "./MicBadge";
import "../index.css";

// 배지 창은 투명하다. 배경을 칠하지 않는다.
document.documentElement.style.background = "transparent";
document.body.style.background = "transparent";

initI18n();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <MicBadge />
  </React.StrictMode>,
);
