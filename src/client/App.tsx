import { type FormEvent, useEffect, useMemo, useState } from "react";
import {
  ArrowRightLeft,
  ArrowLeft,
  Briefcase,
  CheckCircle2,
  Clipboard,
  Clock3,
  FileText,
  Languages,
  Loader2,
  LockKeyhole,
  LogIn,
  Mail,
  MessageCircle,
  PenLine,
  RefreshCw,
  Send,
  Settings2,
  Sparkles,
  SlidersHorizontal,
  WandSparkles
} from "lucide-react";
import { toast } from "sonner";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Separator } from "@/components/ui/separator";
import { Select } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { Textarea } from "@/components/ui/textarea";
import { cn } from "@/lib/utils";
import {
  DEFAULT_MODEL,
  getStaticModelGroups,
  type ModelCatalogGroup
} from "../model-catalog";

type PromptMode = "zh_to_en" | "en_to_zh" | "polish_en";
type ZhToEnTone = "casual" | "business" | "email";

type HealthState = {
  ok: boolean;
  model: string;
  models: string[];
  modelGroups: ModelCatalogGroup[];
  hasApiKey: boolean;
  hasProxy: boolean;
};

type AuthState = {
  enabled: boolean;
  authenticated: boolean;
};

type AuthResponse = AuthState & {
  error?: string;
};

type ModelListState = {
  model: string;
  models: string[];
  modelGroups: ModelCatalogGroup[];
  source: "static" | "dynamic";
  error?: string;
};

type CallLogEntry = {
  id: number;
  createdAt: string;
  durationMs: number;
  status: "success" | "error";
  mode: PromptMode;
  model: string;
  tone?: ZhToEnTone;
  input: string;
  outputText?: string;
  error?: string;
  requestId?: string | null;
};

type CallLogsState = {
  limit: number;
  logs: CallLogEntry[];
};

type ToolState = {
  input: string;
  output: string;
  error: string | null;
  loading: boolean;
};

type RunResponse = {
  outputText: string;
  mode: PromptMode;
  model: string;
  requestId?: string | null;
  error?: string;
};

const tools = [
  {
    mode: "zh_to_en",
    title: "中译英",
    description: "保留语气、格式和细节，输出自然英文。",
    placeholder: "粘贴中文内容，例如：\n这份方案整体不错，但还需要把风险和时间线写得更清楚。",
    outputPlaceholder: "英文译文会显示在这里。",
    actionLabel: "翻译成英文",
    icon: Languages,
    accent: "from-teal-500 to-cyan-500"
  },
  {
    mode: "en_to_zh",
    title: "英译中",
    description: "把英文转成清晰自然的简体中文。",
    placeholder: "Paste English text, for example:\nThe proposal is solid, but the risk section needs sharper wording.",
    outputPlaceholder: "中文译文会显示在这里。",
    actionLabel: "翻译成中文",
    icon: ArrowRightLeft,
    accent: "from-blue-500 to-indigo-500"
  },
  {
    mode: "polish_en",
    title: "英文润色",
    description: "修正语法，让英文更顺、更专业。",
    placeholder: "Paste English draft, for example:\nI think this part can be more clearly and easy to understand.",
    outputPlaceholder: "润色后的英文会显示在这里。",
    actionLabel: "润色英文",
    icon: PenLine,
    accent: "from-rose-500 to-orange-400"
  }
] as const satisfies Array<{
  mode: PromptMode;
  title: string;
  description: string;
  placeholder: string;
  outputPlaceholder: string;
  actionLabel: string;
  icon: typeof Languages;
  accent: string;
}>;

const zhToEnToneOptions = [
  {
    value: "casual",
    label: "口语化",
    description: "自然、轻松、日常表达",
    icon: MessageCircle
  },
  {
    value: "business",
    label: "工作用",
    description: "清晰、专业、适合工作",
    icon: Briefcase
  },
  {
    value: "email",
    label: "邮件用",
    description: "礼貌、友好、适合邮件",
    icon: Mail
  }
] as const satisfies Array<{
  value: ZhToEnTone;
  label: string;
  description: string;
  icon: typeof MessageCircle;
}>;

