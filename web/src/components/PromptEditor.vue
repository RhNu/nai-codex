<script setup lang="ts">
/**
 * PromptEditor - 带权重高亮的提示词编辑器
 *
 * 使用 overlay 方案：
 * - 底层是可编辑的 textarea
 * - 上层是同步显示的高亮层 (backdrop)
 *
 * 高亮规则：
 * - 红色深度代表增强 ({} 或正权重)
 * - 蓝色深度代表削弱 ([])
 * - 绿色代表权重结束符 ::
 * - 紫色代表 snippet 引用
 *
 * 自动补全：
 * - 可选功能，默认关闭
 * - 逗号后的非空白字符触发词库搜索
 * - <snippet 开头触发 snippet 搜索
 * - 选择项目后自动添加逗号空格，方便连续操作
 */
import { ref, watch, computed, nextTick, onMounted, onUnmounted } from 'vue';
import { useDebounceFn } from '@vueuse/core';
import { parsePrompt, formatPrompt, type HighlightSpan } from 'src/services/api';
import { useAutocomplete } from 'src/composables/useAutocomplete';

const props = withDefaults(
  defineProps<{
    modelValue: string;
    label?: string;
    placeholder?: string;
    minHeight?: string;
    disabled?: boolean;
    /** 是否显示自动补全开关，默认显示 */
    showAutocompleteToggle?: boolean;
  }>(),
  {
    label: '',
    placeholder: '',
    minHeight: '80px',
    disabled: false,
    showAutocompleteToggle: true,
  },
);

const emit = defineEmits<{
  (e: 'update:modelValue', value: string): void;
  (e: 'format'): void;
  (e: 'snippet-search'): void;
}>();

const textareaRef = ref<HTMLTextAreaElement | null>(null);
const backdropRef = ref<HTMLDivElement | null>(null);
const containerRef = ref<HTMLDivElement | null>(null);

// 自动补全开关
const autocompleteEnabled = ref(false);

const spans = ref<HighlightSpan[]>([]);
const localValue = ref(props.modelValue);

// 使用自动补全 composable
const {
  showAutocomplete,
  autocompleteItems,
  autocompleteIndex,
  autocompleteMode,
  autocompleteLoading,
  autocompletePosition,
  checkAutocomplete,
  selectItem: selectAutocompleteItem,
  handleKeydown: handleAutocompleteKeydown,
  handleMouseEnter: handleAutocompleteMouseEnter,
  handleClickOutside,
} = useAutocomplete({
  enabled: autocompleteEnabled,
  textareaRef,
  containerRef,
  appendSeparator: true,
  onUpdateValue: (value: string) => {
    localValue.value = value;
    emit('update:modelValue', value);
    void debouncedParse();
  },
});

// 同步外部值
watch(
  () => props.modelValue,
  (val) => {
    if (val !== localValue.value) {
      localValue.value = val;
      void debouncedParse();
    }
  },
);

// 构建 UTF-8 字节偏移到 JS 字符索引的映射表
function buildByteToCharMap(text: string): number[] {
  const encoder = new TextEncoder();
  const bytes = encoder.encode(text);
  const byteToChar: number[] = new Array(bytes.length + 1);

  let byteIndex = 0;
  for (let charIndex = 0; charIndex <= text.length; charIndex++) {
    byteToChar[byteIndex] = charIndex;
    if (charIndex < text.length) {
      const char = text.charCodeAt(charIndex);
      // 计算该字符的 UTF-8 字节数
      if (char < 0x80) {
        byteIndex += 1;
      } else if (char < 0x800) {
        byteIndex += 2;
      } else if (char >= 0xd800 && char <= 0xdbff) {
        // 代理对 (surrogate pair)，跳过
        byteIndex += 4;
        charIndex++; // 跳过低代理
        byteToChar[byteIndex] = charIndex + 1;
      } else {
        byteIndex += 3;
      }
    }
  }

  // 填充任何未设置的索引
  let lastValid = 0;
  for (let i = 0; i <= bytes.length; i++) {
    const val = byteToChar[i];
    if (val !== undefined) {
      lastValid = val;
    } else {
      byteToChar[i] = lastValid;
    }
  }

  return byteToChar;
}

