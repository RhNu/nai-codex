import { defineStore } from 'pinia';
import { ref, watch, computed } from 'vue';
import { Dark, LocalStorage, setCssVar } from 'quasar';

const THEME_KEY = 'codex-theme';
const DARK_MODE_KEY = 'codex-dark-mode';

// 主题配色定义
export interface ThemeColors {
  primary: string;
  secondary: string;
  accent: string;
  positive: string;
  negative: string;
  info: string;
  warning: string;
}

export interface Theme {
  id: string;
  name: string;
  icon: string;
  colors: ThemeColors;
  dark?: Partial<ThemeColors>; // 暗色模式下的特殊颜色（可选覆盖）
}

// 预设主题
export const presetThemes: Theme[] = [
  {
    id: 'default',
    name: '默认蓝',
    icon: 'water_drop',
    colors: {
      primary: '#1976D2',
      secondary: '#26A69A',
      accent: '#9C27B0',
      positive: '#21BA45',
      negative: '#C10015',
      info: '#31CCEC',
      warning: '#F2C037',
    },
  },
  {
    id: 'sakura',
    name: '樱花粉',
    icon: 'local_florist',
    colors: {
      primary: '#E91E63',
      secondary: '#F48FB1',
      accent: '#9C27B0',
      positive: '#4CAF50',
      negative: '#F44336',
      info: '#2196F3',
      warning: '#FF9800',
    },
  },
  {
    id: 'forest',
    name: '森林绿',
    icon: 'park',
    colors: {
      primary: '#2E7D32',
      secondary: '#66BB6A',
      accent: '#00BCD4',
      positive: '#4CAF50',
      negative: '#E53935',
      info: '#29B6F6',
      warning: '#FFA726',
    },
  },
  {
    id: 'sunset',
    name: '日落橙',
    icon: 'wb_twilight',
    colors: {
      primary: '#F57C00',
      secondary: '#FFB74D',
      accent: '#E91E63',
      positive: '#66BB6A',
      negative: '#EF5350',
      info: '#42A5F5',
      warning: '#FFCA28',
    },
  },
  {
    id: 'ocean',
    name: '深海蓝',
    icon: 'waves',
    colors: {
      primary: '#0277BD',
      secondary: '#0097A7',
      accent: '#7C4DFF',
      positive: '#00C853',
      negative: '#FF5252',
      info: '#00E5FF',
      warning: '#FFD600',
    },
  },
  {
    id: 'lavender',
    name: '薰衣草',
    icon: 'spa',
    colors: {
      primary: '#7E57C2',
      secondary: '#B39DDB',
      accent: '#E91E63',
      positive: '#66BB6A',
      negative: '#EF5350',
      info: '#4FC3F7',
      warning: '#FFB74D',
    },
  },
  {
    id: 'midnight',
    name: '午夜黑',
    icon: 'nightlight',
    colors: {
      primary: '#424242',
      secondary: '#757575',
      accent: '#FF4081',
      positive: '#69F0AE',
      negative: '#FF5252',
      info: '#40C4FF',
      warning: '#FFD740',
    },
  },
  {
    id: 'coral',
    name: '珊瑚红',
    icon: 'favorite',
    colors: {
      primary: '#FF5722',
      secondary: '#FF8A65',
      accent: '#7C4DFF',
      positive: '#00E676',
      negative: '#FF1744',
      info: '#18FFFF',
      warning: '#FFEA00',
    },
  },
];

export const useThemeStore = defineStore('theme', () => {
  // 当前主题 ID
  const savedThemeId = LocalStorage.getItem<string>(THEME_KEY);
  const currentThemeId = ref<string>(savedThemeId ?? 'default');

  // 暗色模式
  const savedDarkMode = LocalStorage.getItem<boolean | 'auto'>(DARK_MODE_KEY);
  const darkMode = ref<boolean | 'auto'>(savedDarkMode ?? 'auto');

  // 当前主题配置
  const currentTheme = computed(() => {
    return presetThemes.find((t) => t.id === currentThemeId.value) ?? presetThemes[0];
  });

  // 是否处于暗色模式
  const isDark = computed(() => {
    if (darkMode.value === 'auto') {
      return Dark.isActive;
    }
    return darkMode.value;
  });

  // 应用主题颜色
  function applyTheme() {
    const theme = currentTheme.value;
    if (!theme) return;
    const colors = theme.colors;
    const darkColors = theme.dark;

    // 设置基础颜色
    Object.entries(colors).forEach(([key, value]) => {
      // 如果是暗色模式且有特殊暗色配置，使用暗色配置
      const finalValue =
        isDark.value && darkColors && darkColors[key as keyof ThemeColors]
          ? darkColors[key as keyof ThemeColors]!
          : value;
      setCssVar(key, finalValue);
    });
  }

  // 应用暗色模式
  function applyDarkMode() {
    Dark.set(darkMode.value);
    // 暗色模式变化后重新应用主题
    applyTheme();
  }

  // 切换主题
  function setTheme(themeId: string) {
    const theme = presetThemes.find((t) => t.id === themeId);
    if (theme) {
      currentThemeId.value = themeId;
    }
  }

  // 切换暗色模式（三态切换: auto -> true -> false -> auto）
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

  // 监听主题变化
  watch(
    currentThemeId,
    (val) => {
      LocalStorage.set(THEME_KEY, val);
      applyTheme();
    },
    { immediate: true },
  );

  // 监听暗色模式变化
  watch(
    darkMode,
    (val) => {
      LocalStorage.set(DARK_MODE_KEY, val);
      applyDarkMode();
    },
    { immediate: true },
  );

  // 监听系统暗色模式变化（当 darkMode === 'auto' 时）
  if (typeof window !== 'undefined') {
    const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
    mediaQuery.addEventListener('change', () => {
      if (darkMode.value === 'auto') {
        applyDarkMode();
      }
    });
  }

  return {
    // State
    currentThemeId,
    darkMode,
    // Getters
    currentTheme,
    isDark,
    presetThemes,
    // Actions
    setTheme,
    toggleDarkMode,
    setDarkMode,
    applyTheme,
    applyDarkMode,
  };
});
