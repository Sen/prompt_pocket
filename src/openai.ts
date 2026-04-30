import OpenAI, { type ClientOptions } from "openai";
import { ProxyAgent, fetch as undiciFetch } from "undici";
import {
  DEFAULT_MODEL,
  flattenModelGroups,
  getStaticModelGroups,
  type ModelCatalogGroup
} from "./model-catalog";
import { getPromptMode, type PromptMode, type ZhToEnTone } from "./prompts";

export { DEFAULT_MODEL } from "./model-catalog";

export type RuntimeEnv = Record<string, string | undefined>;

export type RuntimeConfig = {
  apiKey?: string;
  model: string;
  modelOptions: string[];
  modelGroups: ModelCatalogGroup[];
  proxyUrl?: string;
};

export type RunPromptInput = {
  mode: PromptMode;
  input: string;
  model?: string;
  tone?: ZhToEnTone;
  env?: RuntimeEnv;
  client?: OpenAI;
};

export type RunPromptResult = {
  outputText: string;
  mode: PromptMode;
  model: string;
  requestId?: string | null;
};

type OpenAIFetch = NonNullable<ClientOptions["fetch"]>;
type OpenAIFetchOptions = NonNullable<ClientOptions["fetchOptions"]>;

export function getRuntimeConfig(env: RuntimeEnv = process.env): RuntimeConfig {
  const model = env.OPENAI_MODEL?.trim() || DEFAULT_MODEL;
  const apiKey = env.OPENAI_API_KEY?.trim() || undefined;
  const proxyUrl = env.OPENAI_PROXY_URL?.trim() || undefined;
  const modelGroups = getStaticModelGroups(model);

  return {
    apiKey,
    model,
    modelOptions: flattenModelGroups(modelGroups),
    modelGroups,
    proxyUrl
  };
}

export function createOpenAIClient(config: RuntimeConfig): OpenAI {
  if (!config.apiKey) {
    throw new Error("OPENAI_API_KEY is not configured.");
  }

  const proxyOptions = createProxyOpenAIOptions(config.proxyUrl);

  return new OpenAI({
    apiKey: config.apiKey,
    ...proxyOptions
  });
}

export function createProxyOpenAIOptions(proxyUrl?: string): Pick<ClientOptions, "fetch" | "fetchOptions"> {
  if (!proxyUrl) {
    return {};
  }

  return {
    fetch: undiciFetch as unknown as OpenAIFetch,
    fetchOptions: {
      dispatcher: new ProxyAgent(proxyUrl)
    } as OpenAIFetchOptions
  };
}

export async function listAvailableModels({
  env = process.env,
  client
}: {
  env?: RuntimeEnv;
  client?: OpenAI;
}): Promise<string[]> {
  const config = getRuntimeConfig(env);
  const openai = client ?? createOpenAIClient(config);
  const models = await openai.models.list();

  return models.data.map((model) => model.id).sort((left, right) => left.localeCompare(right));
}

export async function runPrompt({
  mode,
  input,
  model,
  tone,
  env = process.env,
  client
}: RunPromptInput): Promise<RunPromptResult> {
  const config = getRuntimeConfig(env);
  const openai = client ?? createOpenAIClient(config);
  const prompt = getPromptMode(mode, {
    zhToEnTone: tone
  });
  const selectedModel = model?.trim() || config.model;

  const response = await openai.responses.create({
    model: selectedModel,
    instructions: prompt.instructions,
    input
  });

  const outputText = response.output_text?.trim();

  if (!outputText) {
    throw new Error("OpenAI response did not include text output.");
  }

  return {
    outputText,
    mode,
    model: selectedModel,
    requestId: response._request_id ?? null
  };
}
