import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import ts from "typescript";

const source = readFileSync(new URL("./time-format.ts", import.meta.url), "utf8");
const { outputText } = ts.transpileModule(source, {
  compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2020 },
});
const { formatRemaining, formatDataAge } = await import(
  `data:text/javascript;base64,${Buffer.from(outputText).toString("base64")}`
);
const now = 1800000000000;
const epoch = now / 1000;

test("reset in the past or at present does not imply a renewed limit", () => {
  assert.equal(formatRemaining(epoch - 60, now), "Сброс ожидается");
  assert.equal(formatRemaining(epoch, now), "Сброс ожидается");
});
test("reset formatting boundaries", () => {
  assert.equal(formatRemaining(epoch + 59, now), "Менее минуты до сброса");
  assert.equal(formatRemaining(epoch + 60, now), "Через 1 мин.");
  assert.equal(formatRemaining(epoch + 3600, now), "Через 1 ч. 0 мин.");
  assert.equal(formatRemaining(epoch + 86400, now), "Через 1 д. 0 ч.");
  assert.equal(formatRemaining(NaN, now), "Время сброса неизвестно");
});
test("age handles absent data and clock shifts", () => {
  assert.equal(formatDataAge(null, now), "");
  assert.equal(formatDataAge(undefined, now), "");
  assert.equal(formatDataAge(NaN, now), "");
  assert.equal(formatDataAge(epoch + 60, now), "менее минуты назад");
});
test("age formatting boundaries", () => {
  assert.equal(formatDataAge(epoch - 59, now), "менее минуты назад");
  assert.equal(formatDataAge(epoch - 60, now), "1 мин. назад");
  assert.equal(formatDataAge(epoch - 3600, now), "1 ч. назад");
  assert.equal(formatDataAge(epoch - 86400, now), "1 д. назад");
});
