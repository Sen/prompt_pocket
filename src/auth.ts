import type { Context, MiddlewareHandler } from "hono";
import { deleteCookie, getSignedCookie, setSignedCookie } from "hono/cookie";

const AUTH_COOKIE_NAME = "prompt_pocket_auth";
const AUTH_COOKIE_VALUE = "authenticated";
const AUTH_COOKIE_MAX_AGE_SECONDS = 60 * 60 * 24 * 14;
const LOGIN_ATTEMPT_WINDOW_MS = 10 * 60 * 1000;
const LOGIN_ATTEMPT_LIMIT = 10;
const LOGIN_BAN_MS = 60 * 60 * 1000;

export type AuthEnv = Record<string, string | undefined>;

export type AuthStatus = {
  enabled: boolean;
  authenticated: boolean;
};

export type LoginAttemptBlock = {
  blocked: true;
  reason: "rate_limited" | "banned";
  retryAfterSeconds: number;
  message: string;
};

export type LoginAttemptResult =
  | {
      blocked: false;
    }
  | LoginAttemptBlock;

export type LoginAttemptLimiter = {
  getBlock(ip: string): LoginAttemptResult;
  recordFailure(ip: string): LoginAttemptResult;
  recordSuccess(ip: string): void;
};

type LoginAttemptState = {
  windowStartedAt: number;
  failures: number;
  cooldownUntil?: number;
  escalationExpiresAt?: number;
  cooldownCount: number;
  bannedUntil?: number;
};

function getAppPassword(env: AuthEnv): string | undefined {
  return env.APP_PASSWORD?.trim() || undefined;
}

function getCookieOptions(env: AuthEnv) {
  return {
    httpOnly: true,
    sameSite: "Lax" as const,
    secure: env.NODE_ENV === "production",
    path: "/",
    maxAge: AUTH_COOKIE_MAX_AGE_SECONDS
  };
}

export function isAuthEnabled(env: AuthEnv): boolean {
  return Boolean(getAppPassword(env));
}

export async function getAuthStatus(c: Context, env: AuthEnv): Promise<AuthStatus> {
  const password = getAppPassword(env);

  if (!password) {
    return {
      enabled: false,
      authenticated: true
    };
  }

  const cookieValue = await getSignedCookie(c, password, AUTH_COOKIE_NAME);

  return {
    enabled: true,
    authenticated: cookieValue === AUTH_COOKIE_VALUE
  };
}

export async function setAuthCookie(c: Context, env: AuthEnv): Promise<void> {
  const password = getAppPassword(env);

  if (!password) {
    return;
  }

  await setSignedCookie(c, AUTH_COOKIE_NAME, AUTH_COOKIE_VALUE, password, getCookieOptions(env));
}

export function clearAuthCookie(c: Context): void {
  deleteCookie(c, AUTH_COOKIE_NAME, {
    path: "/"
  });
}

export function verifyAppPassword(env: AuthEnv, password: unknown): boolean {
  const appPassword = getAppPassword(env);

  if (!appPassword) {
    return true;
  }

  return typeof password === "string" && password === appPassword;
}

function getRetryAfterSeconds(until: number, now: number): number {
  return Math.max(1, Math.ceil((until - now) / 1000));
}

function createBlockedResult(
  reason: LoginAttemptBlock["reason"],
  retryAfterSeconds: number
): LoginAttemptBlock {
  return {
    blocked: true,
    reason,
    retryAfterSeconds,
    message:
      reason === "banned"
        ? "该 IP 已被临时封禁，请 1 小时后再试。"
        : "密码错误次数过多，请到下个 10 分钟窗口再试。"
  };
}

export function createLoginAttemptLimiter({
  now = Date.now
}: {
  now?: () => number;
} = {}): LoginAttemptLimiter {
  const states = new Map<string, LoginAttemptState>();

  function getState(ip: string): LoginAttemptState {
    const existing = states.get(ip);

    if (existing) {
      return existing;
    }

    const state = {
      windowStartedAt: now(),
      failures: 0,
      cooldownCount: 0
    };
    states.set(ip, state);

    return state;
  }

  function normalizeState(ip: string): LoginAttemptResult {
    const state = states.get(ip);
    const currentTime = now();

    if (!state) {
      return {
        blocked: false
      };
    }

    if (state.bannedUntil) {
      if (state.bannedUntil > currentTime) {
        return createBlockedResult("banned", getRetryAfterSeconds(state.bannedUntil, currentTime));
      }

      states.delete(ip);
      return {
        blocked: false
      };
    }

    if (state.cooldownUntil) {
      if (state.cooldownUntil > currentTime) {
        return createBlockedResult("rate_limited", getRetryAfterSeconds(state.cooldownUntil, currentTime));
      }

      state.cooldownUntil = undefined;
      state.windowStartedAt = currentTime;
      state.failures = 0;
    }

    if (state.escalationExpiresAt && state.escalationExpiresAt <= currentTime) {
      state.cooldownCount = 0;
      state.escalationExpiresAt = undefined;
    }

    if (currentTime - state.windowStartedAt >= LOGIN_ATTEMPT_WINDOW_MS) {
      state.windowStartedAt = currentTime;
      state.failures = 0;
    }

    return {
      blocked: false
    };
  }

  return {
    getBlock(ip) {
      return normalizeState(ip);
    },
    recordFailure(ip) {
      const block = normalizeState(ip);

      if (block.blocked) {
        return block;
      }

      const state = getState(ip);
      const currentTime = now();

      if (state.cooldownCount > 0 && (!state.escalationExpiresAt || state.escalationExpiresAt > currentTime)) {
        state.bannedUntil = currentTime + LOGIN_BAN_MS;
        state.cooldownUntil = undefined;
        state.escalationExpiresAt = undefined;
        state.failures = 0;

        return createBlockedResult("banned", getRetryAfterSeconds(state.bannedUntil, currentTime));
      }

      state.failures += 1;

      if (state.failures <= LOGIN_ATTEMPT_LIMIT) {
        return {
          blocked: false
        };
      }

      state.cooldownUntil = state.windowStartedAt + LOGIN_ATTEMPT_WINDOW_MS;
      state.escalationExpiresAt = state.cooldownUntil + LOGIN_ATTEMPT_WINDOW_MS;
      state.cooldownCount += 1;

      return createBlockedResult("rate_limited", getRetryAfterSeconds(state.cooldownUntil, currentTime));
    },
    recordSuccess(ip) {
      states.delete(ip);
    }
  };
}

export function createAuthMiddleware(env: AuthEnv): MiddlewareHandler {
  return async (c, next) => {
    const status = await getAuthStatus(c, env);

    if (status.authenticated) {
      await next();
      return;
    }

    return c.json(
      {
        error: "Unauthorized"
      },
      401
    );
  };
}
