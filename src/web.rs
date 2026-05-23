use std::{sync::Arc, time::Instant};

use askama::Template;
use axum::{
    body::Bytes,
    extract::{Form, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;
use tower_http::services::ServeDir;

use crate::{
    auth::{
        clear_auth_cookie_header, get_auth_status, get_client_ip, set_auth_cookie_header,
        verify_app_password, AuthStatus, LoginAttemptBlock, LoginAttemptLimiter,
        LoginAttemptResult,
    },
    config::Config,
    logs::{CallLogCreateInput, CallLogEntry, CallLogStatus, LogError, LogStore},
    models::{
        flatten_model_groups, is_valid_model_id, merge_model_groups, static_model_groups,
        ModelCatalogGroup,
    },
    openai::{OpenAiError, OpenAiService, RealOpenAiService, RunPromptInput, RunPromptResult},
    prompts::{PromptMode, ZhToEnTone, DEFAULT_ZH_TO_EN_TONE},
};

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub login_limiter: LoginAttemptLimiter,
    pub logs: LogStore,
    pub openai: Arc<dyn OpenAiService>,
}

#[derive(Debug, Error)]
pub enum AppStateError {
    #[error("{0}")]
    Logs(#[from] LogError),
    #[error("{0}")]
    OpenAi(#[from] OpenAiError),
}

impl AppState {
    pub async fn new(config: Config) -> Result<Self, AppStateError> {
        let logs = LogStore::connect(&config.db_path, config.log_limit).await?;
        let openai = Arc::new(RealOpenAiService::new(config.clone())?);

        Ok(Self::with_parts(config, logs, openai))
    }

    pub fn with_parts(config: Config, logs: LogStore, openai: Arc<dyn OpenAiService>) -> Self {
        Self {
            config: Arc::new(config),
            login_limiter: LoginAttemptLimiter::default(),
            logs,
            openai,
        }
    }
}

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(index_page))
        .route("/login", get(login_page).post(login_form))
        .route("/logout", post(logout_form))
        .route("/logs", get(logs_page))
        .route("/ui/run", post(ui_run))
        .route("/ui/models", get(ui_models))
        .route("/ui/logs", get(ui_logs))
        .route("/health", get(health))
        .route("/api/run", post(api_run))
        .route("/api/models", get(api_models))
        .route("/api/logs", get(api_logs))
        .route("/auth/status", get(auth_status))
        .route("/auth/login", post(auth_login))
        .route("/auth/logout", post(auth_logout))
        .nest_service("/static", ServeDir::new("static"))
        .with_state(state)
}

#[derive(Clone)]
struct HealthView {
    model: String,
    has_api_key: bool,
    has_proxy: bool,
}

#[derive(Clone)]
struct ToolView {
    mode: &'static str,
    title: &'static str,
    description: &'static str,
    placeholder: &'static str,
    output_placeholder: &'static str,
    action_label: &'static str,
    icon: &'static str,
    accent_class: &'static str,
    show_tone: bool,
}

#[derive(Clone)]
struct ToneOptionView {
    value: &'static str,
    label: &'static str,
    description: &'static str,
}

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate {
    health: HealthView,
    can_submit: bool,
    model_picker_html: String,
    tools: Vec<ToolView>,
    tone_options: Vec<ToneOptionView>,
}

#[derive(Template)]
#[template(path = "login.html")]
struct LoginTemplate {
    error: String,
    has_error: bool,
}

#[derive(Template)]
#[template(path = "logs.html")]
struct LogsTemplate {
    logs_list_html: String,
}

#[derive(Clone, Template)]
#[template(path = "partials/model_picker.html")]
struct ModelPickerTemplate {
    source_label: String,
    groups: Vec<ModelGroupView>,
}

#[derive(Clone)]
struct ModelGroupView {
    label: String,
    models: Vec<ModelOptionView>,
}

#[derive(Clone)]
struct ModelOptionView {
    value: String,
    selected: bool,
    is_default: bool,
}

#[derive(Template)]
#[template(path = "partials/run_result.html")]
struct RunResultTemplate {
    mode: String,
    success: bool,
    has_text: bool,
    text: String,
    error: String,
    request_id: String,
    has_request_id: bool,
    placeholder: String,
}

#[derive(Clone, Template)]
#[template(path = "partials/logs_list.html")]
struct LogsListTemplate {
    limit: i64,
    logs: Vec<LogEntryView>,
    has_logs: bool,
}

#[derive(Clone)]
struct LogEntryView {
    status_label: String,
    status_class: String,
    mode: String,
    tone_label: String,
    has_tone: bool,
    created_at: String,
    duration_ms: i64,
    model: String,
    request_id: String,
    has_request_id: bool,
    input: String,
    output_label: String,
    output_text: String,
}

#[derive(Deserialize)]
struct LoginForm {
    password: String,
}

#[derive(Deserialize)]
struct RunForm {
    mode: String,
    input: String,
    model: Option<String>,
    tone: Option<String>,
}

#[derive(Deserialize)]
struct LoginBody {
    password: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HealthPayload {
    ok: bool,
    model: String,
    models: Vec<String>,
    model_groups: Vec<ModelCatalogGroup>,
    has_api_key: bool,
    has_proxy: bool,
    modes: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelListPayload {
    model: String,
    models: Vec<String>,
    model_groups: Vec<ModelCatalogGroup>,
    source: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Serialize)]
struct LogsPayload {
    limit: i64,
    logs: Vec<CallLogEntry>,
}

#[derive(Serialize)]
struct LogoutPayload {
    ok: bool,
}

struct ValidRunRequest {
    mode: PromptMode,
    input: String,
    model: String,
    tone: Option<ZhToEnTone>,
}

struct RunFailure {
    status: StatusCode,
    message: String,
}

async fn index_page(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if !get_auth_status(&headers, &state.config).authenticated {
        return Redirect::to("/login").into_response();
    }

    let model_picker_html = render_template_string(static_model_picker(&state.config));
    let template = IndexTemplate {
        health: HealthView {
            model: state.config.model.clone(),
            has_api_key: state.config.has_api_key(),
            has_proxy: state.config.has_proxy(),
        },
        can_submit: state.config.has_api_key(),
        model_picker_html,
        tools: tool_views(),
        tone_options: tone_options(),
    };

    render_html(template)
}

async fn login_page(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if get_auth_status(&headers, &state.config).authenticated {
        return Redirect::to("/").into_response();
    }

    render_html(LoginTemplate {
        error: String::new(),
        has_error: false,
    })
}

async fn login_form(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<LoginForm>,
) -> Response {
    let client_ip = get_client_ip(&headers);

    if let LoginAttemptResult::Blocked(block) = state.login_limiter.get_block(&client_ip) {
        return login_error_response(block.message, Some(block.retry_after_seconds));
    }

    if !verify_app_password(&state.config, Some(&form.password)) {
        if let LoginAttemptResult::Blocked(block) = state.login_limiter.record_failure(&client_ip) {
            return login_error_response(block.message, Some(block.retry_after_seconds));
        }

        return login_error_response("Invalid password.".to_string(), None);
    }

    state.login_limiter.record_success(&client_ip);

    let mut response = Redirect::to("/").into_response();
    if let Some(cookie) = set_auth_cookie_header(&state.config) {
        response.headers_mut().insert(header::SET_COOKIE, cookie);
    }

    response
}

async fn logout_form(State(state): State<AppState>) -> Response {
    let mut response = Redirect::to("/login").into_response();
    response
        .headers_mut()
        .insert(header::SET_COOKIE, clear_auth_cookie_header(&state.config));

    response
}

async fn logs_page(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if !get_auth_status(&headers, &state.config).authenticated {
        return Redirect::to("/login").into_response();
    }

    match logs_list_template(&state).await {
        Ok(logs_list) => render_html(LogsTemplate {
            logs_list_html: render_template_string(logs_list),
        }),
        Err(error) => error_response(StatusCode::INTERNAL_SERVER_ERROR, error),
    }
}

async fn ui_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<RunForm>,
) -> Response {
    if !get_auth_status(&headers, &state.config).authenticated {
        return render_html(run_error_template(&form.mode, "登录已过期，请重新登录。"));
    }

    let mode_for_target = form.mode.clone();
    let request = match valid_run_request_from_form(&state.config, form) {
        Ok(request) => request,
        Err(message) => return render_html(run_error_template(&mode_for_target, &message)),
    };

    match run_and_record(&state, request).await {
        Ok(result) => render_html(RunResultTemplate {
            mode: mode_for_target,
            success: true,
            has_text: true,
            text: result.output_text,
            error: String::new(),
            request_id: result.request_id.clone().unwrap_or_default(),
            has_request_id: result.request_id.is_some(),
            placeholder: String::new(),
        }),
        Err(error) => render_html(run_error_template(&mode_for_target, &error.message)),
    }
}

async fn ui_models(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if !get_auth_status(&headers, &state.config).authenticated {
        return error_response(StatusCode::UNAUTHORIZED, "Unauthorized");
    }

    let payload = model_payload(&state).await;
    let template = ModelPickerTemplate {
        source_label: if payload.source == "dynamic" {
            "动态模型".to_string()
        } else {
            "内置模型".to_string()
        },
        groups: model_group_views(&payload.model, &payload.model, payload.model_groups),
    };

    render_html(template)
}

async fn ui_logs(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if !get_auth_status(&headers, &state.config).authenticated {
        return error_response(StatusCode::UNAUTHORIZED, "Unauthorized");
    }

    match logs_list_template(&state).await {
        Ok(template) => render_html(template),
        Err(error) => error_response(StatusCode::INTERNAL_SERVER_ERROR, error),
    }
}

async fn health(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(response) = require_json_auth(&state, &headers) {
        return response;
    }

    Json(health_payload(&state.config)).into_response()
}

async fn api_models(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(response) = require_json_auth(&state, &headers) {
        return response;
    }

    Json(model_payload(&state).await).into_response()
}

async fn api_logs(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(response) = require_json_auth(&state, &headers) {
        return response;
    }

    match state.logs.list().await {
        Ok(logs) => Json(LogsPayload {
            limit: state.logs.limit(),
            logs,
        })
        .into_response(),
        Err(error) => json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            call_log_error_message(error),
        ),
    }
}

