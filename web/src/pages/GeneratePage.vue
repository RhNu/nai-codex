<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue';
import { useQuasar } from 'quasar';
import { useDebounceFn } from '@vueuse/core';
import { useTaskStore } from 'src/stores/tasks';
import {
  fetchPresets,
  fetchPreset,
  fetchMainPresets,
  loadGenerationSettings,
  saveGenerationSettings,
  dryRunPrompt,
  type CharacterPrompt,
  type GenerationParams,
  type Preset,
  type MainPreset,
  type MainPresetSettings,
  type DryRunResult,
} from 'src/services/api';
import { parseImageMetadata, type ParsedMetadata } from 'src/composables';
import PromptEditor from 'src/components/PromptEditor.vue';
import PromptSuggester from 'src/components/PromptSuggester.vue';

// 尺寸预设
const sizePresets = [
  { label: '小型(纵向) 512×768', value: '512x768' },
  { label: '小型(横向) 768×512', value: '768x512' },
  { label: '小型(方形) 640×640', value: '640x640' },
  { label: '中型(纵向) 832×1216', value: '832x1216' },
  { label: '中型(横向) 1216×832', value: '1216x832' },
  { label: '中型(方形) 1024×1024', value: '1024x1024' },
  { label: '大型(纵向) 1024×1536', value: '1024x1536' },
  { label: '大型(横向) 1536×1024', value: '1536x1024' },
  { label: '大型(方形) 1472×1472', value: '1472x1472' },
  { label: '壁纸(纵向) 1088×1920', value: '1088x1920' },
  { label: '壁纸(横向) 1920×1088', value: '1920x1088' },
  { label: '自定义', value: 'custom' },
];

// 采样器选项
const samplerOptions = [
  { label: 'Euler', value: 'k_euler' },
  { label: 'Euler Ancestral', value: 'k_euler_ancestral' },
  { label: 'DPM++ 2S Ancestral', value: 'k_dpmpp_2s_ancestral' },
  { label: 'DPM++ 2M', value: 'k_dpmpp_2m' },
  { label: 'DPM++ SDE', value: 'k_dpmpp_sde' },
  { label: 'DPM++ 2M SDE', value: 'k_dpmpp_2m_sde' },
  { label: 'DDIM V3', value: 'ddim_v3' },
];

// 噪声调度选项
const noiseOptions = [
  { label: 'Native', value: 'native' },
  { label: 'Karras', value: 'karras' },
  { label: 'Exponential', value: 'exponential' },
  { label: 'PolyExponential', value: 'polyexponential' },
];

// 模型选项
const modelOptions = [
  { label: 'NAI Diffusion V4.5 Full', value: 'nai-diffusion-4-5-full' },
  { label: 'NAI Diffusion V4.5 Curated', value: 'nai-diffusion-4-5-curated' },
];

// UC预设选项 (根据模型不同有所不同)
const ucPresetOptionsAll = computed(() => {
  if (model.value === 'nai-diffusion-4-5-curated') {
    return [
      { label: 'Heavy', value: 0, icon: 'shield', color: 'red' },
      { label: 'Light', value: 1, icon: 'security', color: 'orange' },
      { label: 'Human Focus', value: 2, icon: 'person', color: 'blue' },
      { label: 'None', value: 3, icon: 'block', color: 'grey' },
    ];
  }
  return [
    { label: 'Heavy', value: 0, icon: 'shield', color: 'red' },
    { label: 'Light', value: 1, icon: 'security', color: 'orange' },
    { label: 'Furry Focus', value: 2, icon: 'pets', color: 'brown' },
    { label: 'Human Focus', value: 3, icon: 'person', color: 'blue' },
    { label: 'None', value: 4, icon: 'block', color: 'grey' },
  ];
});

const selectedUcPresetInfo = computed(() => {
  const preset = ucPresetOptionsAll.value.find((p) => p.value === ucPreset.value);
  return preset || { label: 'Heavy', icon: 'shield', color: 'red' };
});

// 表单状态
const prompt = ref('');
const negative = ref('');
const count = ref(1);
const sizePreset = ref('832x1216');
const width = ref(832);
const height = ref(1216);
const steps = ref(28);
const scale = ref(5.0);
const cfgRescale = ref(0);
const seedInput = ref<string>('');
const sampler = ref('k_euler_ancestral');
const noise = ref('karras');
const model = ref('nai-diffusion-4-5-full');
const addQualityTags = ref(true);
const ucPreset = ref<number | null>(0);
const varietyPlus = ref(false);

// Snippet 搜索面板
const showSnippetPanel = ref(false);
const showSnippetPanelForNegative = ref(false);
const showSnippetPanelForCharacter = ref<number | null>(null); // 角色槽索引
const promptEditorRef = ref<InstanceType<typeof PromptEditor> | null>(null);

// 图片元数据导入
const showImportDialog = ref(false);
const importMetadata = ref<ParsedMetadata | null>(null);
const importPreviewUrl = ref<string | null>(null);
const importLoading = ref(false);
const isDragOver = ref(false);
const importFileInputRef = ref<HTMLInputElement | null>(null);

// 导入选项
const importOptions = ref({
  positivePrompt: true,
  negativePrompt: true,
  params: false,
});

