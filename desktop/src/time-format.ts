export function formatRemaining(resetAt: number, now = Date.now()): string {
  if (!Number.isFinite(resetAt)) return "Время сброса неизвестно";
  const seconds = Math.max(0, Math.floor(resetAt - now / 1000));
  if (seconds === 0) return "Сброс ожидается";
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  if (days > 0) return `Через ${days} д. ${hours} ч.`;
  if (hours > 0) return `Через ${hours} ч. ${minutes} мин.`;
  if (minutes > 0) return `Через ${minutes} мин.`;
  return "Менее минуты до сброса";
}

export function formatDataAge(updatedAtEpoch: number | null | undefined, now = Date.now()): string {
  if (updatedAtEpoch == null || !Number.isFinite(updatedAtEpoch)) return "";
  const seconds = Math.max(0, Math.floor(now / 1000 - updatedAtEpoch));
  if (seconds < 60) return "менее минуты назад";
  if (seconds < 3600) return `${Math.floor(seconds / 60)} мин. назад`;
  if (seconds < 86400) return `${Math.floor(seconds / 3600)} ч. назад`;
  return `${Math.floor(seconds / 86400)} д. назад`;
}
