<script setup lang="ts">
import { ref, computed } from 'vue';
import { useQuasar } from 'quasar';
import { useClipboard } from '@vueuse/core';
import { useImageMetadata } from 'src/composables';

const $q = useQuasar();
const { copy } = useClipboard();

// 图片元数据解析
const { metadata, loading, error, parseFile, clear, hasMetadata } = useImageMetadata();

// 图片预览
const previewUrl = ref<string | null>(null);
const isDragOver = ref(false);

// 文件输入引用
const fileInputRef = ref<HTMLInputElement | null>(null);

// 是否显示原始数据
const showRawData = ref(false);

// 生成器类型图标
const generatorIcon = computed(() => {
  switch (metadata.value?.generator) {
    case 'NovelAI':
      return 'auto_awesome';
    case 'ComfyUI':
      return 'hub';
    case 'Stable Diffusion':
      return 'image';
    case 'Midjourney':
      return 'palette';
    case 'Illustrious XL':
      return 'star';
    default:
      return 'help_outline';
  }
});

// 生成器类型颜色
const generatorColor = computed(() => {
  switch (metadata.value?.generator) {
    case 'NovelAI':
      return 'purple';
    case 'ComfyUI':
      return 'orange';
    case 'Stable Diffusion':
      return 'blue';
    case 'Midjourney':
      return 'cyan';
    case 'Illustrious XL':
      return 'amber';
    default:
      return 'grey';
  }
});

// 处理文件
async function processFile(file: File) {
  if (!file.type.startsWith('image/')) {
    $q.notify({ type: 'negative', message: '请上传图片文件' });
    return;
  }

  // 创建预览 URL
  if (previewUrl.value) {
    URL.revokeObjectURL(previewUrl.value);
  }
  previewUrl.value = URL.createObjectURL(file);

  // 解析元数据
  await parseFile(file);
}

// 选择文件
function selectFile() {
  fileInputRef.value?.click();
}

// 处理文件变化
async function handleFileChange(event: Event) {
  const input = event.target as HTMLInputElement;
  const file = input.files?.[0];
  if (file) {
    await processFile(file);
  }
  input.value = '';
}

// 处理拖放
function handleDragOver(event: DragEvent) {
  event.preventDefault();
  isDragOver.value = true;
}

function handleDragLeave() {
  isDragOver.value = false;
}

async function handleDrop(event: DragEvent) {
  event.preventDefault();
  isDragOver.value = false;

  const file = event.dataTransfer?.files[0];
  if (file) {
    await processFile(file);
  }
}

// 处理粘贴
async function handlePaste(event: ClipboardEvent) {
  const items = event.clipboardData?.items;
  if (!items) return;

  for (const item of items) {
    if (item.type.startsWith('image/')) {
      const file = item.getAsFile();
      if (file) {
        await processFile(file);
        break;
      }
    }
  }
}

// 复制文本
async function copyText(text: string, label: string) {
  try {
    await copy(text);
    $q.notify({ type: 'positive', message: `已复制${label}`, timeout: 1500 });
  } catch {
    $q.notify({ type: 'negative', message: '复制失败' });
  }
}

// 复制所有参数
async function copyAllParams() {
  if (!metadata.value) return;

  const m = metadata.value;
  const lines: string[] = [];

  if (m.generator) lines.push(`生成器: ${m.generator}`);
  if (m.model) lines.push(`模型: ${m.model}`);
  if (m.dimensions) lines.push(`尺寸: ${m.dimensions.width}×${m.dimensions.height}`);
  if (m.steps) lines.push(`步数: ${m.steps}`);
  if (m.cfg) lines.push(`CFG: ${m.cfg}`);
  if (m.seed) lines.push(`种子: ${m.seed}`);
  if (m.sampler) lines.push(`采样器: ${m.sampler}`);
  if (m.scheduler) lines.push(`调度器: ${m.scheduler}`);
  if (m.positivePrompt) lines.push(`\n正向提示词:\n${m.positivePrompt}`);
  if (m.negativePrompt) lines.push(`\n负向提示词:\n${m.negativePrompt}`);

  await copyText(lines.join('\n'), '所有参数');
}

// 清除
function clearAll() {
  clear();
  if (previewUrl.value) {
    URL.revokeObjectURL(previewUrl.value);
    previewUrl.value = null;
  }
}

