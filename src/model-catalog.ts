export const DEFAULT_MODEL = "gpt-5.2";

export type ModelCatalogGroup = {
  label: string;
  models: string[];
};

export const STATIC_MODEL_CATALOG: ModelCatalogGroup[] = [
  {
    label: "GPT-5 models",
    models: [
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
      "gpt-5-nano"
    ]
  },
  {
    label: "GPT-4 models",
    models: [
      "gpt-4.1",
      "gpt-4.1-mini",
      "gpt-4.1-nano",
      "gpt-4o",
      "gpt-4o-mini",
      "gpt-4.5-preview",
      "gpt-4-turbo",
      "gpt-4-turbo-preview",
      "gpt-4"
    ]
  }
];

export function getStaticModelIds(): string[] {
  return uniqueStrings(STATIC_MODEL_CATALOG.flatMap((group) => group.models));
}

export function getStaticModelGroups(defaultModel: string): ModelCatalogGroup[] {
  const groups = cloneGroups(STATIC_MODEL_CATALOG);

  if (!getStaticModelIds().includes(defaultModel)) {
    return [
      {
        label: "Configured default",
        models: [defaultModel]
      },
      ...groups
    ];
  }

  return groups;
}

export function mergeModelGroups(defaultModel: string, dynamicModelIds: string[]): ModelCatalogGroup[] {
  const staticIds = getStaticModelIds();
  const groups = getStaticModelGroups(defaultModel);
  const dynamicOnly = uniqueStrings(dynamicModelIds)
    .filter((model) => isSupportedGptTextModelId(model))
    .filter((model) => !staticIds.includes(model) && model !== defaultModel)
    .sort((left, right) => left.localeCompare(right));

  if (dynamicOnly.length === 0) {
    return groups;
  }

  return [
    {
      label: "Available from your API key",
      models: dynamicOnly
    },
    ...groups
  ];
}

export function flattenModelGroups(groups: ModelCatalogGroup[]): string[] {
  return uniqueStrings(groups.flatMap((group) => group.models));
}

export function isValidModelId(model: string): boolean {
  return /^[A-Za-z0-9._:-]{1,160}$/.test(model);
}

export function isSupportedGptTextModelId(model: string): boolean {
  if (!isValidModelId(model) || !/^gpt-[45]/.test(model)) {
    return false;
  }

  return !/(audio|codex|image|realtime|search|transcribe|tts)/.test(model);
}

function cloneGroups(groups: ModelCatalogGroup[]): ModelCatalogGroup[] {
  return groups.map((group) => ({
    label: group.label,
    models: [...group.models]
  }));
}

function uniqueStrings(values: string[]): string[] {
  return Array.from(new Set(values));
}