const zhToEnToneLabels: Record<ZhToEnTone, string> = {
  casual: "口语化",
  business: "工作用",
  email: "邮件用"
};

const initialToolState = tools.reduce(
  (state, tool) => {
    state[tool.mode] = {
      input: "",
      output: "",
      error: null,
      loading: false
    };
    return state;
  },
  {} as Record<PromptMode, ToolState>
);

function App() {
  const isLogsPage = window.location.pathname === "/logs";
  const [auth, setAuth] = useState<AuthState | null>(null);
  const [authError, setAuthError] = useState<string | null>(null);
  const [health, setHealth] = useState<HealthState | null>(null);
  const [healthError, setHealthError] = useState<string | null>(null);
  const [selectedModel, setSelectedModel] = useState<string>(DEFAULT_MODEL);
  const [modelGroups, setModelGroups] = useState<ModelCatalogGroup[]>(() => getStaticModelGroups(DEFAULT_MODEL));
  const [modelSource, setModelSource] = useState<"static" | "dynamic">("static");
  const [zhToEnTone, setZhToEnTone] = useState<ZhToEnTone>("casual");
  const [toolState, setToolState] = useState<Record<PromptMode, ToolState>>(() => initialToolState);

  useEffect(() => {
    let active = true;

    fetch("/auth/status")
      .then(async (response) => {
        if (!response.ok) {
          throw new Error("无法读取登录状态");
        }

        return (await response.json()) as AuthState;
      })
      .then((data) => {
        if (active) {
          setAuth(data);
          setAuthError(null);
        }
      })
      .catch((error) => {
        if (active) {
          setAuthError(error instanceof Error ? error.message : "无法读取登录状态");
        }
      });

    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    if (!auth?.authenticated) {
      setHealth(null);
      return;
    }

    let active = true;

    fetch("/health")
      .then(async (response) => {
        if (response.status === 401) {
          setAuth({
            enabled: true,
            authenticated: false
          });
          return null;
        }

        if (!response.ok) {
          throw new Error("无法读取服务状态");
        }

        return (await response.json()) as HealthState;
      })
      .then((data) => {
        if (active && data) {
          setHealth(data);
          setSelectedModel((current) => (current === DEFAULT_MODEL ? data.model : current));
          setModelGroups(data.modelGroups?.length ? data.modelGroups : getStaticModelGroups(data.model));
          setModelSource("static");
          setHealthError(null);
        }
      })
      .catch((error) => {
        if (active) {
          setHealthError(error instanceof Error ? error.message : "无法读取服务状态");
        }
      });

    return () => {
      active = false;
    };
  }, [auth?.authenticated]);

  useEffect(() => {
    if (!auth?.authenticated || !health?.hasApiKey) {
      return;
    }

    let active = true;

    fetch("/api/models")
      .then(async (response) => {
        if (!response.ok) {
          throw new Error("无法读取模型列表");
        }

        return (await response.json()) as ModelListState;
      })
      .then((data) => {
        if (active && data.modelGroups.length > 0) {
          setModelGroups(data.modelGroups);
          setModelSource(data.source);
        }
      })
      .catch(() => {
        if (active) {
          setModelSource("static");
        }
      });

    return () => {
      active = false;
    };
  }, [auth?.authenticated, health?.hasApiKey]);

  const canSubmit = useMemo(
    () => Boolean(auth?.authenticated && health?.hasApiKey),
    [auth?.authenticated, health?.hasApiKey]
  );

  function updateTool(mode: PromptMode, patch: Partial<ToolState>) {
    setToolState((current) => ({
      ...current,
      [mode]: {
        ...current[mode],
        ...patch
      }
    }));
  }

  async function runTool(mode: PromptMode) {
    const current = toolState[mode];
    const input = current.input.trim();

    if (!input) {
      updateTool(mode, {
        error: "先输入要处理的文本。",
        output: ""
      });
      return;
    }

    updateTool(mode, {
      loading: true,
      error: null
    });

    try {
      const response = await fetch("/api/run", {
        method: "POST",
        headers: {
          "Content-Type": "application/json"
        },
        body: JSON.stringify({
          mode,
          input,
          model: selectedModel || health?.model,
          ...(mode === "zh_to_en" ? { tone: zhToEnTone } : {})
        })
      });
      const data = (await response.json()) as RunResponse;

      if (response.status === 401) {
        setAuth({
          enabled: true,
          authenticated: false
        });
        throw new Error("登录已过期，请重新登录。");
      }

      if (!response.ok) {
        throw new Error(data.error || "请求失败");
      }

      updateTool(mode, {
        output: data.outputText,
        loading: false,
        error: null
      });
      toast.success("处理完成");
    } catch (error) {
      updateTool(mode, {
        loading: false,
        error: error instanceof Error ? error.message : "请求失败"
      });
    }
  }

  async function copyOutput(mode: PromptMode) {
    const output = toolState[mode].output;

    if (!output) {
      return;
    }

    try {
      await navigator.clipboard.writeText(output);
      toast.success("已复制到剪贴板");
    } catch {
      toast.error("复制失败");
    }
  }

  function handleAuthenticated(nextAuth: AuthState) {
    setAuth(nextAuth);
    setAuthError(null);
  }

  if (!auth) {
    return <AuthLoadingPage error={authError} />;
  }

  if (!auth.authenticated) {
    return <LoginPage statusError={authError} onAuthenticated={handleAuthenticated} />;
  }

  if (isLogsPage) {
    return (
      <LogsPage
        onUnauthorized={() =>
          setAuth({
            enabled: true,
            authenticated: false
          })
        }
      />
    );
  }

  return (
    <main className="min-h-screen bg-[radial-gradient(circle_at_top_left,_rgba(20,184,166,0.16),_transparent_32%),linear-gradient(180deg,_#f8fafc_0%,_#eef2f7_100%)] text-foreground">
      <div className="flex min-h-screen w-full flex-col gap-6 px-4 py-5 sm:px-6 lg:px-8 2xl:px-10">
        <header className="flex flex-col gap-4 rounded-lg border bg-white/78 px-5 py-4 shadow-soft backdrop-blur md:flex-row md:items-center md:justify-between">
          <div className="min-w-0">
            <div className="flex items-center gap-3">
              <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-slate-950 text-white shadow-sm">
                <Sparkles className="h-5 w-5" aria-hidden="true" />
              </div>
              <div className="min-w-0">
                <h1 className="truncate text-xl font-semibold tracking-normal text-slate-950 sm:text-2xl">
                  Prompt Pocket
                </h1>
                <p className="mt-1 text-sm text-muted-foreground">翻译、润色和语法修正，一屏完成。</p>
              </div>
            </div>
          </div>

          <div className="flex flex-col gap-3 md:items-end">
            <StatusBar health={health} healthError={healthError} />
            <div className="flex w-full flex-col gap-2 sm:flex-row md:w-auto">
              <Button asChild variant="outline" className="gap-2">
                <a href="/logs">
                  <FileText className="h-4 w-4" aria-hidden="true" />
                  调用日志
                </a>
              </Button>
              <ModelPicker
                defaultModel={health?.model ?? DEFAULT_MODEL}
                modelGroups={modelGroups}
                modelSource={modelSource}
                value={selectedModel}
                onChange={setSelectedModel}
              />
            </div>
          </div>
        </header>

        {!canSubmit ? (
          <Alert className="border-amber-200 bg-amber-50 text-amber-900">
            <Settings2 className="absolute left-4 top-4 h-4 w-4" aria-hidden="true" />
            <div className="pl-6">
              <AlertTitle>还没有配置 OpenAI API Key</AlertTitle>
              <AlertDescription>
                在本地环境里设置 OPENAI_API_KEY 后重启 API 服务。页面不会保存或展示密钥。
              </AlertDescription>
            </div>
          </Alert>
        ) : null}

        <section className="grid grid-cols-1 gap-4 xl:min-h-0 xl:flex-1 xl:grid-cols-3">
          {tools.map((tool) => (
            <ToolCard
              key={tool.mode}
              tool={tool}
              state={toolState[tool.mode]}
              disabled={!canSubmit}
              tone={tool.mode === "zh_to_en" ? zhToEnTone : undefined}
              onToneChange={tool.mode === "zh_to_en" ? setZhToEnTone : undefined}
              onInputChange={(input) => updateTool(tool.mode, { input, error: null })}
              onSubmit={() => runTool(tool.mode)}
              onCopy={() => copyOutput(tool.mode)}
            />
          ))}
        </section>
      </div>
    </main>
  );
}

