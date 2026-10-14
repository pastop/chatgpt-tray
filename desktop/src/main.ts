
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import "./style.css";
import { getLimitLabel, getLimitState } from "./limit-state";
import { formatDataAge, formatRemaining } from "./time-format";

interface UsageWindow {
  remaining: number;
  resetAt: number;
}

interface UsageSnapshot {
  primary: UsageWindow | null;
  secondary: UsageWindow | null;
}

interface AppStatus {
  usage: UsageSnapshot | null;
  plan: string | null;
  updatedAt: string | null;
  updatedAtEpoch?: number | null;
  nextRetryAtEpoch?: number | null;
  error: string | null;
  loading: boolean;
  fromCache?: boolean;
}

let currentStatus: AppStatus | null = null;
let unlisten: UnlistenFn | undefined;
let disposed = false;

const app = document.querySelector<HTMLDivElement>("#app");
if (!app) throw new Error("Не найден контейнер статистики");

app.innerHTML = `
  <main class="dashboard">
    <header class="header">
      <div>
        <h1>ChatGPT Usage</h1>
        <p class="subtitle">Лимиты подписки Codex</p>
      </div>
      <span id="connection" class="connection loading" role="status">Загрузка</span>
    </header>

    <section id="primary-card" class="usage-card primary no-data">
      <div class="usage-heading">
        <div><h2>5-часовой лимит</h2><span id="primary-label" class="limit-label">Нет данных</span></div>
        <strong id="primary-value">—</strong>
      </div>
      <div class="progress-track">
        <div id="primary-bar" class="progress-fill" role="progressbar" aria-label="Остаток 5-часового лимита" aria-valuemin="0" aria-valuemax="100"></div>
      </div>
      <div class="reset"><span>Сброс лимита</span><span id="primary-reset">Ожидание данных</span></div>
    </section>

    <section id="secondary-card" class="usage-card secondary no-data">
      <div class="usage-heading">
        <div><h2>Недельный лимит</h2><span id="secondary-label" class="limit-label">Нет данных</span></div>
        <strong id="secondary-value">—</strong>
      </div>
      <div class="progress-track">
        <div id="secondary-bar" class="progress-fill" role="progressbar" aria-label="Остаток недельного лимита" aria-valuemin="0" aria-valuemax="100"></div>
      </div>
      <div class="reset"><span>Сброс лимита</span><span id="secondary-reset">Ожидание данных</span></div>
    </section>

    <div id="cache-notice" class="empty" role="status" hidden>Данные из кеша — актуальность ещё не подтверждена.</div>
    <div id="empty" class="empty" role="status">Загружаем статистику лимитов…</div>
        <div id="error" class="error" role="alert" hidden></div>

    <footer class="footer">
      <span><span id="updated-at">Ещё не обновлялось</span><span id="retry-at" class="limit-label" hidden></span></span>
      <button id="refresh" type="button"><span class="refresh-icon" aria-hidden="true">↻</span> Обновить</button>
    </footer>
  </main>
`;

const elements = new Map<string, HTMLElement>();
function element(id: string): HTMLElement {
  const cached = elements.get(id);
  if (cached) return cached;
  const node = document.getElementById(id);
  if (!node) throw new Error(`Не найден элемент: ${id}`);
  elements.set(id, node);
  return node;
}

function renderRetry(status: AppStatus): void {
  const deadline = status.nextRetryAtEpoch;
  const waiting = deadline != null && Number.isFinite(deadline) && deadline > Date.now() / 1000;
  const retry = element("retry-at");
  retry.hidden = !waiting;
  retry.textContent = waiting
    ? `Повтор после ${new Date(deadline * 1000).toLocaleTimeString("ru-RU")}`
    : "";
  const refresh = element("refresh");
  if (refresh instanceof HTMLButtonElement) {
    refresh.disabled = status.loading || waiting;
    refresh.title = waiting ? "Повтор временно отложен после ошибки" : "Обновить статистику";
  }
}

function renderUpdatedAt(status: AppStatus): void {
  const age = formatDataAge(status.updatedAtEpoch);
  const receivedAt = status.updatedAtEpoch != null && Number.isFinite(status.updatedAtEpoch)
    ? new Date(status.updatedAtEpoch * 1000).toLocaleString("ru-RU")
    : status.updatedAt;
  element("updated-at").textContent = receivedAt
    ? `${status.fromCache ? "Кеш получен" : "Успешно обновлено"}: ${receivedAt}${age ? ` · ${age}` : ""}`
    : "Ещё не обновлялось";
}