// 格式化 JSON
function formatRawData(): string {
  if (!metadata.value?.rawTags) return '{}';
  try {
    return JSON.stringify(metadata.value.rawTags, null, 2);
  } catch {
    return '{}';
  }
}
</script>

<template>
  <q-page padding class="metadata-page" @paste="handlePaste" tabindex="0">
    <div class="page-header">
      <div class="text-h5">图片元数据解析</div>
      <div class="text-caption text-grey-6">
        支持 NovelAI、ComfyUI、Stable Diffusion、Midjourney、Illustrious XL 等格式
      </div>
    </div>

    <!-- 上传区域 -->
    <q-card class="upload-card q-mb-md">
      <input
        ref="fileInputRef"
        type="file"
        accept="image/*"
        style="display: none"
        @change="handleFileChange"
      />

      <div
        class="upload-zone"
        :class="{ 'drag-over': isDragOver, 'has-image': previewUrl }"
        @click="selectFile"
        @dragover="handleDragOver"
        @dragleave="handleDragLeave"
        @drop="handleDrop"
      >
        <template v-if="!previewUrl">
          <q-icon name="cloud_upload" size="4rem" color="grey-5" />
          <div class="text-h6 text-grey-6 q-mt-md">点击上传或拖放图片</div>
          <div class="text-caption text-grey-5">也可以按 Ctrl+V 粘贴图片</div>
        </template>

        <template v-else>
          <q-img :src="previewUrl" fit="contain" class="preview-image" />
          <q-btn round flat icon="close" color="negative" class="clear-btn" @click.stop="clearAll">
            <q-tooltip>清除</q-tooltip>
          </q-btn>
        </template>

        <q-inner-loading :showing="loading">
          <q-spinner-gears size="3rem" color="primary" />
        </q-inner-loading>
      </div>
    </q-card>

    <!-- 错误提示 -->
    <q-banner v-if="error" dense class="bg-negative text-white q-mb-md">
      <template #avatar>
        <q-icon name="error" />
      </template>
      {{ error }}
    </q-banner>

    <!-- 解析结果 -->
    <q-card v-if="hasMetadata && metadata" class="result-card">
      <q-card-section>
        <div class="row items-center justify-between">
          <div class="row items-center q-gutter-sm">
            <q-chip :color="generatorColor" text-color="white" :icon="generatorIcon" size="md">
              {{ metadata.generator }}
            </q-chip>
            <q-chip v-if="metadata.dimensions" outline>
              {{ metadata.dimensions.width }}×{{ metadata.dimensions.height }}
            </q-chip>
          </div>
          <q-btn flat dense icon="content_copy" label="复制全部" @click="copyAllParams" />
        </div>
      </q-card-section>

      <q-separator />

      <!-- 基本参数 -->
      <q-card-section v-if="metadata.model || metadata.steps || metadata.cfg || metadata.seed">
        <div class="text-subtitle2 q-mb-sm">生成参数</div>
        <div class="params-grid">
          <div v-if="metadata.model" class="param-item">
            <div class="param-label">模型</div>
            <div class="param-value">
              {{ metadata.model }}
              <q-btn
                flat
                round
                dense
                size="xs"
                icon="content_copy"
                @click="copyText(String(metadata.model), '模型')"
              />
            </div>
          </div>
          <div v-if="metadata.steps" class="param-item">
            <div class="param-label">步数</div>
            <div class="param-value">{{ metadata.steps }}</div>
          </div>
          <div v-if="metadata.cfg" class="param-item">
            <div class="param-label">CFG</div>
            <div class="param-value">{{ metadata.cfg }}</div>
          </div>
          <div v-if="metadata.seed" class="param-item">
            <div class="param-label">种子</div>
            <div class="param-value">
              {{ metadata.seed }}
              <q-btn
                flat
                round
                dense
                size="xs"
                icon="content_copy"
                @click="copyText(String(metadata.seed), '种子')"
              />
            </div>
          </div>
          <div v-if="metadata.sampler" class="param-item">
            <div class="param-label">采样器</div>
            <div class="param-value">{{ metadata.sampler }}</div>
          </div>
          <div v-if="metadata.scheduler" class="param-item">
            <div class="param-label">调度器</div>
            <div class="param-value">{{ metadata.scheduler }}</div>
          </div>
        </div>
      </q-card-section>

      <q-separator v-if="metadata.positivePrompt" />

      <!-- 正向提示词 -->
      <q-card-section v-if="metadata.positivePrompt">
        <div class="row items-center justify-between q-mb-sm">
          <div class="text-subtitle2">正向提示词</div>
          <q-btn
            flat
            dense
            size="sm"
            icon="content_copy"
            label="复制"
            @click="copyText(metadata.positivePrompt!, '正向提示词')"
          />
        </div>
        <div class="prompt-text positive">{{ metadata.positivePrompt }}</div>
      </q-card-section>

      <q-separator v-if="metadata.negativePrompt" />

      <!-- 负向提示词 -->
      <q-card-section v-if="metadata.negativePrompt">
        <div class="row items-center justify-between q-mb-sm">
          <div class="text-subtitle2">负向提示词</div>
          <q-btn
            flat
            dense
            size="sm"
            icon="content_copy"
            label="复制"
            @click="copyText(metadata.negativePrompt!, '负向提示词')"
          />
        </div>
        <div class="prompt-text negative">{{ metadata.negativePrompt }}</div>
      </q-card-section>

      <q-separator />

      <!-- 原始数据 -->
      <q-expansion-item
        v-model="showRawData"
        label="原始元数据"
        icon="data_object"
        header-class="text-grey-7"
      >
        <q-card-section class="q-pt-none">
          <pre class="raw-data">{{ formatRawData() }}</pre>
        </q-card-section>
      </q-expansion-item>
    </q-card>

    <!-- 空状态 -->
    <q-card v-else-if="!loading" class="empty-state-card">
      <q-card-section class="text-center">
        <q-icon name="image_search" size="4rem" color="grey-4" />
        <div class="text-h6 text-grey-6 q-mt-md">上传图片以解析元数据</div>
        <div class="text-body2 text-grey-5 q-mt-sm">支持从各种 AI 图像生成工具导出的图片</div>
      </q-card-section>
    </q-card>
  </q-page>
