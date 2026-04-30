export const ZH_TO_EN_TONES = {
  casual: {
    label: "口语化",
    instructions: [
      "Use conversational, plain English that sounds natural in everyday speaking or chat.",
      "Prefer simple wording and contractions when appropriate, but do not become slangy unless the source text is casual."
    ].join("\n")
  },
  business: {
    label: "工作用",
    instructions: [
      "Use clear, polished workplace English suitable for email, documentation, product discussions, or business messages.",
      "Prefer concise professional wording over overly casual expressions."
    ].join("\n")
  }
} as const;

export type ZhToEnTone = keyof typeof ZH_TO_EN_TONES;

export const DEFAULT_ZH_TO_EN_TONE: ZhToEnTone = "casual";

export const PROMPT_MODES = {
  zh_to_en: {
    label: "中译英",
    description: "把中文翻成自然、准确的英文。",
    instructions: [
      "Translate the user's Chinese text into natural, fluent English.",
      "Preserve the original meaning, tone, formatting, names, numbers, and punctuation as much as possible.",
      "Do not add explanations, alternatives, labels, or commentary.",
      "Return only the translated English text."
    ].join("\n")
  },
  en_to_zh: {
    label: "英译中",
    description: "把英文翻成自然、准确的简体中文。",
    instructions: [
      "Translate the user's English text into natural, fluent Simplified Chinese.",
      "Preserve the original meaning, tone, formatting, names, numbers, and punctuation as much as possible.",
      "Do not add explanations, alternatives, labels, or commentary.",
      "Return only the translated Chinese text."
    ].join("\n")
  },
  polish_en: {
    label: "英文润色",
    description: "修正英文语法，并让表达更自然。",
    instructions: [
      "Polish the user's English text for grammar, clarity, and natural phrasing.",
      "Preserve the original meaning, intent, formatting, names, numbers, and tone.",
      "If the input is a fragment, keep it as a fragment instead of expanding it into a full paragraph.",
      "Do not add explanations, alternatives, labels, or commentary.",
      "Return only the polished English text."
    ].join("\n")
  }
} as const;

export type PromptMode = keyof typeof PROMPT_MODES;

export function isPromptMode(value: unknown): value is PromptMode {
  return typeof value === "string" && value in PROMPT_MODES;
}

export function isZhToEnTone(value: unknown): value is ZhToEnTone {
  return typeof value === "string" && value in ZH_TO_EN_TONES;
}

export function getPromptMode(
  mode: PromptMode,
  options: {
    zhToEnTone?: ZhToEnTone;
  } = {}
) {
  const prompt = PROMPT_MODES[mode];

  if (mode !== "zh_to_en") {
    return prompt;
  }

  const tone = options.zhToEnTone ?? DEFAULT_ZH_TO_EN_TONE;

  return {
    ...prompt,
    instructions: [prompt.instructions, ZH_TO_EN_TONES[tone].instructions].join("\n")
  };
}
