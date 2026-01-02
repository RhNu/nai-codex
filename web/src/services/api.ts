import axios from 'axios';

export const apiBase = import.meta.env.VITE_API_BASE || '/api';
export const api = axios.create({ baseURL: apiBase });

// 预览图服务路径（服务端挂载在根路径下，不在 /api 下）
export const previewsBase = '/previews';

// ============== Types ==============

export type Center = { x: number; y: number };

export type CharacterPrompt = {
  prompt: string;
  uc: string;
  center?: Center;
  enabled?: boolean;
};

export type GenerationParams = {
  model?: string;
  width?: number;
  height?: number;
  steps?: number;
  scale?: number;
  sampler?: string;
  noise?: string;
  cfg_rescale?: number;
  undesired_content_preset?: number | null;
  add_quality_tags?: boolean;
  character_prompts?: CharacterPrompt[];
  seed?: number | null;
  variety_plus?: boolean;
};

export type TaskSubmitPayload = {
  raw_prompt: string;
  negative_prompt: string;
  count?: number;
  params?: GenerationParams;
  preset_id?: string | null;
};

export type TaskStatus =
  | { status: 'pending' }
  | { status: 'running' }
  | { status: 'failed'; error: string }
  | { status: 'completed'; record: GenerationRecord }
  | { status: 'unknown' };

export type GenerationRecord = {
  id: string;
  task_id: string;
  created_at: string;
  raw_prompt: string;
  expanded_prompt: string;
  negative_prompt: string;
  images: Array<{ url: string; seed: number; width: number; height: number }>;
};

export type Page<T> = { items: T[]; total: number };

export type Snippet = {
  id: string;
  name: string;
  category: string;
  tags: string[];
  description?: string | null;
  preview_path?: string | null;
  content: string;
  created_at: string;
  updated_at: string;
};

export type SnippetSummary = {
  id: string;
  name: string;
  category: string;
  tags: string[];
  description?: string | null;
  preview_path?: string | null;
};

export type Preset = {
  id: string;
  name: string;
  description?: string | null;
  preview_path?: string | null;
  before?: string | null;
  after?: string | null;
  replace?: string | null;
  uc_before?: string | null;
  uc_after?: string | null;
  uc_replace?: string | null;
  created_at: string;
  updated_at: string;
};

export type PresetSummary = {
  id: string;
  name: string;
  description?: string | null;
  preview_path?: string | null;
};

// ============== Health ==============

export async function checkHealth() {
  const { data } = await api.get<string>('/health');
  return data;
}

// ============== Tasks ==============

export async function submitTask(payload: TaskSubmitPayload) {
  const { data } = await api.post<{ id: string }>('/tasks', payload);
  return data.id;
}

export async function fetchTaskStatus(id: string) {
  const { data } = await api.get<TaskStatus>(`/tasks/${id}`);
  return data;
}

// ============== Records ==============

export async function fetchRecentRecords() {
  const { data } = await api.get<GenerationRecord[]>('/records/recent');
  return data;
}

// ============== Snippets ==============

export async function fetchSnippets(params: {
  q?: string;
  category?: string;
  offset?: number;
  limit?: number;
}) {
  const { data } = await api.get<Page<SnippetSummary>>('/snippets', { params });
  return data;
}

export async function fetchSnippet(id: string) {
  const { data } = await api.get<Snippet>(`/snippets/${id}`);
  return data;
}

export async function createSnippet(payload: {
  name: string;
  category: string;
  content: string;
  description?: string;
  tags?: string[];
  preview_base64?: string;
}) {
  const { data } = await api.post<SnippetSummary>('/snippets', payload);
  return data;
}

export async function updateSnippet(
  id: string,
  payload: {
    name?: string;
    category?: string;
    content?: string;
    description?: string;
    tags?: string[];
    preview_base64?: string;
  },
) {
  const { data } = await api.put<Snippet>(`/snippets/${id}`, payload);
  return data;
}

export async function updateSnippetPreview(id: string, previewBase64: string) {
  const { data } = await api.put<Snippet>(`/snippets/${id}/preview`, {
    preview_base64: previewBase64,
  });
  return data;
}

export async function deleteSnippetPreview(id: string) {
  const { data } = await api.delete<Snippet>(`/snippets/${id}/preview`);
  return data;
}

