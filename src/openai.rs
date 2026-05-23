use async_trait::async_trait;
use reqwest::{Client, Proxy};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    config::Config,
    prompts::{get_instructions, PromptMode, ZhToEnTone},
};

#[derive(Clone, Debug)]
pub struct RunPromptInput {
    pub mode: PromptMode,
    pub input: String,
    pub model: String,
    pub tone: Option<ZhToEnTone>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunPromptResult {
    pub output_text: String,
    pub mode: PromptMode,
    pub model: String,
    pub request_id: Option<String>,
}

#[derive(Debug, Error)]
pub enum OpenAiError {
    #[error("OPENAI_API_KEY is not configured.")]
    MissingApiKey,
    #[error("{0}")]
    Request(String),
}

#[async_trait]
pub trait OpenAiService: Send + Sync {
    async fn run_prompt(&self, input: RunPromptInput) -> Result<RunPromptResult, OpenAiError>;
    async fn list_models(&self) -> Result<Vec<String>, OpenAiError>;
}

#[derive(Clone)]
pub struct RealOpenAiService {
    config: Config,
    client: Client,
}

impl RealOpenAiService {
    pub fn new(config: Config) -> Result<Self, OpenAiError> {
        let mut builder = Client::builder();

        if let Some(proxy_url) = config.proxy_url.as_deref() {
            let proxy = Proxy::all(proxy_url).map_err(|error| {
                OpenAiError::Request(format!("Invalid OPENAI_PROXY_URL: {error}"))
            })?;
            builder = builder.proxy(proxy);
        }

        let client = builder.build().map_err(|error| {
            OpenAiError::Request(format!("Could not create HTTP client: {error}"))
        })?;

        Ok(Self { config, client })
    }
}

#[async_trait]
impl OpenAiService for RealOpenAiService {
    async fn run_prompt(&self, input: RunPromptInput) -> Result<RunPromptResult, OpenAiError> {
        let api_key = self
            .config
            .api_key
            .as_deref()
            .ok_or(OpenAiError::MissingApiKey)?;
        let instructions = get_instructions(input.mode, input.tone);
        let body = ResponsesRequest {
            model: &input.model,
            instructions: &instructions,
            input: &input.input,
        };
        let response = self
            .client
            .post("https://api.openai.com/v1/responses")
            .bearer_auth(api_key)
            .json(&body)
            .send()
            .await
            .map_err(|error| OpenAiError::Request(format!("OpenAI request failed: {error}")))?;
        let request_id = response
            .headers()
            .get("x-request-id")
            .and_then(|value| value.to_str().ok())
            .map(ToString::to_string);
        let status = response.status();

        if !status.is_success() {
            let message = response
                .text()
                .await
                .unwrap_or_else(|_| "OpenAI request failed.".to_string());

            return Err(OpenAiError::Request(format!(
                "OpenAI request failed with status {status}: {message}"
            )));
        }

        let payload = response
            .json::<ResponsesResponse>()
            .await
            .map_err(|error| {
                OpenAiError::Request(format!("Could not parse OpenAI response: {error}"))
            })?;
        let output_text = payload.extract_output_text().ok_or_else(|| {
            OpenAiError::Request("OpenAI response did not include text output.".to_string())
        })?;

        Ok(RunPromptResult {
            output_text,
            mode: input.mode,
            model: input.model,
            request_id,
        })
    }

    async fn list_models(&self) -> Result<Vec<String>, OpenAiError> {
        let api_key = self
            .config
            .api_key
            .as_deref()
            .ok_or(OpenAiError::MissingApiKey)?;
        let response = self
            .client
            .get("https://api.openai.com/v1/models")
            .bearer_auth(api_key)
            .send()
            .await
            .map_err(|error| {
                OpenAiError::Request(format!("Could not fetch OpenAI models: {error}"))
            })?;
        let status = response.status();

        if !status.is_success() {
            let message = response
                .text()
                .await
                .unwrap_or_else(|_| "Could not fetch OpenAI models.".to_string());

            return Err(OpenAiError::Request(format!(
                "Could not fetch OpenAI models with status {status}: {message}"
            )));
        }

        let mut models = response
            .json::<ModelsResponse>()
            .await
            .map_err(|error| {
                OpenAiError::Request(format!("Could not parse OpenAI models: {error}"))
            })?
            .data
            .into_iter()
            .map(|model| model.id)
            .collect::<Vec<_>>();

        models.sort();

        Ok(models)
    }
}

#[derive(Serialize)]
struct ResponsesRequest<'a> {
    model: &'a str,
    instructions: &'a str,
    input: &'a str,
}

#[derive(Deserialize)]
struct ResponsesResponse {
    output_text: Option<String>,
    output: Option<Vec<ResponseOutputItem>>,
}

impl ResponsesResponse {
    fn extract_output_text(self) -> Option<String> {
        normalize_text(self.output_text).or_else(|| {
            let text = self
                .output?
                .into_iter()
                .flat_map(|item| item.content.unwrap_or_default())
                .filter_map(|content| {
                    if content.kind.as_deref() == Some("output_text") {
                        content.text
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("");

            normalize_text(Some(text))
        })
    }
}

#[derive(Deserialize)]
struct ResponseOutputItem {
    content: Option<Vec<ResponseContent>>,
}

#[derive(Deserialize)]
struct ResponseContent {
    #[serde(rename = "type")]
    kind: Option<String>,
    text: Option<String>,
}

#[derive(Deserialize)]
struct ModelsResponse {
    data: Vec<ModelItem>,
}

#[derive(Deserialize)]
struct ModelItem {
    id: String,
}

fn normalize_text(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_top_level_output_text_when_present() {
        let payload: ResponsesResponse = serde_json::from_value(serde_json::json!({
            "output_text": " hello "
        }))
        .unwrap();

        assert_eq!(payload.extract_output_text().as_deref(), Some("hello"));
    }

    #[test]
    fn extracts_nested_response_output_text_from_raw_http_shape() {
        let payload: ResponsesResponse = serde_json::from_value(serde_json::json!({
            "output": [
                {
                    "type": "message",
                    "content": [
                        {
                            "type": "output_text",
                            "text": "Hello"
                        },
                        {
                            "type": "output_text",
                            "text": ", world"
                        }
                    ]
                }
            ]
        }))
        .unwrap();

        assert_eq!(
            payload.extract_output_text().as_deref(),
            Some("Hello, world")
        );
    }

    #[test]
    fn ignores_non_text_response_content() {
        let payload: ResponsesResponse = serde_json::from_value(serde_json::json!({
            "output": [
                {
                    "type": "message",
                    "content": [
                        {
                            "type": "refusal",
                            "refusal": "No"
                        }
                    ]
                }
            ]
        }))
        .unwrap();

        assert_eq!(payload.extract_output_text(), None);
    }
}
