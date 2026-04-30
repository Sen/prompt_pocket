import { type Context, Hono } from "hono";
import { cors } from "hono/cors";
import {
  clearAuthCookie,
  createAuthMiddleware,
  createLoginAttemptLimiter,
  getAuthStatus,
  isAuthEnabled,
  type LoginAttemptBlock,
  type LoginAttemptLimiter,
  setAuthCookie,
  verifyAppPassword
} from "./auth";
import { createCallLogStore, getCallLogLimit, type CallLogStore } from "./call-logs";
import {
  createOpenAIClient,
  getRuntimeConfig,
  listAvailableModels,
  runPrompt,
  type RuntimeEnv,
  type RunPromptResult
} from "./openai";
import { flattenModelGroups, isValidModelId, mergeModelGroups, type ModelCatalogGroup } from "./model-catalog";
import {
  DEFAULT_ZH_TO_EN_TONE,
  isPromptMode,
  isZhToEnTone,
  PROMPT_MODES,
  type PromptMode,
  type ZhToEnTone
} from "./prompts";

type RunBody = {
  mode?: unknown;
  input?: unknown;
  model?: unknown;
  tone?: unknown;
};

type LoginBody = {
  password?: unknown;
};

type RunPromptHandler = (input: {
  mode: PromptMode;
  input: string;
  model: string;
  tone?: ZhToEnTone;
  env: RuntimeEnv;
}) => Promise<RunPromptResult>;

type ListModelsHandler = (input: {
  env: RuntimeEnv;
}) => Promise<string[]>;

export type AppDependencies = {
  env?: RuntimeEnv;
  runPromptHandler?: RunPromptHandler;
  listModelsHandler?: ListModelsHandler;
  callLogStore?: CallLogStore;
  loginAttemptLimiter?: LoginAttemptLimiter;
};

type ModelListPayload = {
  model: string;
  models: string[];
  modelGroups: ModelCatalogGroup[];
  source: "static" | "dynamic";
  error?: string;
};

function jsonError(c: Context, status: 400 | 401 | 404 | 422 | 429 | 500 | 502, message: string) {
  return c.json(
    {
      error: message
    },
    status
  );
}

function jsonLoginAttemptBlock(c: Context, block: LoginAttemptBlock) {
  c.header("Retry-After", String(block.retryAfterSeconds));

  return c.json(
    {
      error: block.message,
      reason: block.reason,
      retryAfterSeconds: block.retryAfterSeconds
    },
    429
  );
}

function getClientIp(c: Context): string {
  const forwardedFor = c.req.header("x-forwarded-for")?.split(",")[0]?.trim();

  return (
    c.req.header("cf-connecting-ip")?.trim() ||
    c.req.header("x-real-ip")?.trim() ||
    forwardedFor ||
    c.req.header("x-client-ip")?.trim() ||
    "unknown"
  );
}

async function parseLoginBody(c: Context): Promise<LoginBody | null> {
  try {
    return (await c.req.json()) as LoginBody;
  } catch {
    return null;
  }
}

async function parseRunBody(c: Context): Promise<RunBody | null> {
  try {
    return (await c.req.json()) as RunBody;
  } catch {
    return null;
  }
}

function getRunTone(mode: PromptMode, value: unknown): ZhToEnTone | undefined | null {
  if (mode !== "zh_to_en") {
    return undefined;
  }

  if (value === undefined) {
    return DEFAULT_ZH_TO_EN_TONE;
  }

  return isZhToEnTone(value) ? value : null;
}

