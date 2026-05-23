use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptMode {
    ZhToEn,
    EnToZh,
    PolishEn,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ZhToEnTone {
    Casual,
    Business,
    Email,
}

#[derive(Clone, Copy, Debug)]
pub struct PromptDefinition {
    pub label: &'static str,
    pub description: &'static str,
    pub instructions: &'static str,
}

pub const DEFAULT_ZH_TO_EN_TONE: ZhToEnTone = ZhToEnTone::Casual;

impl PromptMode {
    pub const ALL: [PromptMode; 3] = [PromptMode::ZhToEn, PromptMode::EnToZh, PromptMode::PolishEn];

    pub fn as_str(self) -> &'static str {
        match self {
            PromptMode::ZhToEn => "zh_to_en",
            PromptMode::EnToZh => "en_to_zh",
            PromptMode::PolishEn => "polish_en",
        }
    }

    pub fn definition(self) -> PromptDefinition {
        match self {
            PromptMode::ZhToEn => PromptDefinition {
                label: "中译英",
                description: "把中文翻成自然、准确的英文。",
                instructions: "Translate the user's Chinese text into natural, fluent English.\nPreserve the original meaning, tone, formatting, names, numbers, and punctuation as much as possible.\nDo not add explanations, alternatives, labels, or commentary.\nReturn only the translated English text.",
            },
            PromptMode::EnToZh => PromptDefinition {
                label: "英译中",
                description: "把英文翻成自然、准确的简体中文。",
                instructions: "Translate the user's English text into natural, fluent Simplified Chinese.\nPreserve the original meaning, tone, formatting, names, numbers, and punctuation as much as possible.\nDo not add explanations, alternatives, labels, or commentary.\nReturn only the translated Chinese text.",
            },
            PromptMode::PolishEn => PromptDefinition {
                label: "英文润色",
                description: "修正英文语法，并让表达更自然。",
                instructions: "Polish the user's English text for grammar, clarity, and natural phrasing.\nPreserve the original meaning, intent, formatting, names, numbers, and tone.\nIf the input is a fragment, keep it as a fragment instead of expanding it into a full paragraph.\nDo not add explanations, alternatives, labels, or commentary.\nReturn only the polished English text.",
            },
        }
    }
}

impl fmt::Display for PromptMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for PromptMode {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "zh_to_en" => Ok(Self::ZhToEn),
            "en_to_zh" => Ok(Self::EnToZh),
            "polish_en" => Ok(Self::PolishEn),
            _ => Err(()),
        }
    }
}

impl ZhToEnTone {
    pub const ALL: [ZhToEnTone; 3] = [ZhToEnTone::Casual, ZhToEnTone::Business, ZhToEnTone::Email];

    pub fn as_str(self) -> &'static str {
        match self {
            ZhToEnTone::Casual => "casual",
            ZhToEnTone::Business => "business",
            ZhToEnTone::Email => "email",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ZhToEnTone::Casual => "口语化",
            ZhToEnTone::Business => "工作用",
            ZhToEnTone::Email => "邮件用",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            ZhToEnTone::Casual => "自然、轻松、日常表达",
            ZhToEnTone::Business => "清晰、专业、适合工作",
            ZhToEnTone::Email => "礼貌、友好、适合邮件",
        }
    }

    pub fn instructions(self) -> &'static str {
        match self {
            ZhToEnTone::Casual => "Use conversational, plain English that sounds natural in everyday speaking or chat.\nPrefer simple wording and contractions when appropriate, but do not become slangy unless the source text is casual.",
            ZhToEnTone::Business => "Use clear, polished workplace English suitable for documentation, product discussions, or internal business messages.\nPrefer concise professional wording over overly casual expressions.",
            ZhToEnTone::Email => "Use courteous, friendly, professional English suitable for email communication.\nTranslate by meaning and intent instead of word-for-word literal translation.\nSoften direct wording tactfully with polite phrasing, while keeping the request clear and easy to act on.\nUse common email wording such as please, thank you, would, could, and appreciate when appropriate.\nYou may add brief courtesy, transition, or context-setting phrases when they make the email sound more natural and considerate.\nDo not add unrelated facts, a subject line, greeting, or signature unless the source text includes or asks for it.",
        }
    }
}

impl fmt::Display for ZhToEnTone {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for ZhToEnTone {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "casual" => Ok(Self::Casual),
            "business" => Ok(Self::Business),
            "email" => Ok(Self::Email),
            _ => Err(()),
        }
    }
}

pub fn get_instructions(mode: PromptMode, tone: Option<ZhToEnTone>) -> String {
    let definition = mode.definition();

    if mode != PromptMode::ZhToEn {
        return definition.instructions.to_string();
    }

    let tone = tone.unwrap_or(DEFAULT_ZH_TO_EN_TONE);

    format!("{}\n{}", definition.instructions, tone.instructions())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_supported_modes_and_tones() {
        assert_eq!("zh_to_en".parse::<PromptMode>(), Ok(PromptMode::ZhToEn));
        assert_eq!("en_to_zh".parse::<PromptMode>(), Ok(PromptMode::EnToZh));
        assert_eq!("polish_en".parse::<PromptMode>(), Ok(PromptMode::PolishEn));
        assert!("summarize".parse::<PromptMode>().is_err());

        assert_eq!("casual".parse::<ZhToEnTone>(), Ok(ZhToEnTone::Casual));
        assert_eq!("business".parse::<ZhToEnTone>(), Ok(ZhToEnTone::Business));
        assert_eq!("email".parse::<ZhToEnTone>(), Ok(ZhToEnTone::Email));
        assert!("formal".parse::<ZhToEnTone>().is_err());
    }

    #[test]
    fn maps_every_mode_to_instructions() {
        for mode in PromptMode::ALL {
            assert!(mode.definition().instructions.len() > 20);
            assert!(!mode.definition().label.is_empty());
        }
    }

    #[test]
    fn appends_zh_to_en_tone_instructions() {
        let casual = get_instructions(PromptMode::ZhToEn, Some(ZhToEnTone::Casual));
        let business = get_instructions(PromptMode::ZhToEn, Some(ZhToEnTone::Business));
        let email = get_instructions(PromptMode::ZhToEn, Some(ZhToEnTone::Email));

        assert!(casual.contains("conversational"));
        assert!(business.contains("workplace English"));
        assert!(email.contains("email communication"));
        assert_ne!(casual, business);
        assert_ne!(email, business);
    }

    #[test]
    fn defaults_zh_to_en_tone_to_casual() {
        let prompt = get_instructions(PromptMode::ZhToEn, None);

        assert!(prompt.contains("conversational"));
        assert!(!prompt.contains("workplace English"));
    }
}
