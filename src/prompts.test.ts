import { describe, expect, test } from "bun:test";
import { getPromptMode, isPromptMode, isZhToEnTone, PROMPT_MODES } from "./prompts";

describe("prompt modes", () => {
  test("accepts the supported modes", () => {
    expect(isPromptMode("zh_to_en")).toBe(true);
    expect(isPromptMode("en_to_zh")).toBe(true);
    expect(isPromptMode("polish_en")).toBe(true);
  });

  test("rejects unsupported modes", () => {
    expect(isPromptMode("summarize")).toBe(false);
    expect(isPromptMode(undefined)).toBe(false);
    expect(isPromptMode(null)).toBe(false);
  });

  test("accepts supported zh-to-en tones", () => {
    expect(isZhToEnTone("casual")).toBe(true);
    expect(isZhToEnTone("business")).toBe(true);
    expect(isZhToEnTone("formal")).toBe(false);
  });

  test("maps every mode to instructions", () => {
    for (const mode of Object.keys(PROMPT_MODES)) {
      const prompt = getPromptMode(mode as keyof typeof PROMPT_MODES);

      expect(prompt.instructions.length).toBeGreaterThan(20);
      expect(prompt.label.length).toBeGreaterThan(0);
    }
  });

  test("maps zh-to-en tones to different instructions", () => {
    const casualPrompt = getPromptMode("zh_to_en", {
      zhToEnTone: "casual"
    });
    const businessPrompt = getPromptMode("zh_to_en", {
      zhToEnTone: "business"
    });

    expect(casualPrompt.instructions).toContain("conversational");
    expect(businessPrompt.instructions).toContain("workplace English");
    expect(casualPrompt.instructions).not.toBe(businessPrompt.instructions);
  });

  test("uses casual zh-to-en tone by default", () => {
    const prompt = getPromptMode("zh_to_en");

    expect(prompt.instructions).toContain("conversational");
    expect(prompt.instructions).not.toContain("workplace English");
  });
});