function AuthLoadingPage({ error }: { error: string | null }) {
  return (
    <main className="flex min-h-screen items-center justify-center bg-[linear-gradient(180deg,_#f8fafc_0%,_#eef2f7_100%)] px-4 text-foreground">
      <Card className="w-full max-w-sm bg-white/86 backdrop-blur">
        <CardHeader className="items-center text-center">
          <div className="flex h-11 w-11 items-center justify-center rounded-lg bg-slate-950 text-white shadow-sm">
            <Sparkles className="h-5 w-5" aria-hidden="true" />
          </div>
          <CardTitle className="text-lg">Prompt Pocket</CardTitle>
          <CardDescription>正在检查访问状态。</CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          {error ? (
            <Alert variant="destructive">
              <AlertTitle>连接失败</AlertTitle>
              <AlertDescription>{error}</AlertDescription>
            </Alert>
          ) : (
            <div className="space-y-3">
              <Skeleton className="h-10 w-full" />
              <Skeleton className="h-10 w-full" />
            </div>
          )}
        </CardContent>
      </Card>
    </main>
  );
}

function LoginPage({
  statusError,
  onAuthenticated
}: {
  statusError: string | null;
  onAuthenticated: (auth: AuthState) => void;
}) {
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(statusError);
  const [loading, setLoading] = useState(false);

  async function submitLogin(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();

    if (!password) {
      setError("请输入访问密码。");
      return;
    }

    setLoading(true);
    setError(null);

    try {
      const response = await fetch("/auth/login", {
        method: "POST",
        headers: {
          "Content-Type": "application/json"
        },
        credentials: "same-origin",
        body: JSON.stringify({
          password
        })
      });
      const data = (await response.json()) as AuthResponse;

      if (!response.ok) {
        throw new Error(data.error || "登录失败");
      }

      onAuthenticated({
        enabled: data.enabled,
        authenticated: data.authenticated
      });
      toast.success("已解锁");
    } catch (loginError) {
      setError(loginError instanceof Error ? loginError.message : "登录失败");
    } finally {
      setLoading(false);
    }
  }

  return (
    <main className="flex min-h-screen items-center justify-center bg-[radial-gradient(circle_at_top_left,_rgba(20,184,166,0.16),_transparent_32%),linear-gradient(180deg,_#f8fafc_0%,_#eef2f7_100%)] px-4 text-foreground">
      <Card className="w-full max-w-sm bg-white/88 shadow-soft backdrop-blur">
        <CardHeader className="space-y-4">
          <div className="flex items-center gap-3">
            <div className="flex h-11 w-11 shrink-0 items-center justify-center rounded-lg bg-slate-950 text-white shadow-sm">
              <LockKeyhole className="h-5 w-5" aria-hidden="true" />
            </div>
            <div className="min-w-0">
              <CardTitle className="text-lg">访问 Prompt Pocket</CardTitle>
              <CardDescription className="mt-1">请输入部署环境配置的密码。</CardDescription>
            </div>
          </div>
        </CardHeader>
        <CardContent>
          <form className="space-y-4" onSubmit={submitLogin}>
            <label className="space-y-2">
              <span className="text-xs text-muted-foreground">访问密码</span>
              <input
                type="password"
                value={password}
                onChange={(event) => setPassword(event.target.value)}
                disabled={loading}
                autoFocus
                className="flex h-10 w-full rounded-md border border-input bg-background px-3 text-sm ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50"
              />
            </label>

            {error ? (
              <Alert variant="destructive">
                <AlertTitle>无法访问</AlertTitle>
                <AlertDescription>{error}</AlertDescription>
              </Alert>
            ) : null}

            <Button type="submit" className="w-full gap-2" disabled={loading}>
              {loading ? (
                <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" />
              ) : (
                <LogIn className="h-4 w-4" aria-hidden="true" />
              )}
              {loading ? "正在验证" : "进入"}
            </Button>
          </form>
        </CardContent>
      </Card>
    </main>
  );
}

