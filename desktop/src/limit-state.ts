export type LimitState = "good" | "warning" | "critical" | "no-data";

export function getLimitState(remaining: number | null): LimitState {
  if (remaining === null || !Number.isFinite(remaining)) return "no-data";
  if (remaining > 50) return "good";
  if (remaining > 20) return "warning";
  return "critical";
}

export function getLimitLabel(state: LimitState, remaining: number | null): string {
  switch (state) {
    case "good": return "Осталось доступно";
    case "warning": return "Мало осталось";
    case "critical": return remaining !== null && remaining <= 0
      ? "Лимит исчерпан" : "Критический остаток";
    case "no-data": return "Нет данных";
  }
}