</template>

<style scoped lang="scss">
.metadata-page {
  max-width: 900px;
  margin: 0 auto;

  &:focus {
    outline: none;
  }
}

.page-header {
  margin-bottom: 16px;
}

.upload-card {
  overflow: hidden;
}

.upload-zone {
  position: relative;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  min-height: 200px;
  padding: 24px;
  border: 2px dashed var(--q-grey-4);
  border-radius: 8px;
  cursor: pointer;
  transition: all 0.2s ease;
  margin: 16px;

  &:hover,
  &.drag-over {
    border-color: var(--q-primary);
    background: rgba(var(--q-primary-rgb), 0.05);
  }

  &.has-image {
    min-height: 300px;
    max-height: 500px;
    border-style: solid;
    border-color: var(--q-grey-3);
  }
}

.preview-image {
  max-width: 100%;
  max-height: 450px;
  border-radius: 4px;
}

.clear-btn {
  position: absolute;
  top: 8px;
  right: 8px;
  background: rgba(255, 255, 255, 0.9);
}

.result-card {
  margin-bottom: 16px;
}

.params-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
  gap: 12px;
}

.param-item {
  background: var(--q-grey-2);
  padding: 8px 12px;
  border-radius: 6px;
}

.param-label {
  font-size: 0.75rem;
  color: var(--q-grey-7);
  margin-bottom: 2px;
}

.param-value {
  font-size: 0.9rem;
  font-weight: 500;
  display: flex;
  align-items: center;
  gap: 4px;
  word-break: break-all;
}

.prompt-text {
  background: var(--q-grey-2);
  padding: 12px;
  border-radius: 6px;
  font-family: 'Maple Mono', monospace;
  font-size: 0.85rem;
  line-height: 1.5;
  white-space: pre-wrap;
  word-break: break-word;
  max-height: 300px;
  overflow-y: auto;

  &.positive {
    border-left: 3px solid var(--q-positive);
  }

  &.negative {
    border-left: 3px solid var(--q-negative);
  }
}

.raw-data {
  background: var(--q-dark);
  color: #e0e0e0;
  padding: 12px;
  border-radius: 6px;
  font-family: 'Maple Mono', monospace;
  font-size: 0.75rem;
  line-height: 1.4;
  overflow-x: auto;
  max-height: 400px;
  margin: 0;
}

.empty-state-card {
  padding: 48px 24px;
}
</style>