function LogsPage({ onUnauthorized }: { onUnauthorized: () => void }) {
  const [logsState, setLogsState] = useState<CallLogsState | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  async function loadLogs() {
    setLoading(true);
    setError(null);

    try {
      const response = await fetch("/api/logs");

      if (response.status === 401) {
        onUnauthorized();
        throw new Error("登录已过期，请重新登录。");
      }

      if (!response.ok) {
        throw new Error("无法读取调用日志");
      }

      setLogsState((await response.json()) as CallLogsState);
    } catch (loadError) {
      setError(loadError instanceof Error ? loadError.message : "无法读取调用日志");
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void loadLogs();
  }, []);

  return (
    <main className="min-h-screen bg-[linear-gradient(180deg,_#f8fafc_0%,_#eef2f7_100%)] text-foreground">
      <div className="flex w-full flex-col gap-5 px-4 py-5 sm:px-6 lg:px-8 2xl:px-10">
        <header className="flex flex-col gap-4 rounded-lg border bg-white/82 px-5 py-4 shadow-soft backdrop-blur md:flex-row md:items-center md:justify-between">
          <div className="flex min-w-0 items-center gap-3">
            <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-slate-950 text-white shadow-sm">
              <FileText className="h-5 w-5" aria-hidden="true" />
            </div>
            <div className="min-w-0">
              <h1 className="truncate text-xl font-semibold tracking-normal text-slate-950 sm:text-2xl">
                调用日志
              </h1>
              <p className="mt-1 text-sm text-muted-foreground">
                内存中最近 {logsState?.limit ?? 2000} 条 `/api/run` 调用。
              </p>
            </div>
          </div>
          <div className="flex flex-col gap-2 sm:flex-row">
            <Button asChild variant="outline" className="gap-2">
              <a href="/">
                <ArrowLeft className="h-4 w-4" aria-hidden="true" />
                返回工具
              </a>
            </Button>
            <Button type="button" className="gap-2" onClick={() => void loadLogs()} disabled={loading}>
              <RefreshCw className={cn("h-4 w-4", loading && "animate-spin")} aria-hidden="true" />
              刷新
            </Button>
          </div>
        </header>

        {error ? (
          <Alert variant="destructive">
            <AlertTitle>读取失败</AlertTitle>
            <AlertDescription>{error}</AlertDescription>
          </Alert>
        ) : null}

        <Card className="overflow-hidden bg-white/86 backdrop-blur">
          <CardContent className="p-0">
            {loading && !logsState ? (
              <div className="space-y-3 p-5">
                <Skeleton className="h-16 w-full" />
                <Skeleton className="h-16 w-full" />
                <Skeleton className="h-16 w-full" />
              </div>
            ) : logsState?.logs.length ? (
              <div className="divide-y">
                {logsState.logs.map((log) => (
                  <LogRow key={log.id} log={log} />
                ))}
              </div>
            ) : (
              <div className="p-8 text-center text-sm text-muted-foreground">暂无调用日志。</div>
            )}
          </CardContent>
        </Card>
      </div>
    </main>
  );
}

