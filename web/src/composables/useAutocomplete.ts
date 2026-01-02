/**
 * useAutocomplete - 提示词编辑器自动补全逻辑
 *
 * 功能：
 * - 词库自动补全：根据输入搜索词库
 * - Snippet自动补全：<snippet: 开头触发snippet搜索
 * - 自动插入逗号空格：选择项目后自动添加 ", " 方便连续操作
 */
import { ref, computed, type Ref } from 'vue';
import { useDebounceFn } from '@vueuse/core';
import {
  searchLexicon,
  fetchSnippets,
  type LexiconEntry,
  type SnippetSummary,
} from 'src/services/api';

export interface AutocompleteItem {
  type: 'lexicon' | 'snippet';
  tag: string;
  label: string;
  sublabel?: string;
}

export interface UseAutocompleteOptions {
  /** 自动补全开关 */
  enabled: Ref<boolean>;
  /** textarea引用 */
  textareaRef: Ref<HTMLTextAreaElement | null>;
  /** 容器引用（用于计算弹窗位置） */
  containerRef: Ref<HTMLElement | null>;
  /** 选择项目后插入逗号空格，默认true */
  appendSeparator?: boolean;
  /** 词库搜索防抖时间，默认200ms */
  debounceMs?: number;
  /** 更新值的回调 */
  onUpdateValue?: (value: string) => void;
}

