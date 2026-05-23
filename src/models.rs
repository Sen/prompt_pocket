use serde::Serialize;
use std::collections::HashSet;

use crate::config::DEFAULT_MODEL;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCatalogGroup {
    pub label: String,
    pub models: Vec<String>,
}

const GPT_5_MODELS: &[&str] = &[
    "gpt-5.5",
    "gpt-5.5-pro",
    "gpt-5.4",
    "gpt-5.4-pro",
    "gpt-5.4-mini",
    "gpt-5.4-nano",
    "gpt-5.2",
    "gpt-5.2-pro",
    "gpt-5.1",
    "gpt-5",
    "gpt-5-pro",
    "gpt-5-mini",
    "gpt-5-nano",
];

const GPT_4_MODELS: &[&str] = &[
    "gpt-4.1",
    "gpt-4.1-mini",
    "gpt-4.1-nano",
    "gpt-4o",
    "gpt-4o-mini",
    "gpt-4.5-preview",
    "gpt-4-turbo",
    "gpt-4-turbo-preview",
    "gpt-4",
];

pub fn static_model_groups(default_model: &str) -> Vec<ModelCatalogGroup> {
    let mut groups = vec![
        ModelCatalogGroup {
            label: "GPT-5 models".to_string(),
            models: GPT_5_MODELS
                .iter()
                .map(|model| (*model).to_string())
                .collect(),
        },
        ModelCatalogGroup {
            label: "GPT-4 models".to_string(),
            models: GPT_4_MODELS
                .iter()
                .map(|model| (*model).to_string())
                .collect(),
        },
    ];

    if !static_model_ids()
        .iter()
        .any(|model| model == default_model)
    {
        groups.insert(
            0,
            ModelCatalogGroup {
                label: "Configured default".to_string(),
                models: vec![default_model.to_string()],
            },
        );
    }

    groups
}

pub fn merge_model_groups(
    default_model: &str,
    dynamic_model_ids: &[String],
) -> Vec<ModelCatalogGroup> {
    let static_ids = static_model_ids();
    let mut seen = HashSet::new();
    let mut dynamic_only = dynamic_model_ids
        .iter()
        .filter(|model| is_supported_gpt_text_model_id(model))
        .filter(|model| !static_ids.contains(*model) && model.as_str() != default_model)
        .filter(|model| seen.insert((*model).clone()))
        .cloned()
        .collect::<Vec<_>>();

    dynamic_only.sort();

    let groups = static_model_groups(default_model);

    if dynamic_only.is_empty() {
        groups
    } else {
        let mut merged = vec![ModelCatalogGroup {
            label: "Available from your API key".to_string(),
            models: dynamic_only,
        }];
        merged.extend(groups);
        merged
    }
}

pub fn flatten_model_groups(groups: &[ModelCatalogGroup]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut models = Vec::new();

    for group in groups {
        for model in &group.models {
            if seen.insert(model.clone()) {
                models.push(model.clone());
            }
        }
    }

    models
}

pub fn is_valid_model_id(model: &str) -> bool {
    let len = model.len();

    (1..=160).contains(&len)
        && model.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | ':' | '-')
        })
}

pub fn is_supported_gpt_text_model_id(model: &str) -> bool {
    is_valid_model_id(model)
        && (model.starts_with("gpt-4") || model.starts_with("gpt-5"))
        && ![
            "audio",
            "codex",
            "image",
            "realtime",
            "search",
            "transcribe",
            "tts",
        ]
        .iter()
        .any(|blocked| model.contains(blocked))
}

pub fn default_model_groups() -> Vec<ModelCatalogGroup> {
    static_model_groups(DEFAULT_MODEL)
}

fn static_model_ids() -> HashSet<String> {
    GPT_5_MODELS
        .iter()
        .chain(GPT_4_MODELS.iter())
        .map(|model| (*model).to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn includes_configured_default_model_when_missing_from_static_catalog() {
        let groups = static_model_groups("custom-model");

        assert_eq!(groups[0].label, "Configured default");
        assert_eq!(groups[0].models, vec!["custom-model"]);
    }

    #[test]
    fn validates_model_ids() {
        assert!(is_valid_model_id("gpt-5.2"));
        assert!(is_valid_model_id("gpt:custom_model-1"));
        assert!(!is_valid_model_id("not a model"));
        assert!(!is_valid_model_id(""));
    }

    #[test]
    fn merges_dynamic_gpt_text_models_only() {
        let groups = merge_model_groups(
            "gpt-5.2",
            &[
                "gpt-5.6".to_string(),
                "gpt-4.2-mini".to_string(),
                "gpt-image-2".to_string(),
                "gpt-4o-transcribe".to_string(),
                "o3".to_string(),
                "gpt-5.6".to_string(),
            ],
        );
        let models = flatten_model_groups(&groups);

        assert_eq!(groups[0].label, "Available from your API key");
        assert!(models.contains(&"gpt-5.6".to_string()));
        assert!(models.contains(&"gpt-4.2-mini".to_string()));
        assert!(!models.contains(&"gpt-image-2".to_string()));
        assert!(!models.contains(&"gpt-4o-transcribe".to_string()));
        assert!(!models.contains(&"o3".to_string()));
    }
}