function LogRow({ log }: { log: CallLogEntry }) {
  const output = log.status === "success" ? log.outputText : log.error;

  return (
    <article className="grid gap-3 p-4 md:grid-cols-[220px_minmax(0,1fr)]">
      <div className="space-y-2">
        <div className="flex flex-wrap items-center gap-2">
          <Badge variant={log.status === "success" ? "success" : "warning"}>
            {log.status === "success" ? "成功" : "失败"}
          </Badge>
          <Badge variant="secondary">{log.mode}</Badge>
          {log.tone ? <Badge variant="outline">{zhToEnToneLabels[log.tone]}</Badge> : null}
        </div>
        <div className="text-xs leading-5 text-muted-foreground">
          <div>{new Date(log.createdAt).toLocaleString()}</div>
          <div className="flex items-center gap-1">
            <Clock3 className="h-3.5 w-3.5" aria-hidden="true" />
            {log.durationMs} ms
          </div>
          <div className="break-all">{log.model}</div>
          {log.requestId ? <div className="break-all">{log.requestId}</div> : null}
        </div>
      </div>
      <div className="grid gap-3 lg:grid-cols-2">
        <LogText title="输入" text={log.input} />
        <LogText title={log.status === "success" ? "输出" : "错误"} text={output || ""} />
      </div>
    </article>
  );
}