function renderWindow(
  prefix: "primary" | "secondary",
  window: UsageWindow | null
): void {
  const value = element(`${prefix}-value`);
  const bar = element(`${prefix}-bar`);
  const reset = element(`${prefix}-reset`);

  const remaining = window && Number.isFinite(window.remaining)
    ? Math.max(0, Math.min(100, window.remaining))
    : null;
  const state = getLimitState(remaining);
  const label = getLimitLabel(state, remaining);
  element(`${prefix}-card`).className = `usage-card ${prefix} ${state}`;
  element(`${prefix}-label`).textContent = label;
  if (!window || remaining === null) {
    bar.removeAttribute("aria-valuenow");
    bar.setAttribute("aria-valuetext", "Нет данных");
    value.textContent = "—";
    bar.style.width = "0%";
    reset.textContent = "Нет данных";
    return;
  }


  value.textContent = `${remaining}%`;
  bar.style.width = `${remaining}%`;
  bar.setAttribute("aria-valuenow", String(remaining));
  bar.setAttribute("aria-valuetext", `${remaining}% — ${label}`);

  reset.textContent = formatRemaining(window.resetAt);
}

function render(status: AppStatus): void {
  if (disposed) return;
  currentStatus = status;
  const hasData = Boolean(status.usage?.primary || status.usage?.secondary);

  renderWindow("primary", status.usage?.primary ?? null);
  renderWindow("secondary", status.usage?.secondary ?? null);

  const connection = element("connection");
  connection.textContent = status.loading
    ? "Обновление..."
    : status.error
      ? "Ошибка"
      : status.fromCache
        ? "Данные из кеша"
        : hasData
          ? "Подключено"
          : "Нет данных";

  connection.className =
    "connection " +
    (status.loading ? "loading" : status.error ? "offline" : hasData && !status.fromCache ? "online" : "");
      element("cache-notice").hidden = !status.fromCache;
  const empty = element("empty");
  empty.hidden = hasData || Boolean(status.error);
  empty.textContent = status.loading
    ? "Загружаем статистику лимитов…"
    : "Лимиты пока недоступны. Попробуйте обновить статистику.";
  element("app").setAttribute("aria-busy", String(status.loading));

  const error = element("error");
  error.hidden = !status.error;
  error.textContent = status.error
    ? `${hasData ? "Не удалось обновить. Показаны последние данные." : "Не удалось загрузить статистику."}\n${status.error}`
    : "";

  renderUpdatedAt(status);

  renderRetry(status);
}

async function initialize(): Promise<void> {
  // Сначала подписываемся на события, чтобы не пропустить обновление.
  let receivedUpdate = false;
  const stop = await listen<AppStatus>("usage-updated", (event) => {
    receivedUpdate = true;
    render(event.payload);
  });
  if (disposed) {
    stop();
    return;
  }
  unlisten = stop;

  try {
    const status = await invoke<AppStatus>("get_status");
    if (!receivedUpdate) render(status);
  } catch (error) {
    // A newer event already supplied the state even if the initial request failed.
    if (!receivedUpdate) throw error;
  }
}

async function requestRefresh(): Promise<void> {
  if (!currentStatus || currentStatus.loading ||
    (currentStatus.nextRetryAtEpoch != null && currentStatus.nextRetryAtEpoch > Date.now() / 1000)) return;
  try {
    await invoke("request_refresh");
  } catch (error) {
    console.error("Ошибка запроса обновления:", error);
    if (currentStatus) render({ ...currentStatus, loading: false, error: String(error) });
  }
}
element("refresh").addEventListener("click", requestRefresh);

// Локальный обратный отсчёт и возраст данных, без HTTP-запросов.
const timer = setInterval(() => {
  if (!currentStatus) return;
  renderUpdatedAt(currentStatus);
  renderRetry(currentStatus);
  if (currentStatus.usage) {
    renderWindow("primary", currentStatus.usage.primary);
    renderWindow("secondary", currentStatus.usage.secondary);
  }
}, 1000);

function dispose(): void {
  disposed = true;
  clearInterval(timer);
  unlisten?.();
  element("refresh").removeEventListener("click", requestRefresh);
}
window.addEventListener("pagehide", dispose, { once: true });
if (import.meta.hot) import.meta.hot.dispose(dispose);

initialize().catch((error) => {
  if (disposed) return;
  console.error("Ошибка инициализации:", error);
  element("connection").textContent = "Ошибка запуска";
  element("connection").className = "connection offline";
  element("empty").hidden = true;
  const errorElement = element("error");
  errorElement.hidden = false;
  errorElement.textContent = String(error);
});
