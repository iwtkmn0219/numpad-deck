import React from "react";
import ReactDOM from "react-dom/client";
import "../index.css";

// 설정 편집기는 3단계에서 만든다. 지금은 이 창을 띄우지 않는다.
ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <p className="p-6 text-sm text-neutral-600">설정 편집기는 준비 중입니다.</p>
  </React.StrictMode>,
);
