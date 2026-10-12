import React from "react";
import ReactDOM from "react-dom/client";
import { initI18n } from "@/i18n";
import { Editor } from "./Editor";
import "../index.css";

initI18n();

// 브라우저 기본 오른쪽 클릭 메뉴(뒤로·새로 고침·검사 등)를 막는다. 앱이 직접 띄우는 메뉴(페이지 탭)는
// 따로 처리한다. 글자 입력란은 복사·붙여넣기 메뉴를 쓸 수 있게 둔다.
window.addEventListener("contextmenu", (e) => {
  const target = e.target as HTMLElement | null;
  if (target?.closest("input, textarea")) return;
  e.preventDefault();
});

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Editor />
  </React.StrictMode>,
);