function LogText({ title, text }: { title: string; text: string }) {
  return (
    <div className="min-w-0 rounded-md border bg-slate-50/80 p-3">
      <div className="mb-2 text-xs font-medium text-muted-foreground">{title}</div>
      <p className="max-h-44 overflow-auto whitespace-pre-wrap break-words text-sm leading-6 text-slate-900">
        {text || "-"}
      </p>
    </div>
  );
}

function ModelPicker({
  defaultModel,
  modelGroups,
  modelSource,
  value,
  onChange
}: {
  defaultModel: string;
  modelGroups: ModelCatalogGroup[];
  modelSource: "static" | "dynamic";
  value: string;
  onChange: (model: string) => void;
}) {
  return (
    <label className="flex w-full items-center gap-2 md:w-72">
      <SlidersHorizontal className="h-4 w-4 shrink-0 text-muted-foreground" aria-hidden="true" />
      <Select
        value={value || defaultModel}
        onChange={(event) => onChange(event.target.value)}
        aria-label="选择 OpenAI 模型"
        title={modelSource === "dynamic" ? "已合并 OpenAI API 返回的 GPT-4/GPT-5 模型" : "使用内置 GPT-4/GPT-5 模型列表"}
      >
        {modelGroups.map((group) => (
          <optgroup key={group.label} label={group.label}>
            {group.models.map((model) => (
              <option key={model} value={model}>
                {model}
                {model === defaultModel ? " · 默认" : ""}
              </option>
            ))}
          </optgroup>
        ))}
      </Select>
    </label>
  );
}

function StatusBar({
  health,
  healthError
}: {
  health: HealthState | null;
  healthError: string | null;
}) {
  if (healthError) {
    return (
      <Badge variant="warning" className="h-8 justify-center">
        API 未连接
      </Badge>
    );
  }

  if (!health) {
    return (
      <div className="flex items-center gap-2">
        <Skeleton className="h-8 w-24" />
        <Skeleton className="h-8 w-20" />
      </div>
    );
  }

  return (
    <div className="flex flex-wrap items-center gap-2">
      <Badge variant={health.hasApiKey ? "success" : "warning"} className="h-8 gap-1.5">
        <CheckCircle2 className="h-3.5 w-3.5" aria-hidden="true" />
        {health.hasApiKey ? "API Ready" : "缺少 Key"}
      </Badge>
      <Badge variant="secondary" className="h-8">
        {health.model}
      </Badge>
      <Badge variant={health.hasProxy ? "success" : "outline"} className="h-8">
        {health.hasProxy ? "Proxy On" : "Direct"}
      </Badge>
    </div>
  );
}

