import { mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import Database from "libsql";
import { Kysely, SqliteDialect, type Generated } from "kysely";
import type { CallLogStatus } from "./call-logs";
import type { PromptMode, ZhToEnTone } from "./prompts";

export const DEFAULT_PROMPT_LOG_DB_PATH = "data/prompt-pocket.sqlite";

export type CallLogsTable = {
  id: Generated<number>;
  created_at: string;
  duration_ms: number;
  status: CallLogStatus;
  mode: PromptMode;
  model: string;
  tone: ZhToEnTone | null;
  input: string;
  output_text: string | null;
  error: string | null;
  request_id: string | null;
};

export type PromptPocketDatabase = {
  call_logs: CallLogsTable;
};

export function getCallLogDbPath(env: Record<string, string | undefined> = process.env): string {
  const configuredPath = env.PROMPT_LOG_DB_PATH?.trim() || DEFAULT_PROMPT_LOG_DB_PATH;

  if (configuredPath === ":memory:") {
    return configuredPath;
  }

  return resolve(process.cwd(), configuredPath);
}

export function createPromptPocketDatabase(databasePath = getCallLogDbPath()): Kysely<PromptPocketDatabase> {
  if (databasePath !== ":memory:") {
    mkdirSync(dirname(databasePath), { recursive: true });
  }

  return new Kysely<PromptPocketDatabase>({
    dialect: new SqliteDialect({
      database: new Database(databasePath)
    })
  });
}
