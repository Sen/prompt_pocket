import type { PromptMode, ZhToEnTone } from "./prompts";

export const DEFAULT_CALL_LOG_LIMIT = 2000;

export type CallLogStatus = "success" | "error";

export type CallLogEntry = {
  id: number;
  createdAt: string;
  durationMs: number;
  status: CallLogStatus;
  mode: PromptMode;
  model: string;
  tone?: ZhToEnTone;
  input: string;
  outputText?: string;
  error?: string;
  requestId?: string | null;
};

export type CallLogCreateInput = Omit<CallLogEntry, "id" | "createdAt">;

export type CallLogStore = {
  add(entry: CallLogCreateInput): Promise<CallLogEntry>;
  list(): Promise<CallLogEntry[]>;
  close?(): Promise<void>;
  limit: number;
};

export function getCallLogLimit(env: Record<string, string | undefined> = process.env): number {
  const rawLimit = env.PROMPT_LOG_LIMIT?.trim();

  if (!rawLimit) {
    return DEFAULT_CALL_LOG_LIMIT;
  }

  const parsedLimit = Number.parseInt(rawLimit, 10);

  if (!Number.isFinite(parsedLimit) || parsedLimit < 1) {
    return DEFAULT_CALL_LOG_LIMIT;
  }

  return parsedLimit;
}

export function createMemoryCallLogStore(limit: number): CallLogStore {
  const entries: CallLogEntry[] = [];
  let nextId = 1;

  return {
    limit,
    async add(input) {
      const entry: CallLogEntry = {
        id: nextId,
        createdAt: new Date().toISOString(),
        ...input
      };
      nextId += 1;
      entries.unshift(entry);

      return entry;
    },
    async list() {
      return entries.slice(0, limit).map((entry) => ({ ...entry }));
    }
  };
}

export const createCallLogStore = createMemoryCallLogStore;