// 解析提示词获取高亮信息
async function parseHighlight() {
  if (!localValue.value) {
    spans.value = [];
    return;
  }
  try {
    const result = await parsePrompt(localValue.value);
    spans.value = result.spans;
  } catch (err) {
    console.warn('Failed to parse prompt:', err);
  }
}

const debouncedParse = useDebounceFn(parseHighlight, 300);

// 输入处理
function onInput(e: Event) {
  const target = e.target as HTMLTextAreaElement;
  localValue.value = target.value;
  emit('update:modelValue', target.value);
  void debouncedParse();

  // 处理自动补全
  void checkAutocomplete(target);
}

// 键盘事件处理
function onKeydown(e: KeyboardEvent) {
  handleAutocompleteKeydown(e);
}

onMounted(() => {
  document.addEventListener('click', handleClickOutside);
});

onUnmounted(() => {
  document.removeEventListener('click', handleClickOutside);
});

// 同步滚动
function syncScroll() {
  if (textareaRef.value && backdropRef.value) {
    backdropRef.value.scrollTop = textareaRef.value.scrollTop;
    backdropRef.value.scrollLeft = textareaRef.value.scrollLeft;
  }
}

// 根据权重计算颜色
function getWeightStyle(span: HighlightSpan): string {
  const { weight, type } = span;

  // 特殊类型的颜色
  if (type === 'weight_end') {
    return 'color: #10b981; font-weight: bold;'; // 绿色 - 权重结束符
  }
  if (type === 'weight_num') {
    // 权重数字根据值着色
    if (weight > 1) {
      const intensity = Math.min((weight - 1) * 2, 1);
      return `color: rgb(${Math.round(220 * intensity + 35)}, ${Math.round(50 * (1 - intensity))}, ${Math.round(50 * (1 - intensity))}); font-weight: bold;`;
    } else if (weight < 1) {
      const intensity = Math.min((1 - weight) * 2, 1);
      return `color: rgb(${Math.round(50 * (1 - intensity))}, ${Math.round(100 * (1 - intensity))}, ${Math.round(220 * intensity + 35)}); font-weight: bold;`;
    }
    return 'font-weight: bold;';
  }
  if (type === 'snippet') {
    return 'color: #8b5cf6; font-weight: bold;'; // 紫色 - snippet
  }
  if (type === 'brace') {
    // {} 括号本身也用红色
    const intensity = Math.min((weight - 1) * 2, 1);
    return `color: rgb(${Math.round(220 * intensity + 35)}, ${Math.round(50 * (1 - intensity))}, ${Math.round(50 * (1 - intensity))});`;
  }
  if (type === 'bracket') {
    // [] 括号本身用蓝色
    const intensity = Math.min((1 - weight) * 2, 1);
    return `color: rgb(${Math.round(50 * (1 - intensity))}, ${Math.round(100 * (1 - intensity))}, ${Math.round(220 * intensity + 35)});`;
  }

  // 普通文本根据权重着色
  if (type === 'text') {
    if (weight > 1) {
      // 增强 - 红色，越强越深
      const intensity = Math.min((weight - 1) * 2, 1);
      return `color: rgb(${Math.round(220 * intensity + 35)}, ${Math.round(50 * (1 - intensity))}, ${Math.round(50 * (1 - intensity))});`;
    } else if (weight < 1) {
      // 减弱 - 蓝色，越弱越深
      const intensity = Math.min((1 - weight) * 2, 1);
      return `color: rgb(${Math.round(50 * (1 - intensity))}, ${Math.round(100 * (1 - intensity))}, ${Math.round(220 * intensity + 35)});`;
    }
  }

  return '';
}

