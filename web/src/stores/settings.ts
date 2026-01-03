import { defineStore } from 'pinia';
import { ref, watch } from 'vue';
import { LocalStorage } from 'quasar';

const AUTOCOMPLETE_ENABLED_KEY = 'codex-autocomplete-enabled';

/**
 * 全局编辑器设置 Store
 */
export const useSettingsStore = defineStore('settings', () => {
  // 自动补全开关
  const savedAutocompleteEnabled = LocalStorage.getItem<boolean>(AUTOCOMPLETE_ENABLED_KEY);
  const autocompleteEnabled = ref<boolean>(savedAutocompleteEnabled ?? false);

  // 监听变化，持久化到 LocalStorage
  watch(
    autocompleteEnabled,
    (val) => {
      LocalStorage.set(AUTOCOMPLETE_ENABLED_KEY, val);
    },
    { immediate: false },
  );

  // 切换自动补全
  function toggleAutocomplete() {
    autocompleteEnabled.value = !autocompleteEnabled.value;
  }

  // 设置自动补全
  function setAutocomplete(enabled: boolean) {
    autocompleteEnabled.value = enabled;
  }

  return {
    // State
    autocompleteEnabled,
    // Actions
    toggleAutocomplete,
    setAutocomplete,
  };
});
