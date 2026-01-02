/**
 * useGalleryNavigation - 画廊图片导航逻辑
 *
 * 功能：
 * - 键盘左右键切换图片
 * - 支持循环/非循环模式
 */
import { computed, type Ref } from 'vue';
import { useEventListener } from '@vueuse/core';

export interface UseGalleryNavigationOptions<T> {
  /** 当前选中的项目 */
  currentItem: Ref<T | null>;
  /** 所有项目列表 */
  items: Ref<T[]>;
  /** 对话框是否打开 */
  isDialogOpen: Ref<boolean>;
  /** 获取项目唯一标识 */
  getItemId: (item: T) => string;
  /** 是否循环导航（首尾相连），默认 true */
  loop?: boolean;
}

export interface UseGalleryNavigationReturn {
  /** 是否有上一项 */
  hasPrev: Ref<boolean>;
  /** 是否有下一项 */
  hasNext: Ref<boolean>;
  /** 当前索引 */
  currentIndex: Ref<number>;
  /** 总数 */
  total: Ref<number>;
  /** 导航到上一项 */
  goToPrev: () => void;
  /** 导航到下一项 */
  goToNext: () => void;
  /** 导航到指定索引 */
  goToIndex: (index: number) => void;
}

export function useGalleryNavigation<T>(
  options: UseGalleryNavigationOptions<T>,
): UseGalleryNavigationReturn {
  const { currentItem, items, isDialogOpen, getItemId, loop = true } = options;

  const currentIndex = computed(() => {
    if (!currentItem.value) return -1;
    const currentId = getItemId(currentItem.value);
    return items.value.findIndex((item) => getItemId(item) === currentId);
  });

  const total = computed(() => items.value.length);

  const hasPrev = computed(() => {
    if (total.value === 0) return false;
    if (loop) return total.value > 1;
    return currentIndex.value > 0;
  });

  const hasNext = computed(() => {
    if (total.value === 0) return false;
    if (loop) return total.value > 1;
    return currentIndex.value < total.value - 1;
  });

  function goToPrev(): void {
    if (!hasPrev.value || total.value === 0) return;

    let newIndex: number;
    if (currentIndex.value <= 0) {
      newIndex = loop ? total.value - 1 : 0;
    } else {
      newIndex = currentIndex.value - 1;
    }

    const item = items.value[newIndex];
    if (item) currentItem.value = item;
  }

  function goToNext(): void {
    if (!hasNext.value || total.value === 0) return;

    let newIndex: number;
    if (currentIndex.value >= total.value - 1) {
      newIndex = loop ? 0 : total.value - 1;
    } else {
      newIndex = currentIndex.value + 1;
    }

    const item = items.value[newIndex];
    if (item) currentItem.value = item;
  }

  function goToIndex(index: number): void {
    if (index >= 0 && index < total.value) {
      const item = items.value[index];
      if (item) currentItem.value = item;
    }
  }

  // 监听键盘事件
  useEventListener('keydown', (event: KeyboardEvent) => {
    if (!isDialogOpen.value) return;

    // 避免在输入框中触发
    const target = event.target as HTMLElement;
    if (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA') return;

    switch (event.key) {
      case 'ArrowLeft':
        event.preventDefault();
        goToPrev();
        break;
      case 'ArrowRight':
        event.preventDefault();
        goToNext();
        break;
      case 'Home':
        event.preventDefault();
        goToIndex(0);
        break;
      case 'End':
        event.preventDefault();
        goToIndex(total.value - 1);
        break;
    }
  });

  return {
    hasPrev,
    hasNext,
    currentIndex,
    total,
    goToPrev,
    goToNext,
    goToIndex,
  };
}