type CharacterSlot = {
  prompt: string;
  uc: string;
  enabled: boolean;
  preset_id: string | null;
};

const characterSlots = ref<CharacterSlot[]>([]);
const presetOptions = ref<Array<{ label: string; value: string }>>([]);
const presetsCache = ref<Map<string, Preset>>(new Map());

const presetOptionsWithNone = computed(() => [
  { label: '无预设', value: null },
  ...presetOptions.value,
]);

// 主预设选项和状态
const mainPresetOptions = ref<Array<{ label: string; value: string }>>([]);
const mainPresetsCache = ref<Map<string, MainPreset>>(new Map());
const selectedMainPresetId = ref<string | null>(null);
const mainPresetsLoading = ref(false);

const mainPresetOptionsWithNone = computed(() => [
  { label: '无预设', value: null },
  ...mainPresetOptions.value,
]);

// 当前选中的主预设（用于 dry-run）
const currentMainPresetSettings = computed((): MainPresetSettings => {
  if (!selectedMainPresetId.value) return {};
  const preset = mainPresetsCache.value.get(selectedMainPresetId.value);
  if (!preset) return {};
  return {
    before: preset.before ?? null,
    after: preset.after ?? null,
    replace: preset.replace ?? null,
    uc_before: preset.uc_before ?? null,
    uc_after: preset.uc_after ?? null,
    uc_replace: preset.uc_replace ?? null,
  };
});

// Dry-run 对话框
const showDryRunDialog = ref(false);
const dryRunResult = ref<DryRunResult | null>(null);
const dryRunLoading = ref(false);

const $q = useQuasar();
const taskStore = useTaskStore();
const loading = ref(false);
const settingsLoaded = ref(false);
const presetsLoading = ref(false);

// 计算整体页面是否锁定（在关键异步操作期间）
const isLocked = computed(() => loading.value || !settingsLoaded.value);

// 监听尺寸预设变化
watch(sizePreset, (val) => {
  if (val && val !== 'custom') {
    const [w, h] = val.split('x').map(Number);
    if (w && h) {
      width.value = w;
      height.value = h;
    }
  }
});

// 监听宽高变化，检测是否匹配预设
watch([width, height], ([w, h]) => {
  const match = sizePresets.find((p) => p.value === `${w}x${h}`);
  if (match) {
    sizePreset.value = match.value;
  } else {
    sizePreset.value = 'custom';
  }
});

// 主预设是否有选择
const hasMainPreset = computed(() => !!selectedMainPresetId.value);

// 执行 Dry-run
async function executeDryRun() {
  dryRunLoading.value = true;
  try {
    const result = await dryRunPrompt({
      raw_positive: prompt.value,
      raw_negative: negative.value,
      main_preset: currentMainPresetSettings.value,
      character_slots: characterSlots.value.map((s) => ({
        prompt: s.prompt,
        uc: s.uc,
        enabled: s.enabled,
        preset_id: s.preset_id,
      })),
    });
    dryRunResult.value = result;
    showDryRunDialog.value = true;
  } catch (err) {
    console.error('Dry-run failed:', err);
    $q.notify({ type: 'negative', message: 'Dry-run 失败' });
  } finally {
    dryRunLoading.value = false;
  }
}

function addCharacterSlot() {
  if (characterSlots.value.length < 6) {
    characterSlots.value.push({ prompt: '', uc: '', enabled: true, preset_id: null });
  }
}

function removeCharacterSlot(idx: number) {
  characterSlots.value.splice(idx, 1);
}

// 构建角色提示词，应用预设
async function buildCharacterPrompts(): Promise<CharacterPrompt[]> {
  const results: CharacterPrompt[] = [];

  for (const slot of characterSlots.value) {
    // 如果未启用，跳过
    if (!slot.enabled) continue;
    // 如果没有输入提示词且没有预设，跳过
    if (!slot.prompt.trim() && !slot.preset_id) continue;

    let finalPrompt = slot.prompt.trim();
    let finalUc = slot.uc.trim();

    // 如果有预设，先获取预设并应用
    if (slot.preset_id) {
      let preset = presetsCache.value.get(slot.preset_id);
      if (!preset) {
        try {
          preset = await fetchPreset(slot.preset_id);
          presetsCache.value.set(slot.preset_id, preset);
        } catch (e) {
          console.warn('Failed to fetch preset:', e);
        }
      }

      if (preset) {
        // 应用预设逻辑 - 正向提示词
        if (preset.replace) {
          finalPrompt = preset.replace;
        } else {
          if (preset.before) {
            finalPrompt = preset.before + ', ' + finalPrompt;
          }
          if (preset.after) {
            finalPrompt = finalPrompt + ', ' + preset.after;
          }
        }

        // 应用预设逻辑 - 负面提示词
        if (preset.uc_replace) {
          finalUc = preset.uc_replace;
        } else {
          if (preset.uc_before && finalUc) {
            finalUc = preset.uc_before + ', ' + finalUc;
          } else if (preset.uc_before) {
            finalUc = preset.uc_before;
          }
          if (preset.uc_after && finalUc) {
            finalUc = finalUc + ', ' + preset.uc_after;
          } else if (preset.uc_after) {
            finalUc = preset.uc_after;
          }
        }
      }
    }

    results.push({
      prompt: finalPrompt,
      uc: finalUc,
      enabled: true,
    });
  }

  return results;
}