async fn api_run(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> Response {
    if let Some(response) = require_json_auth(&state, &headers) {
        return response;
    }

    let request = match valid_run_request_from_json(&state.config, &body) {
        Ok(request) => request,
        Err(error) => return json_error(error.status, error.message),
    };

    match run_and_record(&state, request).await {
        Ok(result) => Json(result).into_response(),
        Err(error) => json_error(error.status, error.message),
    }
}

async fn auth_status(State(state): State<AppState>, headers: HeaderMap) -> Response {
    Json(get_auth_status(&headers, &state.config)).into_response()
}

async fn auth_login(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> Response {
    let client_ip = get_client_ip(&headers);

    if let LoginAttemptResult::Blocked(block) = state.login_limiter.get_block(&client_ip) {
        return json_login_block(block);
    }

    let body = match serde_json::from_slice::<LoginBody>(&body) {
        Ok(body) => body,
        Err(_) => return json_error(StatusCode::BAD_REQUEST, "Request body must be valid JSON."),
    };

    if !verify_app_password(&state.config, body.password.as_deref()) {
        if let LoginAttemptResult::Blocked(block) = state.login_limiter.record_failure(&client_ip) {
            return json_login_block(block);
        }

        return json_error(StatusCode::UNAUTHORIZED, "Invalid password.");
    }

    state.login_limiter.record_success(&client_ip);

    let status = AuthStatus {
        enabled: state.config.auth_enabled(),
        authenticated: true,
    };
    let mut response = Json(status).into_response();

    if let Some(cookie) = set_auth_cookie_header(&state.config) {
        response.headers_mut().insert(header::SET_COOKIE, cookie);
    }

    response
}

async fn auth_logout(State(state): State<AppState>) -> Response {
    let mut response = Json(LogoutPayload { ok: true }).into_response();
    response
        .headers_mut()
        .insert(header::SET_COOKIE, clear_auth_cookie_header(&state.config));

    response
}

async fn model_payload(state: &AppState) -> ModelListPayload {
    if !state.config.has_api_key() {
        let groups = static_model_groups(&state.config.model);
        return ModelListPayload {
            model: state.config.model.clone(),
            models: flatten_model_groups(&groups),
            model_groups: groups,
            source: "static",
            error: None,
        };
    }

    match state.openai.list_models().await {
        Ok(dynamic_models) => {
            let groups = merge_model_groups(&state.config.model, &dynamic_models);

            ModelListPayload {
                model: state.config.model.clone(),
                models: flatten_model_groups(&groups),
                model_groups: groups,
                source: "dynamic",
                error: None,
            }
        }
        Err(error) => {
            let groups = static_model_groups(&state.config.model);

            ModelListPayload {
                model: state.config.model.clone(),
                models: flatten_model_groups(&groups),
                model_groups: groups,
                source: "static",
                error: Some(error.to_string()),
            }
        }
    }
}

async fn run_and_record(
    state: &AppState,
    request: ValidRunRequest,
) -> Result<RunPromptResult, RunFailure> {
    if !state.config.has_api_key() {
        return Err(RunFailure {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: "OPENAI_API_KEY is not configured.".to_string(),
        });
    }

    let started = Instant::now();
    let result = state
        .openai
        .run_prompt(RunPromptInput {
            mode: request.mode,
            input: request.input.clone(),
            model: request.model.clone(),
            tone: request.tone,
        })
        .await;
    let duration_ms = started.elapsed().as_millis().try_into().unwrap_or(i64::MAX);

    match result {
        Ok(result) => {
            if let Err(error) = state
                .logs
                .add(CallLogCreateInput {
                    duration_ms,
                    status: CallLogStatus::Success,
                    mode: request.mode,
                    model: request.model,
                    tone: request.tone,
                    input: request.input,
                    output_text: Some(result.output_text.clone()),
                    error: None,
                    request_id: result.request_id.clone(),
                })
                .await
            {
                return Err(RunFailure {
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                    message: call_log_error_message(error),
                });
            }

            Ok(result)
        }
        Err(error) => {
            let message = error.to_string();

            if let Err(log_error) = state
                .logs
                .add(CallLogCreateInput {
                    duration_ms,
                    status: CallLogStatus::Error,
                    mode: request.mode,
                    model: request.model,
                    tone: request.tone,
                    input: request.input,
                    output_text: None,
                    error: Some(message.clone()),
                    request_id: None,
                })
                .await
            {
                return Err(RunFailure {
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                    message: call_log_error_message(log_error),
                });
            }

            Err(RunFailure {
                status: StatusCode::BAD_GATEWAY,
                message,
            })
        }
    }
}

fn valid_run_request_from_json(
    config: &Config,
    body: &[u8],
) -> Result<ValidRunRequest, RunFailure> {
    let value = serde_json::from_slice::<Value>(body).map_err(|_| RunFailure {
        status: StatusCode::BAD_REQUEST,
        message: "Request body must be valid JSON.".to_string(),
    })?;

    let mode = string_field(&value, "mode")
        .and_then(|mode| mode.parse::<PromptMode>().ok())
        .ok_or_else(|| RunFailure {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            message: "Invalid prompt mode.".to_string(),
        })?;
    let input = string_field(&value, "input")
        .map(str::trim)
        .filter(|input| !input.is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| RunFailure {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            message: "Input text is required.".to_string(),
        })?;
    let tone = parse_tone(mode, value.get("tone").and_then(Value::as_str)).map_err(|message| {
        RunFailure {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            message,
        }
    })?;
    let model = string_field(&value, "model")
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .unwrap_or(&config.model)
        .to_string();

    if !is_valid_model_id(&model) {
        return Err(RunFailure {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            message: "Invalid model.".to_string(),
        });
    }

    Ok(ValidRunRequest {
        mode,
        input,
        model,
        tone,
    })
}

fn valid_run_request_from_form(config: &Config, form: RunForm) -> Result<ValidRunRequest, String> {
    let mode = form
        .mode
        .parse::<PromptMode>()
        .map_err(|_| "Invalid prompt mode.".to_string())?;
    let input = form.input.trim().to_string();

    if input.is_empty() {
        return Err("先输入要处理的文本。".to_string());
    }

    let tone = parse_tone(mode, form.tone.as_deref())?;
    let model = form
        .model
        .as_deref()
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .unwrap_or(&config.model)
        .to_string();

    if !is_valid_model_id(&model) {
        return Err("Invalid model.".to_string());
    }

    Ok(ValidRunRequest {
        mode,
        input,
        model,
        tone,
    })
}

fn parse_tone(mode: PromptMode, value: Option<&str>) -> Result<Option<ZhToEnTone>, String> {
    if mode != PromptMode::ZhToEn {
        return Ok(None);
    }

    match value {
        Some(value) => value
            .parse::<ZhToEnTone>()
            .map(Some)
            .map_err(|_| "Invalid translation tone.".to_string()),
        None => Ok(Some(DEFAULT_ZH_TO_EN_TONE)),
    }
}

fn string_field<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn health_payload(config: &Config) -> HealthPayload {
    let groups = static_model_groups(&config.model);

    HealthPayload {
        ok: true,
        model: config.model.clone(),
        models: flatten_model_groups(&groups),
        model_groups: groups,
        has_api_key: config.has_api_key(),
        has_proxy: config.has_proxy(),
        modes: PromptMode::ALL
            .iter()
            .map(|mode| mode.as_str().to_string())
            .collect(),
    }
}

async fn logs_list_template(state: &AppState) -> Result<LogsListTemplate, String> {
    let logs = state
        .logs
        .list()
        .await
        .map_err(call_log_error_message)?
        .into_iter()
        .map(log_view)
        .collect::<Vec<_>>();

    Ok(LogsListTemplate {
        limit: state.logs.limit(),
        has_logs: !logs.is_empty(),
        logs,
    })
}

fn log_view(log: CallLogEntry) -> LogEntryView {
    let success = log.status == CallLogStatus::Success;
    let tone_label = log
        .tone
        .map(|tone| tone.label().to_string())
        .unwrap_or_default();
    let request_id = log.request_id.unwrap_or_default();
    let output_text = if success {
        log.output_text.unwrap_or_default()
    } else {
        log.error.unwrap_or_default()
    };

    LogEntryView {
        status_label: if success { "成功" } else { "失败" }.to_string(),
        status_class: if success { "success" } else { "warning" }.to_string(),
        mode: log.mode.as_str().to_string(),
        tone_label,
        has_tone: log.tone.is_some(),
        created_at: log.created_at,
        duration_ms: log.duration_ms,
        model: log.model,
        has_request_id: !request_id.is_empty(),
        request_id,
        input: log.input,
        output_label: if success { "输出" } else { "错误" }.to_string(),
        output_text,
    }
}

fn static_model_picker(config: &Config) -> ModelPickerTemplate {
    let groups = static_model_groups(&config.model);

    ModelPickerTemplate {
        source_label: "内置模型".to_string(),
        groups: model_group_views(&config.model, &config.model, groups),
    }
}

fn model_group_views(
    default_model: &str,
    selected_model: &str,
    groups: Vec<ModelCatalogGroup>,
) -> Vec<ModelGroupView> {
    groups
        .into_iter()
        .map(|group| ModelGroupView {
            label: group.label,
            models: group
                .models
                .into_iter()
                .map(|model| ModelOptionView {
                    selected: model == selected_model,
                    is_default: model == default_model,
                    value: model,
                })
                .collect(),
        })
        .collect()
}

fn tool_views() -> Vec<ToolView> {
    vec![
        ToolView {
            mode: "zh_to_en",
            title: "中译英",
            description: "保留语气、格式和细节，输出自然英文。",
            placeholder: "粘贴中文内容，例如：\n这份方案整体不错，但还需要把风险和时间线写得更清楚。",
            output_placeholder: "英文译文会显示在这里。",
            action_label: "翻译成英文",
            icon: "文",
            accent_class: "accent-teal",
            show_tone: true,
        },
        ToolView {
            mode: "en_to_zh",
            title: "英译中",
            description: "把英文转成清晰自然的简体中文。",
            placeholder: "Paste English text, for example:\nThe proposal is solid, but the risk section needs sharper wording.",
            output_placeholder: "中文译文会显示在这里。",
            action_label: "翻译成中文",
            icon: "译",
            accent_class: "accent-blue",
            show_tone: false,
        },
        ToolView {
            mode: "polish_en",
            title: "英文润色",
            description: "修正语法，让英文更顺、更专业。",
            placeholder: "Paste English draft, for example:\nI think this part can be more clearly and easy to understand.",
            output_placeholder: "润色后的英文会显示在这里。",
            action_label: "润色英文",
            icon: "修",
            accent_class: "accent-rose",
            show_tone: false,
        },
    ]
}

fn tone_options() -> Vec<ToneOptionView> {
    ZhToEnTone::ALL
        .iter()
        .map(|tone| ToneOptionView {
            value: tone.as_str(),
            label: tone.label(),
            description: tone.description(),
        })
        .collect()
}

fn run_error_template(mode: &str, message: &str) -> RunResultTemplate {
    RunResultTemplate {
        mode: mode.to_string(),
        success: false,
        has_text: false,
        text: String::new(),
        error: message.to_string(),
        request_id: String::new(),
        has_request_id: false,
        placeholder: String::new(),
    }
}

fn render_html<T: Template>(template: T) -> Response {
    match template.render() {
        Ok(html) => Html(html).into_response(),
        Err(error) => error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Template rendering failed: {error}"),
        ),
    }
}

fn render_template_string<T: Template>(template: T) -> String {
    template.render().unwrap_or_else(|error| {
        format!(
            r#"<div class="alert alert-error">Template rendering failed: {}</div>"#,
            escape_html(&error.to_string())
        )
    })
}

fn require_json_auth(state: &AppState, headers: &HeaderMap) -> Option<Response> {
    (!get_auth_status(headers, &state.config).authenticated)
        .then(|| json_error(StatusCode::UNAUTHORIZED, "Unauthorized"))
}

fn login_error_response(message: String, retry_after_seconds: Option<u64>) -> Response {
    let mut response = render_html(LoginTemplate {
        error: message,
        has_error: true,
    });

    if let Some(seconds) = retry_after_seconds {
        response.headers_mut().insert(
            header::RETRY_AFTER,
            HeaderValue::from_str(&seconds.to_string()).expect("retry after header is valid"),
        );
        *response.status_mut() = StatusCode::TOO_MANY_REQUESTS;
    } else {
        *response.status_mut() = StatusCode::UNAUTHORIZED;
    }

    response
}

fn json_login_block(block: LoginAttemptBlock) -> Response {
    let mut response = Json(json!({
        "error": block.message,
        "reason": block.reason,
        "retryAfterSeconds": block.retry_after_seconds
    }))
    .into_response();

    *response.status_mut() = StatusCode::TOO_MANY_REQUESTS;
    response.headers_mut().insert(
        header::RETRY_AFTER,
        HeaderValue::from_str(&block.retry_after_seconds.to_string())
            .expect("retry after header is valid"),
    );

    response
}

fn json_error(status: StatusCode, message: impl Into<String>) -> Response {
    let mut response = Json(json!({ "error": message.into() })).into_response();
    *response.status_mut() = status;

    response
}

fn error_response(status: StatusCode, message: impl Into<String>) -> Response {
    let mut response = Html(format!(
        r#"<div class="alert alert-error"><strong>请求失败</strong><p>{}</p></div>"#,
        escape_html(&message.into())
    ))
    .into_response();
    *response.status_mut() = status;

    response
}

fn call_log_error_message(error: LogError) -> String {
    format!("Call log database operation failed. {error}")
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use axum::body::to_bytes;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use tower::ServiceExt;

    #[derive(Clone, Default)]
    struct MockOpenAi {
        fail: bool,
        models: Vec<String>,
    }

    #[async_trait]
    impl OpenAiService for MockOpenAi {
        async fn run_prompt(&self, input: RunPromptInput) -> Result<RunPromptResult, OpenAiError> {
            if self.fail || input.input == "fail" {
                return Err(OpenAiError::Request("mock failure".to_string()));
            }

            Ok(RunPromptResult {
                output_text: format!(
                    "processed:{}:{}:{}",
                    input.mode.as_str(),
                    input.tone.map(|tone| tone.as_str()).unwrap_or("none"),
                    input.input
                ),
                mode: input.mode,
                model: input.model,
                request_id: Some("req_test".to_string()),
            })
        }

        async fn list_models(&self) -> Result<Vec<String>, OpenAiError> {
            Ok(self.models.clone())
        }
    }

    async fn test_state(config: Config, openai: MockOpenAi) -> AppState {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::new()
                    .in_memory(true)
                    .create_if_missing(true),
            )
            .await
            .unwrap();
        let logs = LogStore::from_pool(pool, config.log_limit);
        logs.init().await.unwrap();

        AppState::with_parts(config, logs, Arc::new(openai))
    }

    fn base_config() -> Config {
        Config {
            app_password: None,
            api_key: Some("test-key".to_string()),
            model: "test-model".to_string(),
            proxy_url: Some("http://127.0.0.1:7890".to_string()),
            db_path: ":memory:".to_string(),
            log_limit: 2,
            port: 8787,
            production: false,
        }
    }

    async fn response_json(response: Response) -> Value {
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();

        serde_json::from_slice(&body).unwrap()
    }

    #[tokio::test]
    async fn reports_auth_disabled_by_default() {
        let mut config = base_config();
        config.app_password = None;
        let app = create_router(test_state(config, MockOpenAi::default()).await);
        let response = app
            .oneshot(
                axum::http::Request::get("/auth/status")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = response_json(response).await;

        assert_eq!(body, json!({ "enabled": false, "authenticated": true }));
    }

    #[tokio::test]
    async fn reports_health_without_exposing_secrets() {
        let app = create_router(test_state(base_config(), MockOpenAi::default()).await);
        let response = app
            .oneshot(
                axum::http::Request::get("/health")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = response_json(response).await;

        assert_eq!(body["ok"], true);
        assert_eq!(body["model"], "test-model");
        assert_eq!(body["hasApiKey"], true);
        assert_eq!(body["hasProxy"], true);
        assert!(body["models"]
            .as_array()
            .unwrap()
            .iter()
            .any(|model| model == "test-model"));
        assert!(!body.to_string().contains("test-key"));
    }

    #[tokio::test]
    async fn requires_password_when_configured() {
        let mut config = base_config();
        config.app_password = Some("secret".to_string());
        let app = create_router(test_state(config, MockOpenAi::default()).await);

        let unauthorized = app
            .clone()
            .oneshot(
                axum::http::Request::get("/health")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

        let failed_login = app
            .clone()
            .oneshot(
                axum::http::Request::post("/auth/login")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(axum::body::Body::from(r#"{"password":"wrong"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(failed_login.status(), StatusCode::UNAUTHORIZED);

        let login = app
            .clone()
            .oneshot(
                axum::http::Request::post("/auth/login")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(axum::body::Body::from(r#"{"password":"secret"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        let cookie = login
            .headers()
            .get(header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string();

        assert_eq!(login.status(), StatusCode::OK);
        assert!(cookie.starts_with("prompt_pocket_auth="));

        let health = app
            .clone()
            .oneshot(
                axum::http::Request::get("/health")
                    .header(header::COOKIE, cookie.clone())
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(health.status(), StatusCode::OK);

        let run = app
            .oneshot(
                axum::http::Request::post("/api/run")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(header::COOKIE, cookie)
                    .body(axum::body::Body::from(
                        r#"{"mode":"zh_to_en","input":"你好","model":"gpt-5.2"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = response_json(run).await;

        assert_eq!(body["outputText"], "processed:zh_to_en:casual:你好");
    }

    #[tokio::test]
    async fn rejects_invalid_run_requests() {
        let app = create_router(test_state(base_config(), MockOpenAi::default()).await);

        for (payload, status) in [
            (
                r#"{"mode":"summarize","input":"hello"}"#,
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
            (
                r#"{"mode":"polish_en","input":"   "}"#,
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
            (
                r#"{"mode":"zh_to_en","input":"你好","tone":"formal"}"#,
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
            (
                r#"{"mode":"polish_en","input":"hello","model":"not a model"}"#,
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
        ] {
            let response = app
                .clone()
                .oneshot(
                    axum::http::Request::post("/api/run")
                        .header(header::CONTENT_TYPE, "application/json")
                        .body(axum::body::Body::from(payload))
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(response.status(), status);
        }
    }

    #[tokio::test]
    async fn returns_dynamic_gpt_models_only() {
        let state = test_state(
            base_config(),
            MockOpenAi {
                fail: false,
                models: vec![
                    "gpt-5.6".to_string(),
                    "gpt-4.2-mini".to_string(),
                    "gpt-image-2".to_string(),
                    "gpt-4o-transcribe".to_string(),
                    "o3".to_string(),
                ],
            },
        )
        .await;
        let app = create_router(state);
        let response = app
            .oneshot(
                axum::http::Request::get("/api/models")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = response_json(response).await;

        assert_eq!(body["source"], "dynamic");
        assert!(body["models"]
            .as_array()
            .unwrap()
            .iter()
            .any(|model| model == "gpt-5.6"));
        assert!(!body["models"]
            .as_array()
            .unwrap()
            .iter()
            .any(|model| model == "gpt-image-2"));
        assert!(!body["models"]
            .as_array()
            .unwrap()
            .iter()
            .any(|model| model == "o3"));
    }

    #[tokio::test]
    async fn records_successful_and_failed_runs() {
        let app = create_router(test_state(base_config(), MockOpenAi::default()).await);

        let success = app
            .clone()
            .oneshot(
                axum::http::Request::post("/api/run")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(axum::body::Body::from(
                        r#"{"mode":"zh_to_en","input":"hello","tone":"casual","model":"gpt-5.2"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(success.status(), StatusCode::OK);

        let failure = app
            .clone()
            .oneshot(
                axum::http::Request::post("/api/run")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(axum::body::Body::from(
                        r#"{"mode":"zh_to_en","input":"fail","model":"gpt-5.2"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(failure.status(), StatusCode::BAD_GATEWAY);

        let logs = app
            .clone()
            .oneshot(
                axum::http::Request::get("/api/logs")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = response_json(logs).await;

        assert_eq!(body["limit"], 2);
        assert_eq!(body["logs"].as_array().unwrap().len(), 2);
        assert_eq!(body["logs"][0]["status"], "error");
        assert_eq!(body["logs"][0]["input"], "fail");
        assert_eq!(body["logs"][1]["status"], "success");
        assert_eq!(body["logs"][1]["input"], "hello");

        let page = app
            .oneshot(
                axum::http::Request::get("/logs")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(page.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn htmx_run_returns_partial_html() {
        let app = create_router(test_state(base_config(), MockOpenAi::default()).await);
        let response = app
            .oneshot(
                axum::http::Request::post("/ui/run")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .header("HX-Request", "true")
                    .body(axum::body::Body::from(
                        "mode=polish_en&input=hello&model=gpt-5.2",
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let html = String::from_utf8(body.to_vec()).unwrap();

        assert!(html.contains("processed:polish_en:none:hello"));
        assert!(!html.contains("<html"));
    }
}
