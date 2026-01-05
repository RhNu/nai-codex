/**
 * useImageTools - 图片处理工具
 *
 * 功能：
 * - 下载图片
 * - 去除图片元数据
 * - 复制图片到剪贴板
 */

export interface UseImageToolsReturn {
  /** 获取图片为 Blob */
  fetchImageAsBlob: (url: string) => Promise<Blob>;
  /** 去除图片元数据 */
  removeMetadata: (blob: Blob) => Promise<Blob>;
  /** 转换为 JPG 格式 */
  convertToJpg: (blob: Blob, quality?: number) => Promise<Blob>;
  /** 下载 Blob 为文件 */
  downloadBlob: (blob: Blob, filename: string) => void;
  /** 下载图片原图 */
  downloadImage: (url: string, filename: string) => Promise<void>;
  /** 下载去除元数据的图片 */
  downloadImageClean: (url: string, filename: string) => Promise<void>;
  /** 下载图片为 JPG 格式 */
  downloadImageAsJpg: (url: string, filename: string, quality?: number) => Promise<void>;
  /** 复制图片到剪贴板（去除元数据） */
  copyImageToClipboard: (url: string) => Promise<void>;
}

export function useImageTools(): UseImageToolsReturn {
  /**
   * 获取图片为 Blob
   */
  async function fetchImageAsBlob(url: string): Promise<Blob> {
    const response = await fetch(url);
    return response.blob();
  }

  /**
   * 去除图片元数据（通过 canvas 重绘）
   */
  function removeMetadata(blob: Blob): Promise<Blob> {
    return new Promise((resolve, reject) => {
      const img = new Image();
      img.onload = () => {
        const canvas = document.createElement('canvas');
        canvas.width = img.width;
        canvas.height = img.height;
        const ctx = canvas.getContext('2d');
        if (!ctx) {
          reject(new Error('Failed to get canvas context'));
          return;
        }
        ctx.drawImage(img, 0, 0);
        canvas.toBlob(
          (newBlob) => {
            if (newBlob) {
              resolve(newBlob);
            } else {
              reject(new Error('Failed to create blob'));
            }
          },
          'image/png',
          1,
        );
        URL.revokeObjectURL(img.src);
      };
      img.onerror = () => {
        URL.revokeObjectURL(img.src);
        reject(new Error('Failed to load image'));
      };
      img.src = URL.createObjectURL(blob);
    });
  }

  /**
   * 转换为 JPG 格式
   * @param blob 原始图片 Blob
   * @param quality JPG 质量，0-1 之间，默认 0.92
   */
  function convertToJpg(blob: Blob, quality: number = 0.92): Promise<Blob> {
    return new Promise((resolve, reject) => {
      const img = new Image();
      img.onload = () => {
        const canvas = document.createElement('canvas');
        canvas.width = img.width;
        canvas.height = img.height;
        const ctx = canvas.getContext('2d');
        if (!ctx) {
          reject(new Error('Failed to get canvas context'));
          return;
        }
        // 填充白色背景（JPG 不支持透明）
        ctx.fillStyle = '#FFFFFF';
        ctx.fillRect(0, 0, canvas.width, canvas.height);
        ctx.drawImage(img, 0, 0);
        canvas.toBlob(
          (newBlob) => {
            if (newBlob) {
              resolve(newBlob);
            } else {
              reject(new Error('Failed to create blob'));
            }
          },
          'image/jpeg',
          quality,
        );
        URL.revokeObjectURL(img.src);
      };
      img.onerror = () => {
        URL.revokeObjectURL(img.src);
        reject(new Error('Failed to load image'));
      };
      img.src = URL.createObjectURL(blob);
    });
  }

  /**
   * 下载 Blob 为文件
   */
  function downloadBlob(blob: Blob, filename: string): void {
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = filename;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
  }

  /**
   * 下载图片原图
   */
  async function downloadImage(url: string, filename: string): Promise<void> {
    const blob = await fetchImageAsBlob(url);
    downloadBlob(blob, filename);
  }

  /**
   * 下载去除元数据的图片
   */
  async function downloadImageClean(url: string, filename: string): Promise<void> {
    const blob = await fetchImageAsBlob(url);
    const cleanBlob = await removeMetadata(blob);
    downloadBlob(cleanBlob, filename);
  }

  /**
   * 下载图片为 JPG 格式
   */
  async function downloadImageAsJpg(
    url: string,
    filename: string,
    quality: number = 0.92,
  ): Promise<void> {
    const blob = await fetchImageAsBlob(url);
    const jpgBlob = await convertToJpg(blob, quality);
    downloadBlob(jpgBlob, filename);
  }

  /**
   * 复制图片到剪贴板（去除元数据）
   */
  async function copyImageToClipboard(url: string): Promise<void> {
    const blob = await fetchImageAsBlob(url);
    const cleanBlob = await removeMetadata(blob);
    await navigator.clipboard.write([new ClipboardItem({ 'image/png': cleanBlob })]);
  }

  return {
    fetchImageAsBlob,
    removeMetadata,
    convertToJpg,
    downloadBlob,
    downloadImage,
    downloadImageClean,
    downloadImageAsJpg,
    copyImageToClipboard,
  };
}