export function useAutocomplete(options: UseAutocompleteOptions) {
  const {
    enabled,
    textareaRef,
    containerRef,
    appendSeparator = true,
    debounceMs = 200,
    onUpdateValue,
  } = options;

  // 自动补全状态
  const showAutocomplete = ref(false);
  const autocompleteItems = ref<AutocompleteItem[]>([]);
  const autocompleteIndex = ref(0);
  const autocompleteQuery = ref('');
  const autocompleteStart = ref(0);
  const autocompleteMode = ref<'lexicon' | 'snippet'>('lexicon');
  const autocompleteLoading = ref(false);
  const autocompletePosition = ref({ top: 0, left: 0 });

  // 计算属性
  const hasItems = computed(() => autocompleteItems.value.length > 0);
  const currentItem = computed(() => autocompleteItems.value[autocompleteIndex.value]);

  /**
   * 搜索自动补全
   */
  async function searchAutocomplete(query: string, mode: 'lexicon' | 'snippet') {
    if (!query && mode === 'lexicon') {
      showAutocomplete.value = false;
      return;
    }

    autocompleteLoading.value = true;
    try {
      if (mode === 'lexicon') {
        const result = await searchLexicon({ q: query, limit: 10 });
        autocompleteItems.value = result.entries.map((e: LexiconEntry) => ({
          type: 'lexicon' as const,
          tag: e.tag,
          label: e.tag,
          sublabel: e.zh,
        }));
      } else {
        const params: { q?: string; limit: number } = { limit: 10 };
        if (query) {
          params.q = query;
        }
        const result = await fetchSnippets(params);
        autocompleteItems.value = result.items.map((s: SnippetSummary) => ({
          type: 'snippet' as const,
          tag: `<snippet:${s.name}>`,
          label: s.name,
          sublabel: s.description || s.category,
        }));
      }

      autocompleteIndex.value = 0;
      showAutocomplete.value = autocompleteItems.value.length > 0;
    } catch (err) {
      console.warn('Autocomplete search failed:', err);
      showAutocomplete.value = false;
    } finally {
      autocompleteLoading.value = false;
    }
  }

  // 防抖搜索词库
  const debouncedLexiconSearch = useDebounceFn(async (query: string) => {
    await searchAutocomplete(query, 'lexicon');
  }, debounceMs);

  /**
   * 计算自动补全弹窗位置
   */
  function updateAutocompletePosition(textarea: HTMLTextAreaElement, start: number) {
    const rect = textarea.getBoundingClientRect();
    const containerRect = containerRef.value?.getBoundingClientRect();
    if (!containerRect) return;

    // 估算光标位置
    const lineHeight = 21; // 14px * 1.5
    const textBefore = textarea.value.slice(0, start);
    const lines = textBefore.split('\n');
    const currentLine = lines.length - 1;

    autocompletePosition.value = {
      top: Math.min((currentLine + 1) * lineHeight + 12, rect.height - 200),
      left: 12,
    };
  }

  /**
   * 检查是否需要触发自动补全
   */
  async function checkAutocomplete(textarea: HTMLTextAreaElement) {
    if (!enabled.value) return;

    const cursorPos = textarea.selectionStart;
    const text = textarea.value;

    // 向前找到当前词的起始位置（逗号、换行或开头）
    let start = cursorPos;
    while (start > 0 && text.charAt(start - 1) !== ',' && text.charAt(start - 1) !== '\n') {
      start--;
    }
    // 跳过前导空格
    while (start < cursorPos && /\s/.test(text.charAt(start))) {
      start++;
    }

    const currentWord = text.slice(start, cursorPos);

    // 检查是否是 snippet 模式
    if (currentWord.startsWith('<snippet:') || currentWord.startsWith('<snippet')) {
      autocompleteMode.value = 'snippet';
      const snippetQuery = currentWord.replace(/^<snippet:?/, '');
      autocompleteQuery.value = snippetQuery;
      autocompleteStart.value = start;
      await searchAutocomplete(snippetQuery, 'snippet');
      updateAutocompletePosition(textarea, start);
      return;
    }

    // 词库模式：需要至少一个字符
    if (currentWord.length >= 1) {
      autocompleteMode.value = 'lexicon';
      autocompleteQuery.value = currentWord;
      autocompleteStart.value = start;
      await debouncedLexiconSearch(currentWord);
      updateAutocompletePosition(textarea, start);
      return;
    }

    // 隐藏自动补全
    showAutocomplete.value = false;
  }

  /**
   * 选择自动补全项
   */
  function selectItem(item: AutocompleteItem) {
    const textarea = textareaRef.value;
    if (!textarea) return;

    const text = textarea.value;
    const cursorPos = textarea.selectionStart;

    // 计算插入内容
    let insertText = item.tag;

    // 自动添加逗号空格（如果启用且后续不是逗号）
    if (appendSeparator) {
      const afterCursor = text.slice(cursorPos).trimStart();
      // 如果后面没有内容，或者后面不是以逗号开头，则添加逗号空格
      if (!afterCursor || !afterCursor.startsWith(',')) {
        insertText += ', ';
      }
    }

    // 替换当前词
    const newValue = text.slice(0, autocompleteStart.value) + insertText + text.slice(cursorPos);

    // 调用更新回调
    onUpdateValue?.(newValue);

    // 移动光标到插入内容之后
    const newCursorPos = autocompleteStart.value + insertText.length;
    requestAnimationFrame(() => {
      textarea.setSelectionRange(newCursorPos, newCursorPos);
      textarea.focus();
    });

    showAutocomplete.value = false;
  }

  /**
   * 选择当前高亮项
   */
  function selectCurrentItem() {
    const item = currentItem.value;
    if (item) {
      selectItem(item);
    }
  }

  /**
   * 键盘导航处理
   * @returns 是否处理了事件（用于阻止默认行为）
   */
  function handleKeydown(e: KeyboardEvent): boolean {
    if (!showAutocomplete.value) return false;

    const itemsLength = autocompleteItems.value.length;
    if (itemsLength === 0) return false;

    switch (e.key) {
      case 'ArrowDown':
        e.preventDefault();
        autocompleteIndex.value = (autocompleteIndex.value + 1) % itemsLength;
        return true;

      case 'ArrowUp':
        e.preventDefault();
        autocompleteIndex.value = (autocompleteIndex.value - 1 + itemsLength) % itemsLength;
        return true;

      case 'Enter':
      case 'Tab':
        e.preventDefault();
        selectCurrentItem();
        return true;

      case 'Escape':
        showAutocomplete.value = false;
        return true;
    }

    return false;
  }

  /**
   * 鼠标悬停选择
   */
  function handleMouseEnter(index: number) {
    autocompleteIndex.value = index;
  }

  /**
   * 隐藏自动补全
   */
  function hide() {
    showAutocomplete.value = false;
  }

  /**
   * 点击外部关闭
   */
  function handleClickOutside(e: MouseEvent) {
    if (containerRef.value && !containerRef.value.contains(e.target as Node)) {
      hide();
    }
  }

  return {
    // 状态
    showAutocomplete,
    autocompleteItems,
    autocompleteIndex,
    autocompleteQuery,
    autocompleteMode,
    autocompleteLoading,
    autocompletePosition,

    // 计算属性
    hasItems,
    currentItem,

    // 方法
    checkAutocomplete,
    selectItem,
    selectCurrentItem,
    handleKeydown,
    handleMouseEnter,
    handleClickOutside,
    hide,
  };
}
