import { describe, expect, test } from "bun:test";
import { createLoginAttemptLimiter } from "./auth";
import { createCallLogStore } from "./call-logs";
import { createApp } from "./index";

describe("Prompt Pocket API", () => {
  test("reports auth disabled by default", async () => {
    const app = createApp({
      env: {}
    });

    const response = await app.request("/auth/status");
    const body = await response.json();

    expect(response.status).toBe(200);
    expect(body).toEqual({
      enabled: false,
      authenticated: true
    });
  });

  test("reports health without exposing secrets", async () => {
    const app = createApp({
      env: {
        OPENAI_API_KEY: "test-key",
        OPENAI_MODEL: "test-model",
        OPENAI_PROXY_URL: "http://127.0.0.1:7890"
      }
    });

    const response = await app.request("/health");
    const body = await response.json();

    expect(response.status).toBe(200);
    expect(body).toMatchObject({
      ok: true,
      model: "test-model",
      hasApiKey: true,
      hasProxy: true
    });
    expect(body.models.slice(0, 2)).toEqual(["test-model", "gpt-5.5"]);
    expect(JSON.stringify(body)).not.toContain("test-key");
  });

  test("requires the app password when configured", async () => {
    const app = createApp({
      env: {
        APP_PASSWORD: "secret",
        OPENAI_API_KEY: "test-key"
      },
      runPromptHandler: async ({ mode, input, model }) => ({
        outputText: `processed:${input}`,
        mode,
        model,
        requestId: "req_auth"
      })
    });

    const statusResponse = await app.request("/auth/status");
    const statusBody = await statusResponse.json();

    expect(statusResponse.status).toBe(200);
    expect(statusBody).toEqual({
      enabled: true,
      authenticated: false
    });

    expect((await app.request("/health")).status).toBe(401);
    expect(
      (
        await app.request("/api/run", {
          method: "POST",
          headers: {
            "Content-Type": "application/json"
          },
          body: JSON.stringify({
            mode: "zh_to_en",
            input: "你好"
          })
        })
      ).status
    ).toBe(401);

    const failedLogin = await app.request("/auth/login", {
      method: "POST",
      headers: {
        "Content-Type": "application/json"
      },
      body: JSON.stringify({
        password: "wrong"
      })
    });

    expect(failedLogin.status).toBe(401);

    const loginResponse = await app.request("/auth/login", {
      method: "POST",
      headers: {
        "Content-Type": "application/json"
      },
      body: JSON.stringify({
        password: "secret"
      })
    });
    const loginBody = await loginResponse.json();
    const cookie = loginResponse.headers.get("set-cookie")?.split(";")[0] ?? "";

    expect(loginResponse.status).toBe(200);
    expect(loginBody).toEqual({
      enabled: true,
      authenticated: true
    });
    expect(cookie.startsWith("prompt_pocket_auth=")).toBe(true);

    const healthResponse = await app.request("/health", {
      headers: {
        Cookie: cookie
      }
    });

    expect(healthResponse.status).toBe(200);

    const runResponse = await app.request("/api/run", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Cookie: cookie
      },
      body: JSON.stringify({
        mode: "zh_to_en",
        input: "你好",
        model: "gpt-5.2"
      })
    });
    const runBody = await runResponse.json();

    expect(runResponse.status).toBe(200);
    expect(runBody.outputText).toBe("processed:你好");
  });

  test("rate limits failed app password attempts by IP", async () => {
    let now = 0;
    const loginAttemptLimiter = createLoginAttemptLimiter({
      now: () => now
    });
    const app = createApp({
      env: {
        APP_PASSWORD: "secret",
        OPENAI_API_KEY: "test-key"
      },
      loginAttemptLimiter
    });
    const headers = {
      "Content-Type": "application/json",
      "X-Forwarded-For": "203.0.113.10"
    };

    for (let index = 0; index < 10; index += 1) {
      const response = await app.request("/auth/login", {
        method: "POST",
        headers,
        body: JSON.stringify({
          password: "wrong"
        })
      });

      expect(response.status).toBe(401);
    }

    const limitedResponse = await app.request("/auth/login", {
      method: "POST",
      headers,
      body: JSON.stringify({
        password: "wrong"
      })
    });
    const limitedBody = await limitedResponse.json();

    expect(limitedResponse.status).toBe(429);
    expect(limitedResponse.headers.get("Retry-After")).toBe("600");
    expect(limitedBody.reason).toBe("rate_limited");

    const blockedCorrectPassword = await app.request("/auth/login", {
      method: "POST",
      headers,
      body: JSON.stringify({
        password: "secret"
      })
    });

    expect(blockedCorrectPassword.status).toBe(429);

    now = 10 * 60 * 1000;

    const bannedResponse = await app.request("/auth/login", {
      method: "POST",
      headers,
      body: JSON.stringify({
        password: "still-wrong"
      })
    });
    const bannedBody = await bannedResponse.json();

    expect(bannedResponse.status).toBe(429);
    expect(bannedResponse.headers.get("Retry-After")).toBe("3600");
    expect(bannedBody.reason).toBe("banned");

    const otherIpResponse = await app.request("/auth/login", {
      method: "POST",
      headers: {
        ...headers,
        "X-Forwarded-For": "203.0.113.11"
      },
      body: JSON.stringify({
        password: "secret"
      })
    });

    expect(otherIpResponse.status).toBe(200);
  });

  test("rejects requests when OPENAI_API_KEY is missing", async () => {
    const app = createApp({
      env: {}
    });

    const response = await app.request("/api/run", {
      method: "POST",
      headers: {
        "Content-Type": "application/json"
      },
      body: JSON.stringify({
        mode: "zh_to_en",
        input: "你好"
      })
    });
    const body = await response.json();

    expect(response.status).toBe(500);
    expect(body.error).toContain("OPENAI_API_KEY");
  });

  test("rejects invalid prompt modes", async () => {
    const app = createApp({
      env: {
        OPENAI_API_KEY: "test-key"
      }
    });

    const response = await app.request("/api/run", {
      method: "POST",
      headers: {
        "Content-Type": "application/json"
      },
      body: JSON.stringify({
        mode: "summarize",
        input: "hello"
      })
    });

    expect(response.status).toBe(422);
  });

  test("rejects empty input", async () => {
    const app = createApp({
      env: {
        OPENAI_API_KEY: "test-key"
      }
    });

    const response = await app.request("/api/run", {
      method: "POST",
      headers: {
        "Content-Type": "application/json"
      },
      body: JSON.stringify({
        mode: "polish_en",
        input: "   "
      })
    });

    expect(response.status).toBe(422);
  });

  test("rejects invalid zh-to-en tone", async () => {
    const app = createApp({
      env: {
        OPENAI_API_KEY: "test-key"
      }
    });

    const response = await app.request("/api/run", {
      method: "POST",
      headers: {
        "Content-Type": "application/json"
      },
      body: JSON.stringify({
        mode: "zh_to_en",
        input: "你好",
        tone: "formal"
      })
    });

    expect(response.status).toBe(422);
  });

  test("rejects invalid models", async () => {
    const app = createApp({
      env: {
        OPENAI_API_KEY: "test-key"
      }
    });

    const response = await app.request("/api/run", {
      method: "POST",
      headers: {
        "Content-Type": "application/json"
      },
      body: JSON.stringify({
        mode: "polish_en",
        input: "hello",
        model: "not a model"
      })
    });

    expect(response.status).toBe(422);
  });

  test("returns dynamic GPT-4 and GPT-5 model list only", async () => {
    const app = createApp({
      env: {
        OPENAI_API_KEY: "test-key"
      },
      listModelsHandler: async () => [
        "gpt-5.6",
        "gpt-4.2-mini",
        "gpt-image-2",
        "gpt-4o-transcribe",
        "o3"
      ]
    });

    const response = await app.request("/api/models");
    const body = await response.json();

    expect(response.status).toBe(200);
    expect(body.source).toBe("dynamic");
    expect(body.models).toContain("gpt-5.6");
    expect(body.models).toContain("gpt-4.2-mini");
    expect(body.models).not.toContain("gpt-image-2");
    expect(body.models).not.toContain("gpt-4o-transcribe");
    expect(body.models).not.toContain("o3");
  });

  test("returns a mocked OpenAI response", async () => {
    const app = createApp({
      env: {
        OPENAI_API_KEY: "test-key",
        OPENAI_MODEL: "test-model"
      },
      runPromptHandler: async ({ mode, input, model, tone }) => ({
        outputText: `processed:${mode}:${tone}:${input}`,
        mode,
        model,
        requestId: "req_test"
      })
    });

    const response = await app.request("/api/run", {
      method: "POST",
      headers: {
        "Content-Type": "application/json"
      },
      body: JSON.stringify({
        mode: "zh_to_en",
        input: "  你好  ",
        tone: "casual",
        model: "gpt-5.2"
      })
    });
    const body = await response.json();

    expect(response.status).toBe(200);
    expect(body).toEqual({
      outputText: "processed:zh_to_en:casual:你好",
      mode: "zh_to_en",
      model: "gpt-5.2",
      requestId: "req_test"
    });
  });

  test("records successful and failed run calls in memory", async () => {
    const callLogStore = createCallLogStore(2);
    const app = createApp({
      env: {
        OPENAI_API_KEY: "test-key"
      },
      callLogStore,
      runPromptHandler: async ({ mode, input, model }) => {
        if (input === "fail") {
          throw new Error("mock failure");
        }

        return {
          outputText: `processed:${input}`,
          mode,
          model,
          requestId: "req_success"
        };
      }
    });

    await app.request("/api/run", {
      method: "POST",
      headers: {
        "Content-Type": "application/json"
      },
      body: JSON.stringify({
        mode: "zh_to_en",
        input: "hello",
        tone: "casual",
        model: "gpt-5.2"
      })
    });
    await app.request("/api/run", {
      method: "POST",
      headers: {
        "Content-Type": "application/json"
      },
      body: JSON.stringify({
        mode: "zh_to_en",
        input: "fail",
        model: "gpt-5.2"
      })
    });

    const response = await app.request("/api/logs");
    const body = await response.json();

    expect(response.status).toBe(200);
    expect(body.limit).toBe(2);
    expect(body.logs).toHaveLength(2);
    expect(body.logs[0]).toMatchObject({
      status: "error",
      input: "fail",
      error: "mock failure"
    });
    expect(body.logs[1]).toMatchObject({
      status: "success",
      input: "hello",
      tone: "casual",
      outputText: "processed:hello",
      requestId: "req_success"
    });
  });
});