// 插入 snippet 到指定目标
function insertSnippet(target: 'prompt' | 'negative' | number, snippetTag: string) {
  if (target === 'prompt') {
    if (prompt.value) {
      if (!prompt.value.trim().endsWith(',')) {
        prompt.value = prompt.value.trim() + ', ';
      }
      prompt.value += snippetTag;
    } else {
      prompt.value = snippetTag;
    }
  } else if (target === 'negative') {
    if (negative.value) {
      if (!negative.value.trim().endsWith(',')) {
        negative.value = negative.value.trim() + ', ';
      }
      negative.value += snippetTag;
    } else {
      negative.value = snippetTag;
    }
  } else if (typeof target === 'number' && characterSlots.value[target]) {
    const slot = characterSlots.value[target];
    if (slot.prompt) {
      if (!slot.prompt.trim().endsWith(',')) {
        slot.prompt = slot.prompt.trim() + ', ';
      }
      slot.prompt += snippetTag;
    } else {
      slot.prompt = snippetTag;
    }
  }
}

// 当从 Snippet 面板选择时
function onSnippetSelect(snippetTag: string) {
  if (showSnippetPanelForNegative.value) {
    insertSnippet('negative', snippetTag);
    showSnippetPanelForNegative.value = false;
  } else if (showSnippetPanelForCharacter.value !== null) {
    insertSnippet(showSnippetPanelForCharacter.value, snippetTag);
    showSnippetPanelForCharacter.value = null;
  } else {
    insertSnippet('prompt', snippetTag);
    showSnippetPanel.value = false;
  }
}

// ========== 图片元数据导入 ==========

// 处理图片文件解析
async function processImportFile(file: File) {
  if (!file.type.startsWith('image/')) {
    $q.notify({ type: 'negative', message: '请上传图片文件' });
    return;
  }

  importLoading.value = true;
  try {
    // 创建预览 URL
    if (importPreviewUrl.value) {
      URL.revokeObjectURL(importPreviewUrl.value);
    }
    importPreviewUrl.value = URL.createObjectURL(file);

    // 解析元数据
    const metadata = await parseImageMetadata(file);
    if (metadata) {
      importMetadata.value = metadata;
      showImportDialog.value = true;
    } else {
      $q.notify({ type: 'warning', message: '未能解析到图片元数据' });
    }
  } catch (err) {
    console.error('解析图片失败:', err);
    $q.notify({ type: 'negative', message: '解析图片失败' });
  } finally {
    importLoading.value = false;
  }
}

// 选择导入文件
function selectImportFile() {
  importFileInputRef.value?.click();
}

// 处理文件变化
async function handleImportFileChange(event: Event) {
  const input = event.target as HTMLInputElement;
  const file = input.files?.[0];
  if (file) {
    await processImportFile(file);
  }
  input.value = '';
}

// 处理页面拖放
function handleDragOver(event: DragEvent) {
  event.preventDefault();
  isDragOver.value = true;
}

function handleDragLeave(event: DragEvent) {
  // 只有真正离开页面时才取消高亮
  const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
  const x = event.clientX;
  const y = event.clientY;
  if (x < rect.left || x >= rect.right || y < rect.top || y >= rect.bottom) {
    isDragOver.value = false;
  }
}

async function handleDrop(event: DragEvent) {
  event.preventDefault();
  isDragOver.value = false;

  const file = event.dataTransfer?.files[0];
  if (file && file.type.startsWith('image/')) {
    await processImportFile(file);
  }
}

// 应用导入的元数据
function applyImportedMetadata() {
  if (!importMetadata.value) return;

  const m = importMetadata.value;

  // 导入正向提示词
  if (importOptions.value.positivePrompt && m.positivePrompt) {
    prompt.value = m.positivePrompt;
  }

  // 导入负向提示词
  if (importOptions.value.negativePrompt && m.negativePrompt) {
    negative.value = m.negativePrompt;
  }

  // 导入参数
  if (importOptions.value.params) {
    if (m.steps) steps.value = Number(m.steps) || steps.value;
    if (m.cfg) scale.value = Number(m.cfg) || scale.value;
    if (m.dimensions) {
      width.value = m.dimensions.width;
      height.value = m.dimensions.height;
    }
  }

  $q.notify({ type: 'positive', message: '已导入图片参数' });
  closeImportDialog();
}

// 导入到角色槽
function importToCharacterSlot(slotIndex: number) {
  if (!importMetadata.value?.positivePrompt) return;

  // 确保槽位存在
  while (characterSlots.value.length <= slotIndex) {
    characterSlots.value.push({ prompt: '', uc: '', enabled: true, preset_id: null });
  }

  const slot = characterSlots.value[slotIndex];
  if (!slot) return;

  if (importOptions.value.positivePrompt && importMetadata.value.positivePrompt) {
    slot.prompt = importMetadata.value.positivePrompt;
  }
  if (importOptions.value.negativePrompt && importMetadata.value.negativePrompt) {
    slot.uc = importMetadata.value.negativePrompt;
  }

  $q.notify({ type: 'positive', message: `已导入到角色 ${slotIndex + 1}` });
  closeImportDialog();
}

