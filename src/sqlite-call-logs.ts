import type { Kysely, Selectable } from "kysely";
import type { CallLogCreateInput, CallLogEntry, CallLogStore } from "./call-logs";
import {
  createPromptPocketDatabase,
  getCallLogDbPath,
  type CallLogsTable,
  type PromptPocketDatabase
} from "./database";

type SqliteCallLogStoreOptions = {
  limit: number;
  databasePath?: string;
  db?: Kysely<PromptPocketDatabase>;
};

export function createSqliteCallLogStore({
  limit,
  databasePath = getCallLogDbPath(),
  db
}: SqliteCallLogStoreOptions): CallLogStore {
  let database = db;

  function getDatabase() {
    database ??= createPromptPocketDatabase(databasePath);

    return database;
  }

  return {
    limit,
    async add(input) {
      const entry = await getDatabase()
        .insertInto("call_logs")
        .values(toInsertableRow(input))
        .returningAll()
        .executeTakeFirstOrThrow();

      return toCallLogEntry(entry);
    },
    async list() {
      const entries = await getDatabase()
        .selectFrom("call_logs")
        .selectAll()
        .orderBy("created_at", "desc")
        .orderBy("id", "desc")
        .limit(limit)
        .execute();

      return entries.map(toCallLogEntry);
    },
    async close() {
      if (!database || db) {
        return;
      }

      await database.destroy();
      database = undefined;
    }
  };
}

function toInsertableRow(input: CallLogCreateInput) {
  return {
    created_at: new Date().toISOString(),
    duration_ms: input.durationMs,
    status: input.status,
    mode: input.mode,
    model: input.model,
    tone: input.tone ?? null,
    input: input.input,
    output_text: input.outputText ?? null,
    error: input.error ?? null,
    request_id: input.requestId ?? null
  };
}

function toCallLogEntry(row: Selectable<CallLogsTable>): CallLogEntry {
  return {
    id: row.id,
    createdAt: row.created_at,
    durationMs: row.duration_ms,
    status: row.status,
    mode: row.mode,
    model: row.model,
    tone: row.tone ?? undefined,
    input: row.input,
    outputText: row.output_text ?? undefined,
    error: row.error ?? undefined,
    requestId: row.request_id
  };
}