// 生成高亮 HTML
const highlightHtml = computed(() => {
  if (!localValue.value || spans.value.length === 0) {
    // 返回带换行保留的空内容
    return escapeHtml(localValue.value || '') + '\n';
  }

  const text = localValue.value;
  // 构建字节偏移到字符索引的映射
  const byteToChar = buildByteToCharMap(text);

  let html = '';
  let lastCharEnd = 0;

  // 对 spans 按 start 排序
  const sortedSpans = [...spans.value].sort((a, b) => a.start - b.start);

  for (const span of sortedSpans) {
    // 将字节偏移转换为字符索引
    const charStart = byteToChar[span.start] ?? 0;
    const charEnd = byteToChar[span.end] ?? text.length;

    // 跳过重叠的 span
    if (charStart < lastCharEnd) {
      continue;
    }

    // 添加未覆盖的部分
    if (charStart > lastCharEnd) {
      const gapText = text.slice(lastCharEnd, charStart);
      html += `<span style="color: inherit;">${escapeHtml(gapText)}</span>`;
    }

    const spanText = text.slice(charStart, charEnd);
    const style = getWeightStyle(span);

    if (style) {
      html += `<span style="${style}">${escapeHtml(spanText)}</span>`;
    } else {
      html += `<span style="color: inherit;">${escapeHtml(spanText)}</span>`;
    }

    lastCharEnd = charEnd;
  }

  // 添加剩余部分
  if (lastCharEnd < text.length) {
    const remaining = text.slice(lastCharEnd);
    html += `<span style="color: inherit;">${escapeHtml(remaining)}</span>`;
  }

  // 末尾加换行确保高度正确
  html += '\n';

  return html;
});

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;');
}

// 格式化
async function doFormat() {
  try {
    const formatted = await formatPrompt(localValue.value);
    localValue.value = formatted;
    emit('update:modelValue', formatted);
    emit('format');
    await parseHighlight();
  } catch (err) {
    console.error('Format failed:', err);
  }
}

// 自动调整高度
function autoResize() {
  if (textareaRef.value) {
    textareaRef.value.style.height = 'auto';
    textareaRef.value.style.height = textareaRef.value.scrollHeight + 'px';
  }
}

watch(localValue, () => {
  void nextTick(autoResize);
});

onMounted(() => {
  void parseHighlight();
  void nextTick(autoResize);
});

// 暴露方法
defineExpose({
  format: doFormat,
  focus: () => textareaRef.value?.focus(),
});
</script>

<template>
  <div class="prompt-editor" ref="containerRef">
    <label v-if="label" class="prompt-editor__label">{{ label }}</label>
    <div class="prompt-editor__wrapper">
      <!-- 高亮背景层 -->
      <div
        ref="backdropRef"
        class="prompt-editor__backdrop"
        :style="{ minHeight }"
        v-html="highlightHtml"
      />
      <!-- 可编辑的 textarea -->
      <textarea
        ref="textareaRef"
        class="prompt-editor__textarea"
        :value="localValue"
        :placeholder="placeholder"
        :disabled="disabled"
        :style="{ minHeight }"
        @input="onInput"
        @scroll="syncScroll"
        @keydown="onKeydown"
        spellcheck="false"
      />

      <!-- 自动补全弹窗 -->
      <div
        v-if="autocompleteEnabled && showAutocomplete"
        class="prompt-editor__autocomplete"
        :style="{ top: autocompletePosition.top + 'px', left: autocompletePosition.left + 'px' }"
      >
        <div class="autocomplete-header">
          <q-icon
            :name="autocompleteMode === 'snippet' ? 'code' : 'translate'"
            size="xs"
            class="q-mr-xs"
          />
          {{ autocompleteMode === 'snippet' ? 'Snippet' : '词库' }}
          <q-spinner v-if="autocompleteLoading" size="xs" class="q-ml-sm" />
        </div>
        <div class="autocomplete-list">
          <div
            v-for="(item, idx) in autocompleteItems"
            :key="item.tag"
            class="autocomplete-item"
            :class="{ 'autocomplete-item--active': idx === autocompleteIndex }"
            @click="selectAutocompleteItem(item)"
            @mouseenter="handleAutocompleteMouseEnter(idx)"
          >
            <div class="autocomplete-item__label">{{ item.label }}</div>
            <div v-if="item.sublabel" class="autocomplete-item__sublabel">{{ item.sublabel }}</div>
          </div>
        </div>
      </div>
    </div>
    <!-- 工具栏 -->
    <div class="prompt-editor__toolbar">
      <q-toggle
        v-if="showAutocompleteToggle"
        v-model="autocompleteEnabled"
        size="xs"
        dense
        icon="auto_awesome"
        color="secondary"
        class="q-mr-sm"
      >
        <q-tooltip>词库自动补全</q-tooltip>
      </q-toggle>
      <q-btn
        flat
        dense
        size="sm"
        icon="auto_fix_high"
        label="格式化"
        @click="doFormat"
        :disable="disabled"
      />
      <q-btn
        flat
        dense
        size="sm"
        icon="add_box"
        label="Snippet"
        @click="emit('snippet-search')"
        :disable="disabled"
      >
        <q-tooltip>搜索插入 Snippet</q-tooltip>
      </q-btn>
      <slot name="toolbar" />
    </div>
  </div>