// 关闭导入对话框
function closeImportDialog() {
  showImportDialog.value = false;
  importMetadata.value = null;
  if (importPreviewUrl.value) {
    URL.revokeObjectURL(importPreviewUrl.value);
    importPreviewUrl.value = null;
  }
}

async function loadPresets() {
  presetsLoading.value = true;
  try {
    const res = await fetchPresets({ limit: 50, offset: 0 });
    presetOptions.value = res.items.map((p) => ({ label: p.name, value: p.id }));
  } catch (err) {
    console.error(err);
  } finally {
    presetsLoading.value = false;
  }
}

async function loadMainPresets() {
  mainPresetsLoading.value = true;
  try {
    const res = await fetchMainPresets({ limit: 50, offset: 0 });
    mainPresetOptions.value = res.items.map((p) => ({ label: p.name, value: p.id }));
    // 缓存所有主预设
    for (const p of res.items) {
      mainPresetsCache.value.set(p.id, p);
    }
  } catch (err) {
    console.error(err);
  } finally {
    mainPresetsLoading.value = false;
  }
}

// 收集当前设置用于保存
function collectCurrentSettings() {
  const params: GenerationParams = {
    model: model.value,
    width: width.value,
    height: height.value,
    steps: steps.value,
    scale: scale.value,
    sampler: sampler.value,
    noise: noise.value,
    cfg_rescale: cfgRescale.value,
    add_quality_tags: addQualityTags.value,
    undesired_content_preset: ucPreset.value,
    variety_plus: varietyPlus.value,
  };

  return {
    prompt: prompt.value,
    negative_prompt: negative.value,
    count: count.value,
    params,
    character_slots: characterSlots.value.map((s) => ({
      prompt: s.prompt,
      uc: s.uc,
      enabled: s.enabled,
      preset_id: s.preset_id,
    })),
    main_preset_id: selectedMainPresetId.value,
  };
}

// 应用加载的设置
function applySettings(settings: {
  prompt: string;
  negative_prompt: string;
  count: number;
  params: GenerationParams;
  character_slots?: Array<{
    prompt: string;
    uc: string;
    enabled: boolean;
    preset_id: string | null;
  }>;
  main_preset_id?: string | null;
}) {
  prompt.value = settings.prompt || '';
  negative.value = settings.negative_prompt || '';
  count.value = settings.count || 1;

  if (settings.params) {
    const p = settings.params;
    model.value = p.model || 'nai-diffusion-4-5-full';
    width.value = p.width || 832;
    height.value = p.height || 1216;
    steps.value = p.steps || 28;
    scale.value = p.scale || 5.0;
    sampler.value = p.sampler || 'k_euler_ancestral';
    noise.value = p.noise || 'karras';
    cfgRescale.value = p.cfg_rescale || 0;
    addQualityTags.value = p.add_quality_tags !== false;
    ucPreset.value = p.undesired_content_preset ?? 0;
    varietyPlus.value = p.variety_plus || false;
  }

  // 恢复角色提示词
  if (settings.character_slots && settings.character_slots.length > 0) {
    characterSlots.value = settings.character_slots.map((s) => ({
      prompt: s.prompt,
      uc: s.uc,
      enabled: s.enabled,
      preset_id: s.preset_id,
    }));
  } else {
    characterSlots.value = [];
  }

  // 恢复主预设ID
  selectedMainPresetId.value = settings.main_preset_id || null;
}

// 保存当前设置到后端（防抖）
const debouncedSaveSettings = useDebounceFn(() => {
  if (!settingsLoaded.value) return;
  void saveGenerationSettings(collectCurrentSettings()).catch((err) => {
    console.warn('Failed to save settings:', err);
  });
}, 2000);

// 监听设置变化
watch(
  [
    prompt,
    negative,
    count,
    model,
    width,
    height,
    steps,
    scale,
    sampler,
    noise,
    cfgRescale,
    addQualityTags,
    ucPreset,
    varietyPlus,
    characterSlots,
    selectedMainPresetId,
  ],
  () => {
    void debouncedSaveSettings();
  },
  { deep: true },
);

async function submit() {
  loading.value = true;
  try {
    const params: GenerationParams = {
      model: model.value,
      width: width.value,
      height: height.value,
      steps: steps.value,
      scale: scale.value,
      sampler: sampler.value,
      noise: noise.value,
      cfg_rescale: cfgRescale.value,
      add_quality_tags: addQualityTags.value,
      undesired_content_preset: ucPreset.value,
      variety_plus: varietyPlus.value,
    };

    // 处理seed
    const seedVal = seedInput.value.trim();
    if (seedVal) {
      const parsedSeed = parseInt(seedVal, 10);
      if (!isNaN(parsedSeed) && parsedSeed > 0) {
        params.seed = parsedSeed;
      }
    }

    // 构建角色提示词
    const chars = await buildCharacterPrompts();
    if (chars.length > 0) {
      params.character_prompts = chars;
    }

    await taskStore.submit({
      raw_prompt: prompt.value,
      negative_prompt: negative.value,
      count: count.value,
      params,
      // 主提示词预设由后端处理
      main_preset: currentMainPresetSettings.value,
      title: prompt.value.slice(0, 32) || '任务',
    });
    $q.notify({ type: 'positive', message: '任务已提交' });

    // 提交后立即保存设置
    try {
      await saveGenerationSettings(collectCurrentSettings());
    } catch {
      // ignore
    }
  } catch (err) {
    console.error(err);
    $q.notify({ type: 'negative', message: '提交失败' });
  } finally {
    loading.value = false;
  }
}

