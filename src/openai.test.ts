import { describe, expect, test } from "bun:test";
import { ProxyAgent, fetch as undiciFetch } from "undici";
import { createProxyOpenAIOptions, getRuntimeConfig } from "./openai";

describe("OpenAI runtime config", () => {
  test("reads proxy url from the environment config", () => {
    const config = getRuntimeConfig({
      OPENAI_API_KEY: "test-key",
      OPENAI_PROXY_URL: " http://127.0.0.1:7890 "
    });

    expect(config.proxyUrl).toBe("http://127.0.0.1:7890");
  });

  test("uses OpenAI SDK fetchOptions with undici ProxyAgent", () => {
    const options = createProxyOpenAIOptions("http://127.0.0.1:7890");
    const fetchOptions = options.fetchOptions as { dispatcher?: unknown };

    expect(options.fetch as unknown).toBe(undiciFetch);
    expect(fetchOptions.dispatcher).toBeInstanceOf(ProxyAgent);
  });

  test("does not add proxy options when proxy is unset", () => {
    expect(createProxyOpenAIOptions()).toEqual({});
  });
});
