import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import ts from "typescript";

const source = readFileSync(new URL("./limit-state.ts", import.meta.url), "utf8");
const { outputText } = ts.transpileModule(source, {
  compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2020 },
});
const { getLimitState, getLimitLabel } = await import(
  `data:text/javascript;base64,${Buffer.from(outputText).toString("base64")}`
);

for (const [remaining, expected] of [
  [0, "critical"], [20, "critical"], [21, "warning"],
  [50, "warning"], [51, "good"], [100, "good"],
  [null, "no-data"], [NaN, "no-data"], [Infinity, "no-data"],
]) {
  test(`remaining ${remaining}: ${expected}`, () => {
    assert.equal(getLimitState(remaining), expected);
  });
}

test("text distinguishes exhaustion, critical, warning and missing data", () => {
  assert.equal(getLimitLabel("critical", 0), "Лимит исчерпан");
  assert.equal(getLimitLabel("critical", 20), "Критический остаток");
  assert.equal(getLimitLabel("warning", 21), "Мало осталось");
  assert.equal(getLimitLabel("good", 100), "Осталось доступно");
  assert.equal(getLimitLabel("no-data", null), "Нет данных");
});