export function createApp(dependencies: AppDependencies = {}) {
  const app = new Hono();
  const env = dependencies.env ?? process.env;
  const runtimeConfig = getRuntimeConfig(env);
  const openaiClient = runtimeConfig.apiKey ? createOpenAIClient(runtimeConfig) : undefined;
  const runPromptHandler =
    dependencies.runPromptHandler ?? ((input) => runPrompt({ ...input, client: openaiClient }));
  const listModelsHandler =
    dependencies.listModelsHandler ?? ((input) => listAvailableModels({ ...input, client: openaiClient }));
  const callLogStore = dependencies.callLogStore ?? createCallLogStore(getCallLogLimit(env));
  const authMiddleware = createAuthMiddleware(env);
  const loginAttemptLimiter = dependencies.loginAttemptLimiter ?? createLoginAttemptLimiter();

  const corsOptions = {
    origin: ["http://127.0.0.1:5173", "http://localhost:5173"],
    allowMethods: ["GET", "POST", "OPTIONS"],
    allowHeaders: ["Content-Type"],
    credentials: true
  };

  app.use("/api/*", cors(corsOptions));
  app.use("/auth/*", cors(corsOptions));

  app.get("/", (c) => {
    return c.text("Prompt Pocket API is running. Open the Vite web app at http://127.0.0.1:5173.");
  });

  app.get("/auth/status", async (c) => {
    return c.json(await getAuthStatus(c, env));
  });

  app.post("/auth/login", async (c) => {
    const clientIp = getClientIp(c);
    const block = loginAttemptLimiter.getBlock(clientIp);

    if (block.blocked) {
      return jsonLoginAttemptBlock(c, block);
    }

    const body = await parseLoginBody(c);

    if (!body) {
      return jsonError(c, 400, "Request body must be valid JSON.");
    }

    if (!verifyAppPassword(env, body.password)) {
      const failure = loginAttemptLimiter.recordFailure(clientIp);

      if (failure.blocked) {
        return jsonLoginAttemptBlock(c, failure);
      }

      return jsonError(c, 401, "Invalid password.");
    }

    loginAttemptLimiter.recordSuccess(clientIp);
    await setAuthCookie(c, env);

    return c.json({
      enabled: isAuthEnabled(env),
      authenticated: true
    });
  });

  app.post("/auth/logout", (c) => {
    clearAuthCookie(c);

    return c.json({
      ok: true
    });
  });

  app.use("/health", authMiddleware);
  app.use("/api/*", authMiddleware);

  app.get("/health", (c) => {
    const config = getRuntimeConfig(env);

    return c.json({
      ok: true,
      model: config.model,
      models: config.modelOptions,
      modelGroups: config.modelGroups,
      hasApiKey: Boolean(config.apiKey),
      hasProxy: Boolean(config.proxyUrl),
      modes: Object.keys(PROMPT_MODES)
    });
  });

  app.get("/api/logs", (c) => {
    return c.json({
      limit: callLogStore.limit,
      logs: callLogStore.list()
    });
  });

  app.get("/api/models", async (c) => {
    const config = getRuntimeConfig(env);

    if (!config.apiKey) {
      return c.json<ModelListPayload>({
        model: config.model,
        models: config.modelOptions,
        modelGroups: config.modelGroups,
        source: "static"
      });
    }

    try {
      const dynamicModels = await listModelsHandler({ env });
      const modelGroups = mergeModelGroups(config.model, dynamicModels);

      return c.json<ModelListPayload>({
        model: config.model,
        models: flattenModelGroups(modelGroups),
        modelGroups,
        source: "dynamic"
      });
    } catch (error) {
      const message = error instanceof Error ? error.message : "Could not fetch OpenAI models.";

      return c.json<ModelListPayload>({
        model: config.model,
        models: config.modelOptions,
        modelGroups: config.modelGroups,
        source: "static",
        error: message
      });
    }
  });

  app.post("/api/run", async (c) => {
    const config = getRuntimeConfig(env);

    if (!config.apiKey) {
      return jsonError(c, 500, "OPENAI_API_KEY is not configured.");
    }

    const body = await parseRunBody(c);

    if (!body) {
      return jsonError(c, 400, "Request body must be valid JSON.");
    }

    if (!isPromptMode(body.mode)) {
      return jsonError(c, 422, "Invalid prompt mode.");
    }

    const tone = getRunTone(body.mode, body.tone);

    if (tone === null) {
      return jsonError(c, 422, "Invalid translation tone.");
    }

    if (typeof body.input !== "string" || body.input.trim().length === 0) {
      return jsonError(c, 422, "Input text is required.");
    }

    const selectedModel = typeof body.model === "string" && body.model.trim() ? body.model.trim() : config.model;

    if (!isValidModelId(selectedModel)) {
      return jsonError(c, 422, "Invalid model.");
    }

    const startedAt = performance.now();
    const input = body.input.trim();

    try {
      const result = await runPromptHandler({
        mode: body.mode,
        input,
        model: selectedModel,
        tone,
        env
      });

      callLogStore.add({
        durationMs: Math.round(performance.now() - startedAt),
        status: "success",
        mode: body.mode,
        model: selectedModel,
        tone,
        input,
        outputText: result.outputText,
        requestId: result.requestId
      });

      return c.json(result);
    } catch (error) {
      const message = error instanceof Error ? error.message : "OpenAI request failed.";

      callLogStore.add({
        durationMs: Math.round(performance.now() - startedAt),
        status: "error",
        mode: body.mode,
        model: selectedModel,
        tone,
        input,
        error: message
      });

      return jsonError(c, 502, message);
    }
  });

  app.notFound((c) => jsonError(c, 404, "Not found."));

  return app;
}

const app = createApp();
const port = Number(process.env.PORT ?? 8787);

export default {
  port,
  fetch: app.fetch
};