onMounted(() => {
  void loadPresets();
  void loadMainPresets();

  // 加载上次的设置
  void loadGenerationSettings()
    .then((settings) => {
      applySettings(settings);
    })
    .catch((err) => {
      console.warn('Failed to load settings:', err);
    })
    .finally(() => {
      settingsLoaded.value = true;
    });
});

onUnmounted(() => {
  // 离开页面时同步保存一次
  saveGenerationSettings(collectCurrentSettings()).catch(() => {});
});
</script>

<template>
  <q-page
    padding
    class="generate-page"
    @dragover="handleDragOver"
    @dragleave="handleDragLeave"
    @drop="handleDrop"
  >
    <!-- 全局拖放提示遮罩 -->
    <div v-if="isDragOver" class="drop-overlay">
      <div class="drop-overlay-content">
        <q-icon name="cloud_upload" size="4rem" color="white" />
        <div class="text-h5 text-white q-mt-md">拖放图片以导入参数</div>
      </div>
    </div>

    <!-- 隐藏的文件输入 -->
    <input
      ref="importFileInputRef"
      type="file"
      accept="image/*"
      style="display: none"
      @change="handleImportFileChange"
    />

    <q-card class="generate-card">
      <q-card-section class="q-pb-none">
        <div class="row items-center justify-between">
          <div class="text-h5">图像生成</div>
          <q-btn
            flat
            dense
            icon="image_search"
            label="导入图片"
            color="primary"
            @click="selectImportFile"
          >
            <q-tooltip>从图片导入生成参数</q-tooltip>
          </q-btn>
        </div>
      </q-card-section>

      <q-card-section class="q-pt-sm">
        <q-select
          v-model="model"
          :options="modelOptions"
          label="模型"
          emit-value
          map-options
          filled
          dense
          class="model-select"
        />
      </q-card-section>

      <q-card-section class="q-pt-none">
        <!-- 主要提示词 -->
        <div class="row items-center justify-between">
          <div class="section-title">提示词</div>
          <div class="row items-center q-gutter-sm">
            <q-checkbox
              v-model="addQualityTags"
              size="sm"
              dense
              checked-icon="auto_fix_normal"
              unchecked-icon="auto_fix_off"
            >
              <q-tooltip>自动添加质量标签</q-tooltip>
            </q-checkbox>
          </div>
        </div>

        <PromptEditor
          ref="promptEditorRef"
          v-model="prompt"
          label="正向提示词"
          placeholder="输入正向提示词..."
          min-height="80px"
          @snippet-search="showSnippetPanel = true"
        />

        <PromptEditor
          v-model="negative"
          label="反向提示词 (UC)"
          placeholder="输入反向提示词..."
          min-height="60px"
          class="q-mt-sm"
          @snippet-search="showSnippetPanelForNegative = true"
        >
          <template #toolbar>
            <q-btn-dropdown
              flat
              dense
              no-caps
              dropdown-icon="expand_more"
              :color="selectedUcPresetInfo.color"
              class="uc-preset-btn"
            >
              <template #label>
                <q-icon :name="selectedUcPresetInfo.icon" size="sm" class="q-mr-xs" />
                <span class="uc-preset-label">{{ selectedUcPresetInfo.label }}</span>
              </template>
              <q-list dense>
                <q-item-label header>UC预设</q-item-label>
                <q-item
                  v-for="opt in ucPresetOptionsAll"
                  :key="opt.value"
                  clickable
                  v-close-popup
                  @click="ucPreset = opt.value"
                  :active="ucPreset === opt.value"
                >
                  <q-item-section avatar>
                    <q-icon :name="opt.icon" :color="opt.color" />
                  </q-item-section>
                  <q-item-section>{{ opt.label }}</q-item-section>
                  <q-item-section side v-if="ucPreset === opt.value">
                    <q-icon name="check" color="primary" />
                  </q-item-section>
                </q-item>
              </q-list>
            </q-btn-dropdown>
          </template>
        </PromptEditor>

        <!-- 基础参数 -->
        <div class="section-title q-mt-md">基础参数</div>

        <div class="row q-col-gutter-sm">
          <div class="col-6 col-sm-4 col-md-2">
            <q-input
              v-model.number="count"
              type="number"
              min="1"
              max="8"
              label="数量"
              filled
              dense
            />
          </div>
          <div class="col-6 col-sm-8 col-md-4">
            <q-select
              v-model="sizePreset"
              :options="sizePresets"
              label="尺寸预设"
              emit-value
              map-options
              filled
              dense
            />
          </div>
          <div class="col-6 col-sm-6 col-md-3">
            <q-input v-model.number="width" type="number" label="宽度" filled dense />
          </div>
          <div class="col-6 col-sm-6 col-md-3">
            <q-input v-model.number="height" type="number" label="高度" filled dense />
          </div>
        </div>

        <!-- 高级参数（可折叠） -->
        <q-expansion-item
          label="高级参数"
          class="q-mt-md advanced-params"
          dense
          header-class="section-title"
          default-closed
        >
          <div class="q-pt-sm">
            <div class="row q-col-gutter-sm">
              <div class="col-6 col-sm-4 col-md-2">
                <q-input v-model.number="steps" type="number" label="Steps" filled dense />
              </div>
              <div class="col-6 col-sm-4 col-md-2">
                <q-input
                  v-model.number="scale"
                  type="number"
                  step="0.1"
                  label="CFG Scale"
                  filled
                  dense
                />
              </div>
              <div class="col-6 col-sm-4 col-md-2">
                <q-input
                  v-model.number="cfgRescale"
                  type="number"
                  step="0.01"
                  label="CFG Rescale"
                  filled
                  dense
                />
              </div>
              <div class="col-6 col-sm-6 col-md-3">
                <q-input v-model="seedInput" label="Seed (留空随机)" filled dense clearable />
              </div>
              <div class="col-6 col-sm-6 col-md-3">
                <q-select
                  v-model="sampler"
                  :options="samplerOptions"
                  label="采样器"
                  emit-value
                  map-options
                  filled
                  dense
                />
              </div>
              <div class="col-6 col-sm-6 col-md-3">
                <q-select
                  v-model="noise"
                  :options="noiseOptions"
                  label="噪声调度"
                  emit-value
                  map-options
                  filled
                  dense
                />
              </div>
              <div class="col-12 col-sm-6 col-md-3">
                <q-toggle v-model="varietyPlus" label="Variety+" color="primary" dense />
                <q-tooltip>启用 Variety+ 模式，增加生成结果的多样性</q-tooltip>
              </div>
            </div>
          </div>
        </q-expansion-item>
      </q-card-section>

      <!-- 角色提示词 -->
      <q-card-section class="q-pt-none">
        <div class="row items-center justify-between">
          <div class="section-title">角色提示词</div>
          <q-btn
            icon="add"
            label="添加角色"
            color="primary"
            flat
            dense
            size="sm"
            @click="addCharacterSlot"
            :disable="characterSlots.length >= 6"
          />
        </div>

        <q-banner dense class="bg-blue-1 text-grey-8 q-mt-sm" v-if="characterSlots.length === 0">
          <template #avatar>
            <q-icon name="info" color="primary" />
          </template>
          点击"添加角色"来为图像添加角色提示词。每个角色可以单独设置预设。
        </q-banner>

        <div class="column q-gutter-sm q-mt-sm">
          <q-card
            v-for="(slot, idx) in characterSlots"
            :key="idx"
            flat
            bordered
            class="character-card"
          >
            <q-card-section class="q-pa-sm">
              <div class="row items-center q-gutter-sm q-mb-sm">
                <q-chip
                  dense
                  :color="slot.enabled ? 'primary' : 'grey'"
                  text-color="white"
                  size="sm"
                >
                  角色 {{ idx + 1 }}
                </q-chip>
                <q-toggle v-model="slot.enabled" dense size="sm" color="primary" />
                <q-space />
                <q-select
                  v-model="slot.preset_id"
                  :options="presetOptionsWithNone"
                  label="预设"
                  emit-value
                  map-options
                  dense
                  filled
                  class="col-grow"
                  style="max-width: 200px"
                  clearable
                />
                <q-btn
                  icon="delete"
                  flat
                  dense
                  round
                  color="negative"
                  size="sm"
                  @click="removeCharacterSlot(idx)"
                />
              </div>
              <PromptEditor
                v-model="slot.prompt"
                label="角色正向提示词"
                min-height="40px"
                :disabled="!slot.enabled"
                @snippet-search="showSnippetPanelForCharacter = idx"
              />
              <PromptEditor
                v-model="slot.uc"
                label="角色反向提示词 (可选)"
                min-height="30px"
                class="q-mt-xs"
                :disabled="!slot.enabled"
                @snippet-search="showSnippetPanelForCharacter = idx"
              />
            </q-card-section>
          </q-card>
        </div>
      </q-card-section>

      <q-card-actions align="right" class="q-px-md q-pb-md">
        <q-select
          v-model="selectedMainPresetId"
          :options="mainPresetOptionsWithNone"
          label="主预设"
          emit-value
          map-options
          dense
          filled
          clearable
          style="min-width: 180px"
          :loading="mainPresetsLoading"
        >
          <template #prepend>
            <q-icon name="tune" :color="hasMainPreset ? 'primary' : 'grey'" />
          </template>
        </q-select>
        <q-btn
          flat
          color="info"
          label="预览"
          icon="visibility"
          :loading="dryRunLoading"
          @click="executeDryRun"
        >
          <q-tooltip>预览最终提示词 (Dry-run)</q-tooltip>
        </q-btn>
        <q-btn
          color="primary"
          label="提交任务"
          icon="send"
          :loading="loading"
          :disable="isLocked"
          @click="submit"
        />
      </q-card-actions>
    </q-card>

    <!-- Snippet 搜索面板 -->
    <PromptSuggester
      v-model:show-panel="showSnippetPanel"
      @select="onSnippetSelect"
      @update:show-panel="
        (v) => {
          if (!v) showSnippetPanel = false;
        }
      "
    />
    <PromptSuggester
      v-model:show-panel="showSnippetPanelForNegative"
      @select="onSnippetSelect"
      @update:show-panel="
        (v) => {
          if (!v) showSnippetPanelForNegative = false;
        }
      "
    />
    <PromptSuggester
      :show-panel="showSnippetPanelForCharacter !== null"
      @select="onSnippetSelect"
      @update:show-panel="
        (v) => {
          if (!v) showSnippetPanelForCharacter = null;
        }
      "
    />

    <!-- 图片元数据导入对话框 -->
    <q-dialog v-model="showImportDialog" persistent>
      <q-card style="min-width: 500px; max-width: 90vw">
        <q-card-section class="row items-center">
          <div class="text-h6">导入图片参数</div>
          <q-space />
          <q-btn icon="close" flat round dense @click="closeImportDialog" />
        </q-card-section>

        <q-separator />

        <q-card-section v-if="importMetadata" class="q-gutter-md">
          <!-- 图片预览和基本信息 -->
          <div class="row q-gutter-md">
            <q-img
              v-if="importPreviewUrl"
              :src="importPreviewUrl"
              fit="contain"
              class="import-preview"
            />
            <div class="col">
              <q-chip
                :color="importMetadata.generator === 'NovelAI' ? 'purple' : 'primary'"
                text-color="white"
                size="md"
              >
                {{ importMetadata.generator }}
              </q-chip>
              <div v-if="importMetadata.dimensions" class="text-caption text-grey-7 q-mt-sm">
                尺寸: {{ importMetadata.dimensions.width }}×{{ importMetadata.dimensions.height }}
              </div>
              <div v-if="importMetadata.model" class="text-caption text-grey-7">
                模型: {{ importMetadata.model }}
              </div>
              <div v-if="importMetadata.steps" class="text-caption text-grey-7">
                步数: {{ importMetadata.steps }} | CFG: {{ importMetadata.cfg }}
              </div>
            </div>
          </div>

          <!-- 提示词预览 -->
          <div v-if="importMetadata.positivePrompt">
            <div class="text-subtitle2">正向提示词</div>
            <div class="import-prompt-preview">{{ importMetadata.positivePrompt }}</div>
          </div>

          <div v-if="importMetadata.negativePrompt">
            <div class="text-subtitle2">负向提示词</div>
            <div class="import-prompt-preview negative">{{ importMetadata.negativePrompt }}</div>
          </div>

          <q-separator />

          <!-- 导入选项 -->
          <div class="text-subtitle2">导入选项</div>
          <div class="row q-gutter-md">
            <q-checkbox
              v-model="importOptions.positivePrompt"
              label="正向提示词"
              :disable="!importMetadata.positivePrompt"
            />
            <q-checkbox
              v-model="importOptions.negativePrompt"
              label="负向提示词"
              :disable="!importMetadata.negativePrompt"
            />
            <q-checkbox
              v-model="importOptions.params"
              label="生成参数"
              :disable="!importMetadata.steps"
            />
          </div>
        </q-card-section>

        <q-separator />

        <q-card-actions align="right" class="q-pa-md">
          <!-- 导入到角色槽 -->
          <q-btn-dropdown
            flat
            label="导入到角色槽"
            icon="person_add"
            :disable="!importMetadata?.positivePrompt"
          >
            <q-list dense>
              <q-item
                v-for="i in 6"
                :key="i"
                clickable
                v-close-popup
                @click="importToCharacterSlot(i - 1)"
              >
                <q-item-section avatar>
                  <q-icon name="person" />
                </q-item-section>
                <q-item-section>
                  角色 {{ i }}
                  <span v-if="characterSlots[i - 1]?.prompt" class="text-grey-6">（已有内容）</span>
                </q-item-section>
              </q-item>
            </q-list>
          </q-btn-dropdown>

          <q-btn flat label="取消" @click="closeImportDialog" />
          <q-btn
            color="primary"
            label="导入到主提示词"
            icon="download"
            @click="applyImportedMetadata"
          />
        </q-card-actions>
      </q-card>
    </q-dialog>

    <q-inner-loading :showing="importLoading">
      <q-spinner-gears size="3rem" color="primary" />
    </q-inner-loading>

    <!-- Dry-run 结果对话框 -->
    <q-dialog v-model="showDryRunDialog" maximized>
      <q-card class="dry-run-dialog">
        <q-card-section class="row items-center q-pb-sm">
          <div class="text-h6">提示词预览 (Dry-run)</div>
          <q-space />
          <q-btn flat round dense icon="close" v-close-popup />
        </q-card-section>

        <q-separator />

        <q-card-section v-if="dryRunResult" class="dry-run-content q-pa-md">
          <!-- 正面提示词处理链 -->
          <div class="text-subtitle1 q-mb-sm">
            <q-icon name="add_circle" color="positive" class="q-mr-xs" />
            正面提示词处理链
          </div>

          <div class="process-chain q-mb-md">
            <div class="chain-step">
              <div class="step-label">原始输入</div>
              <div class="step-content">{{ dryRunResult.raw_positive || '(空)' }}</div>
            </div>
            <q-icon name="arrow_downward" color="grey" class="chain-arrow" />
            <div class="chain-step" v-if="hasMainPreset">
              <div class="step-label">应用主预设后</div>
              <div class="step-content">{{ dryRunResult.positive_after_preset || '(空)' }}</div>
            </div>
            <q-icon v-if="hasMainPreset" name="arrow_downward" color="grey" class="chain-arrow" />
            <div class="chain-step final">
              <div class="step-label">最终结果 (Snippet 展开后)</div>
              <div class="step-content">{{ dryRunResult.final_positive || '(空)' }}</div>
            </div>
          </div>

          <q-separator class="q-my-md" />

          <!-- 负面提示词处理链 -->
          <div class="text-subtitle1 q-mb-sm">
            <q-icon name="remove_circle" color="negative" class="q-mr-xs" />
            负面提示词处理链
          </div>

          <div class="process-chain q-mb-md">
            <div class="chain-step">
              <div class="step-label">原始输入</div>
              <div class="step-content negative">{{ dryRunResult.raw_negative || '(空)' }}</div>
            </div>
            <q-icon name="arrow_downward" color="grey" class="chain-arrow" />
            <div class="chain-step" v-if="hasMainPreset">
              <div class="step-label">应用主预设后</div>
              <div class="step-content negative">
                {{ dryRunResult.negative_after_preset || '(空)' }}
              </div>
            </div>
            <q-icon v-if="hasMainPreset" name="arrow_downward" color="grey" class="chain-arrow" />
            <div class="chain-step final">
              <div class="step-label">最终结果 (Snippet 展开后)</div>
              <div class="step-content negative">{{ dryRunResult.final_negative || '(空)' }}</div>
            </div>
          </div>

          <!-- 角色提示词 -->
          <template v-if="dryRunResult.character_prompts?.length">
            <q-separator class="q-my-md" />
            <div class="text-subtitle1 q-mb-sm">
              <q-icon name="people" color="info" class="q-mr-xs" />
              角色提示词 ({{ dryRunResult.character_prompts.length }} 个)
            </div>

            <div
              v-for="(char, idx) in dryRunResult.character_prompts"
              :key="idx"
              class="character-result q-mb-md"
            >
              <div class="text-caption text-weight-medium q-mb-xs">角色 {{ idx + 1 }}</div>
              <div class="row q-col-gutter-sm">
                <div class="col-12 col-md-6">
                  <div class="step-label">正面提示词</div>
                  <div class="step-content">{{ char.final_prompt || '(空)' }}</div>
                </div>
                <div class="col-12 col-md-6">
                  <div class="step-label">负面提示词</div>
                  <div class="step-content negative">{{ char.final_uc || '(空)' }}</div>
                </div>
              </div>
            </div>
          </template>
        </q-card-section>

        <q-separator />

        <q-card-actions align="right" class="q-pa-md">
          <q-btn flat label="关闭" v-close-popup />
        </q-card-actions>
      </q-card>
    </q-dialog>
  </q-page>