function ToolCard({
  tool,
  state,
  disabled,
  tone,
  onToneChange,
  onInputChange,
  onSubmit,
  onCopy
}: {
  tool: (typeof tools)[number];
  state: ToolState;
  disabled: boolean;
  tone?: ZhToEnTone;
  onToneChange?: (tone: ZhToEnTone) => void;
  onInputChange: (input: string) => void;
  onSubmit: () => void;
  onCopy: () => void;
}) {
  const Icon = tool.icon;
  const hasOutput = state.output.length > 0;

  return (
    <Card className="flex min-h-[680px] flex-col overflow-hidden bg-white/86 backdrop-blur xl:h-full">
      <CardHeader className="space-y-4">
        <div className="flex items-start justify-between gap-3">
          <div className="flex min-w-0 items-center gap-3">
            <div
              className={cn(
                "flex h-11 w-11 shrink-0 items-center justify-center rounded-lg bg-gradient-to-br text-white shadow-sm",
                tool.accent
              )}
            >
              <Icon className="h-5 w-5" aria-hidden="true" />
            </div>
            {tone && onToneChange ? (
              <ToneSelector value={tone} disabled={state.loading} onChange={onToneChange} />
            ) : null}
          </div>
          <Badge variant="outline" className="shrink-0">
            预设 prompt
          </Badge>
        </div>
        <div>
          <CardTitle className="text-lg">{tool.title}</CardTitle>
          <CardDescription className="mt-2 leading-6">{tool.description}</CardDescription>
        </div>
      </CardHeader>

      <CardContent className="flex flex-1 flex-col gap-4">
        <div className="space-y-2">
          <div className="flex items-center justify-between gap-3 text-xs text-muted-foreground">
            <span>输入</span>
            <span>{state.input.trim().length} 字符</span>
          </div>
          <Textarea
            value={state.input}
            onChange={(event) => onInputChange(event.target.value)}
            placeholder={tool.placeholder}
            disabled={state.loading}
            className="min-h-[190px]"
          />
        </div>

        {state.error ? (
          <Alert variant="destructive">
            <AlertTitle>处理失败</AlertTitle>
            <AlertDescription>{state.error}</AlertDescription>
          </Alert>
        ) : null}

        <div className="flex items-center justify-between gap-3">
          <Button
            type="button"
            className="min-w-0 flex-1 gap-2"
            disabled={disabled || state.loading}
            onClick={onSubmit}
          >
            {state.loading ? (
              <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" />
            ) : (
              <Send className="h-4 w-4" aria-hidden="true" />
            )}
            <span className="truncate">{state.loading ? "处理中" : tool.actionLabel}</span>
          </Button>
          <Button
            type="button"
            variant="outline"
            size="icon"
            disabled={!hasOutput || state.loading}
            onClick={onCopy}
            aria-label={`复制${tool.title}结果`}
            title="复制结果"
          >
            <Clipboard className="h-4 w-4" aria-hidden="true" />
          </Button>
        </div>

        <Separator />

        <div className="flex flex-1 flex-col gap-2">
          <div className="flex items-center justify-between gap-3 text-xs text-muted-foreground">
            <span>结果</span>
            <WandSparkles className="h-3.5 w-3.5" aria-hidden="true" />
          </div>
          <div className="min-h-[190px] flex-1 rounded-md border bg-slate-50/80 p-3 text-sm leading-6 text-slate-900">
            {state.loading ? (
              <div className="space-y-3 pt-1">
                <Skeleton className="h-4 w-11/12" />
                <Skeleton className="h-4 w-10/12" />
                <Skeleton className="h-4 w-8/12" />
              </div>
            ) : hasOutput ? (
              <p className="whitespace-pre-wrap break-words">{state.output}</p>
            ) : (
              <p className="text-muted-foreground">{tool.outputPlaceholder}</p>
            )}
          </div>
        </div>
      </CardContent>
    </Card>
  );
}

function ToneSelector({
  value,
  disabled,
  onChange
}: {
  value: ZhToEnTone;
  disabled: boolean;
  onChange: (tone: ZhToEnTone) => void;
}) {
  return (
    <fieldset className="min-w-0">
      <legend className="sr-only">风格</legend>
      <div className="grid h-9 w-[248px] grid-cols-3 rounded-md border bg-slate-100/80 p-1">
        {zhToEnToneOptions.map((option) => {
          const Icon = option.icon;
          const checked = option.value === value;

          return (
            <label
              key={option.value}
              className={cn(
                "flex min-w-0 cursor-pointer items-center justify-center gap-1.5 rounded-[4px] px-2 text-sm font-medium text-muted-foreground transition-colors",
                checked && "bg-white text-slate-950 shadow-sm",
                disabled && "cursor-not-allowed opacity-60"
              )}
              title={option.description}
            >
              <input
                type="radio"
                name="zh-to-en-tone"
                value={option.value}
                checked={checked}
                disabled={disabled}
                onChange={() => onChange(option.value)}
                className="sr-only"
              />
              <Icon className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
              <span className="truncate">
                {option.label}
              </span>
            </label>
          );
        })}
      </div>
    </fieldset>
  );
}

export { App };