</template>

<style scoped lang="scss">
.prompt-editor {
  position: relative;

  &__label {
    display: block;
    font-size: 12px;
    color: rgba(0, 0, 0, 0.6);
    margin-bottom: 4px;
    padding-left: 12px;
  }

  &__wrapper {
    position: relative;
    background: #f5f5f5;
    border-radius: 4px;
    border: 1px solid rgba(0, 0, 0, 0.24);
    transition: border-color 0.2s;

    &:focus-within {
      border-color: var(--q-primary, #1976d2);
      border-width: 2px;
    }
  }

  &__backdrop,
  &__textarea {
    font-family: 'Maple Mono', 'Consolas', 'Monaco', 'Courier New', monospace;
    font-size: 14px;
    line-height: 1.5;
    padding: 12px;
    white-space: pre-wrap;
    word-wrap: break-word;
    overflow-wrap: break-word;
  }

  &__backdrop {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    overflow: hidden;
    pointer-events: none;
    color: #333;
    background: transparent;
  }

  &__textarea {
    position: relative;
    width: 100%;
    resize: vertical;
    border: none;
    background: transparent;
    color: transparent;
    caret-color: #333;
    outline: none;

    &::placeholder {
      color: rgba(0, 0, 0, 0.4);
    }

    // 让光标和选区可见
    &::selection {
      background: rgba(25, 118, 210, 0.3);
    }
  }

  &__toolbar {
    display: flex;
    gap: 8px;
    margin-top: 4px;
    padding-left: 4px;
  }

  &__autocomplete {
    position: absolute;
    z-index: 100;
    min-width: 250px;
    max-width: 400px;
    max-height: 300px;
    background: white;
    border: 1px solid rgba(0, 0, 0, 0.12);
    border-radius: 4px;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
    overflow: hidden;

    .autocomplete-header {
      display: flex;
      align-items: center;
      padding: 6px 10px;
      font-size: 11px;
      color: #666;
      background: #f5f5f5;
      border-bottom: 1px solid rgba(0, 0, 0, 0.08);
    }

    .autocomplete-list {
      max-height: 250px;
      overflow-y: auto;
    }

    .autocomplete-item {
      padding: 8px 12px;
      cursor: pointer;
      transition: background 0.15s;

      &:hover,
      &--active {
        background: #e3f2fd;
      }

      &__label {
        font-family: 'Consolas', 'Monaco', monospace;
        font-size: 13px;
        color: #333;
      }

      &__sublabel {
        font-size: 11px;
        color: #888;
        margin-top: 2px;
      }
    }
  }
}

// 暗色模式支持
.body--dark {
  .prompt-editor {
    &__label {
      color: rgba(255, 255, 255, 0.7);
    }

    &__wrapper {
      background: #1e1e1e;
      border-color: rgba(255, 255, 255, 0.24);

      &:focus-within {
        border-color: var(--q-primary, #1976d2);
      }
    }

    &__backdrop {
      color: #e0e0e0;
    }

    &__textarea {
      caret-color: #e0e0e0;

      &::placeholder {
        color: rgba(255, 255, 255, 0.4);
      }
    }

    &__autocomplete {
      background: #2d2d2d;
      border-color: rgba(255, 255, 255, 0.12);

      .autocomplete-header {
        background: #383838;
        color: #aaa;
        border-color: rgba(255, 255, 255, 0.08);
      }

      .autocomplete-item {
        &:hover,
        &--active {
          background: #3d3d3d;
        }

        &__label {
          color: #e0e0e0;
        }

        &__sublabel {
          color: #888;
        }
      }
    }
  }
}
</style>
