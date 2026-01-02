/**
 * useImageUpload - 图片上传逻辑
 *
 * 功能：
 * - 选择/拖拽上传图片
 * - 图片预览
 * - 转换为 base64 格式
 * - 删除图片
 */
import { ref, computed, type Ref } from 'vue';

export interface UseImageUploadOptions {
  /** 最大文件大小（字节），默认 5MB */
  maxSize?: number;
  /** 接受的文件类型 */
  accept?: string[];
  /** 初始预览 URL */
  initialPreviewUrl?: Ref<string | null | undefined>;
}

export interface UseImageUploadReturn {
  /** 预览 URL */
  previewUrl: Ref<string | null>;
  /** Base64 编码（不含前缀） */
  base64Data: Ref<string | null>;
  /** 是否有图片 */
  hasImage: Ref<boolean>;
  /** 是否正在加载 */
  loading: Ref<boolean>;
  /** 错误信息 */
  error: Ref<string | null>;
  /** 选择文件 */
  selectFile: () => void;
  /** 处理文件变化（用于 input 事件） */
  handleFileChange: (event: Event) => Promise<void>;
  /** 处理拖放 */
  handleDrop: (event: DragEvent) => Promise<void>;
  /** 处理粘贴 */
  handlePaste: (event: ClipboardEvent) => Promise<void>;
  /** 清除图片 */
  clearImage: () => void;
  /** 重置为初始状态 */
  reset: () => void;
  /** 文件输入元素引用 */
  fileInputRef: Ref<HTMLInputElement | null>;
}

export function useImageUpload(options: UseImageUploadOptions = {}): UseImageUploadReturn {
  const { maxSize = 5 * 1024 * 1024, accept = ['image/png', 'image/jpeg', 'image/webp'] } = options;

  const fileInputRef = ref<HTMLInputElement | null>(null);
  const previewUrl = ref<string | null>(options.initialPreviewUrl?.value ?? null);
  const base64Data = ref<string | null>(null);
  const loading = ref(false);
  const error = ref<string | null>(null);

  const hasImage = computed(() => !!previewUrl.value);

  /**
   * 读取文件为 Base64
   */
  function readFileAsBase64(file: File): Promise<string> {
    return new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => {
        const result = reader.result as string;
        // 移除 data:xxx;base64, 前缀
        const base64 = result.split(',')[1] ?? '';
        resolve(base64);
      };
      reader.onerror = () => reject(new Error('读取文件失败'));
      reader.readAsDataURL(file);
    });
  }

  /**
   * 处理单个文件
   */
  async function processFile(file: File): Promise<void> {
    error.value = null;

    // 验证文件类型
    if (!accept.includes(file.type)) {
      error.value = `不支持的文件类型，请上传 ${accept.map((t) => t.split('/')[1]).join('/')} 格式`;
      return;
    }

    // 验证文件大小
    if (file.size > maxSize) {
      error.value = `文件过大，请上传小于 ${Math.round(maxSize / 1024 / 1024)}MB 的图片`;
      return;
    }

    loading.value = true;
    try {
      // 创建预览 URL
      previewUrl.value = URL.createObjectURL(file);
      // 读取 base64
      base64Data.value = await readFileAsBase64(file);
    } catch (e) {
      error.value = e instanceof Error ? e.message : '处理图片失败';
      previewUrl.value = null;
      base64Data.value = null;
    } finally {
      loading.value = false;
    }
  }

  /**
   * 选择文件
   */
  function selectFile(): void {
    fileInputRef.value?.click();
  }

  /**
   * 处理 input change 事件
   */
  async function handleFileChange(event: Event): Promise<void> {
    const input = event.target as HTMLInputElement;
    const file = input.files?.[0];
    if (file) {
      await processFile(file);
    }
    // 清空 input 以便重复选择同一文件
    input.value = '';
  }

  /**
   * 处理拖放
   */
  async function handleDrop(event: DragEvent): Promise<void> {
    event.preventDefault();
    const file = event.dataTransfer?.files[0];
    if (file) {
      await processFile(file);
    }
  }

  /**
   * 处理粘贴
   */
  async function handlePaste(event: ClipboardEvent): Promise<void> {
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

  /**
   * 清除图片
   */
  function clearImage(): void {
    if (previewUrl.value && previewUrl.value.startsWith('blob:')) {
      URL.revokeObjectURL(previewUrl.value);
    }
    previewUrl.value = null;
    base64Data.value = null;
    error.value = null;
  }

  /**
   * 重置为初始状态
   */
  function reset(): void {
    clearImage();
    previewUrl.value = options.initialPreviewUrl?.value ?? null;
  }

  return {
    previewUrl,
    base64Data,
    hasImage,
    loading,
    error,
    selectFile,
    handleFileChange,
    handleDrop,
    handlePaste,
    clearImage,
    reset,
    fileInputRef,
  };
}
