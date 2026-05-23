import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, test } from "bun:test";
import { up as createCallLogsTable } from "../migrations/202605230001_create_call_logs";
import { createPromptPocketDatabase } from "./database";
import { createSqliteCallLogStore } from "./sqlite-call-logs";

describe("SQLite call log store", () => {
  test("persists entries and uses limit only for listing", async () => {
    const tempDir = mkdtempSync(join(tmpdir(), "prompt-pocket-"));
    const databasePath = join(tempDir, "logs.sqlite");

    try {
      const setupDb = createPromptPocketDatabase(databasePath);
      await createCallLogsTable(setupDb);
      await setupDb.destroy();

      const limitedStore = createSqliteCallLogStore({
        limit: 1,
        databasePath
      });

      await limitedStore.add({
        durationMs: 12,
        status: "success",
        mode: "zh_to_en",
        model: "gpt-5.2",
        tone: "casual",
        input: "hello",
        outputText: "processed:hello",
        requestId: "req_success"
      });
      await limitedStore.add({
        durationMs: 8,
        status: "error",
        mode: "polish_en",
        model: "gpt-5.2",
        input: "fail",
        error: "mock failure"
      });

      const limitedLogs = await limitedStore.list();
      await limitedStore.close?.();

      expect(limitedLogs).toHaveLength(1);
      expect(limitedLogs[0]).toMatchObject({
        status: "error",
        input: "fail",
        error: "mock failure"
      });

      const reopenedStore = createSqliteCallLogStore({
        limit: 10,
        databasePath
      });
      const persistedLogs = await reopenedStore.list();
      await reopenedStore.close?.();

      expect(persistedLogs).toHaveLength(2);
      expect(persistedLogs[0]).toMatchObject({
        status: "error",
        input: "fail"
      });
      expect(persistedLogs[1]).toMatchObject({
        status: "success",
        input: "hello",
        outputText: "processed:hello",
        requestId: "req_success"
      });
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  });
});
