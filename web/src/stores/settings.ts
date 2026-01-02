import { defineStore } from 'pinia';
import { ref, watch } from 'vue';
import { Dark, LocalStorage } from 'quasar';

const DARK_MODE_KEY = 'codex-dark-mode';

export const useSettingsStore = defineStore('settings', () => {
  // 从 LocalStorage 读取初始值，默认跟随系统
  const savedDarkMode = LocalStorage.getItem<boolean | 'auto'>(DARK_MODE_KEY);
  const darkMode = ref<boolean | 'auto'>(savedDarkMode ?? 'auto');

  // 应用暗色模式设置
  function applyDarkMode() {
    Dark.set(darkMode.value);
  }

  // 切换暗色模式
  function toggleDarkMode() {
    if (darkMode.value === 'auto') {
      darkMode.value = true;
    } else if (darkMode.value === true) {
      darkMode.value = false;
    } else {
      darkMode.value = 'auto';
    }
  }

  // 设置暗色模式
  function setDarkMode(value: boolean | 'auto') {
    darkMode.value = value;
  }

  // 监听变化并保存
  watch(
    darkMode,
    (val) => {
      LocalStorage.set(DARK_MODE_KEY, val);
      applyDarkMode();
    },
    { immediate: true },
  );

  return {
    darkMode,
    toggleDarkMode,
    setDarkMode,
    applyDarkMode,
  };
});