export async function deleteSnippet(id: string) {
  await api.delete(`/snippets/${id}`);
}

// ============== Presets ==============

export async function fetchPresets(params: { offset?: number; limit?: number } = {}) {
  const { data } = await api.get<Page<PresetSummary>>('/presets', { params });
  return data;
}

export async function fetchPreset(id: string) {
  const { data } = await api.get<Preset>(`/presets/${id}`);
  return data;
}

export async function createPreset(payload: {
  name: string;
  description?: string;
  before?: string;
  after?: string;
  replace?: string;
  uc_before?: string;
  uc_after?: string;
  uc_replace?: string;
  preview_base64?: string;
}) {
  const { data } = await api.post<Preset>('/presets', payload);
  return data;
}

export async function updatePreset(
  id: string,
  payload: {
    name?: string;
    description?: string;
    before?: string;
    after?: string;
    replace?: string;
    uc_before?: string;
    uc_after?: string;
    uc_replace?: string;
    preview_base64?: string;
  },
) {
  const { data } = await api.put<Preset>(`/presets/${id}`, payload);
  return data;
}

export async function updatePresetPreview(id: string, previewBase64: string) {
  const { data } = await api.put<Preset>(`/presets/${id}/preview`, {
    preview_base64: previewBase64,
  });
  return data;
}

export async function deletePresetPreview(id: string) {
  const { data } = await api.delete<Preset>(`/presets/${id}/preview`);
  return data;
}

export async function deletePreset(id: string) {
  await api.delete(`/presets/${id}`);
}

// ============== Generation Settings ==============

export type CharacterSlotSettings = {
  prompt: string;
  uc: string;
  enabled: boolean;
  preset_id: string | null;
};

export type LastGenerationSettings = {
  prompt: string;
  negative_prompt: string;
  count: number;
  params: GenerationParams;
  character_slots?: CharacterSlotSettings[];
};

export async function loadGenerationSettings() {
  const { data } = await api.get<LastGenerationSettings>('/settings/generation');
  return data;
}

export async function saveGenerationSettings(settings: LastGenerationSettings) {
  await api.put('/settings/generation', settings);
}

// ============== Prompt API ==============

export type HighlightSpan = {
  start: number;
  end: number;
  weight: number;
  type:
    | 'text'
    | 'comma'
    | 'whitespace'
    | 'brace'
    | 'bracket'
    | 'weight_num'
    | 'weight_end'
    | 'snippet'
    | 'newline';
};

export type ParsePromptResponse = {
  spans: HighlightSpan[];
  unclosed_braces: number;
  unclosed_brackets: number;
  unclosed_weight: boolean;
};

export async function parsePrompt(prompt: string) {
  const { data } = await api.post<ParsePromptResponse>('/prompt/parse', { prompt });
  return data;
}

export async function formatPrompt(prompt: string) {
  const { data } = await api.post<{ formatted: string }>('/prompt/format', { prompt });
  return data.formatted;
}

// ============== Lexicon API ==============

export type LexiconEntry = {
  tag: string;
  zh: string;
  weight?: number;
  category: string;
  subcategory: string;
};

export type CategoryInfo = {
  name: string;
  file: string;
  subcategories: string[];
  tag_count: number;
};

export type LexiconStats = {
  total_tags: number;
  categorized_tags: number;
  uncategorized_tags: number;
  matched_weights: number;
};

export type LexiconIndex = {
  categories: CategoryInfo[];
  stats: LexiconStats;
};

export type CategoryData = {
  name: string;
  subcategories: Record<string, LexiconEntry[]>;
};

export type LexiconSearchResult = {
  entries: LexiconEntry[];
  total: number;
};

export async function fetchLexiconIndex() {
  const { data } = await api.get<LexiconIndex>('/lexicon');
  return data;
}

export async function fetchLexiconCategory(name: string) {
  const { data } = await api.get<CategoryData>(`/lexicon/categories/${encodeURIComponent(name)}`);
  return data;
}

export async function searchLexicon(params: { q: string; limit?: number; offset?: number }) {
  const { data } = await api.get<LexiconSearchResult>('/lexicon/search', { params });
  return data;
}