</template>

<style scoped lang="scss">
.generate-page {
  max-width: 1200px;
  margin: 0 auto;
  position: relative;
}

.drop-overlay {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: rgba(var(--q-primary-rgb), 0.85);
  z-index: 9999;
  display: flex;
  align-items: center;
  justify-content: center;
  pointer-events: none;
}

.drop-overlay-content {
  text-align: center;
}

.generate-card {
  width: 100%;
}

.model-select {
  max-width: 320px;
}

.section-title {
  font-size: 0.95rem;
  font-weight: 500;
  color: var(--q-dark);
  margin-bottom: 8px;
}

.uc-preset-btn {
  min-width: 100px;
}

.uc-preset-label {
  font-size: 0.85rem;
}

.advanced-params :deep(.q-expansion-item__content) {
  padding-top: 0;
}

.character-card {
  background: rgba(0, 0, 0, 0.02);
}

.import-preview {
  width: 120px;
  height: 120px;
  border-radius: 8px;
  flex-shrink: 0;
}

.import-prompt-preview {
  background: var(--q-grey-2);
  padding: 8px 12px;
  border-radius: 6px;
  font-family: 'Maple Mono', monospace;
  font-size: 0.8rem;
  line-height: 1.4;
  max-height: 120px;
  overflow-y: auto;
  white-space: pre-wrap;
  word-break: break-word;
  border-left: 3px solid var(--q-positive);

  &.negative {
    border-left-color: var(--q-negative);
  }
}

// Dry-run 对话框样式
.dry-run-dialog {
  max-width: 900px;
  width: 100%;
  margin: 0 auto;
}

.dry-run-content {
  max-height: calc(100vh - 150px);
  overflow-y: auto;
}

.process-chain {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.chain-step {
  background: var(--q-grey-2);
  border-radius: 8px;
  padding: 12px;

  &.final {
    background: rgba(var(--q-primary-rgb), 0.1);
    border: 1px solid var(--q-primary);
  }
}

.step-label {
  font-size: 0.75rem;
  color: var(--q-grey-7);
  margin-bottom: 4px;
  font-weight: 500;
}

.step-content {
  font-family: 'Maple Mono', monospace;
  font-size: 0.85rem;
  line-height: 1.5;
  white-space: pre-wrap;
  word-break: break-word;
  border-left: 3px solid var(--q-positive);
  padding-left: 8px;

  &.negative {
    border-left-color: var(--q-negative);
  }
}

.chain-arrow {
  align-self: center;
  opacity: 0.5;
}

.character-result {
  background: var(--q-grey-1);
  border-radius: 8px;
  padding: 12px;
}
</style>
