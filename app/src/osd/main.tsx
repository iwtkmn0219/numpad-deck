import React, { useEffect, useState } from "react";
import ReactDOM from "react-dom/client";
import { listen } from "@tauri-apps/api/event";
import { Osd } from "./Osd";
import { OSD_EVENT, type OsdPayload } from "./types";
import "../index.css";

// OSD 창은 투명하다. 배경을 칠하지 않는다.
document.documentElement.style.background = "transparent";
document.body.style.background = "transparent";

function App() {
  const [payload, setPayload] = useState<OsdPayload | null>(null);

  useEffect(() => {
    const unlisten = listen<OsdPayload>(OSD_EVENT, (event) => setPayload(event.payload));
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  if (!payload) return null;
  return <Osd payload={payload} />;
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
