import { ko, type Dict } from "./ko";

// 언어 선택은 3-6에서 붙인다 (Windows 표시 언어 + 설정 파일의 language).
export function useT(): Dict {
  return ko;
}

export type { Dict };
