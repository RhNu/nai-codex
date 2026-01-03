/**
 * useNotificationPermission - 浏览器通知权限管理
 *
 * 功能：
 * - 检查通知权限状态
 * - 请求通知权限
 */
import { ref, onMounted, computed } from 'vue';
import { usePermission } from '@vueuse/core';

export function useNotificationPermission() {
  const permissionRef = usePermission('notifications');
  const permissionState = computed(() => permissionRef.value);
  const isSupported = ref(false);

  onMounted(() => {
    isSupported.value = 'Notification' in window;
  });

  async function requestPermission(): Promise<boolean> {
    if (!isSupported.value) return false;

    try {
      const result = await Notification.requestPermission();
      return result === 'granted';
    } catch {
      return false;
    }
  }

  return {
    isSupported,
    permissionState,
    requestPermission,
  };
}
