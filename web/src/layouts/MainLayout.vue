<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { useQuasar } from 'quasar';
import { useTaskStore, type TaskItem } from 'src/stores/tasks';
import { useThemeStore, presetThemes } from 'src/stores/theme';
import { useSettingsStore } from 'src/stores/settings';
import { useNotificationPermission } from 'src/composables';

const $q = useQuasar();
const leftDrawerOpen = ref(true);
const taskDrawerOpen = ref(false);

const taskStore = useTaskStore();
const themeStore = useThemeStore();
const settingsStore = useSettingsStore();
const tasks = computed(() => taskStore.items);

// 通知权限
const {
  isSupported: notificationSupported,
  permissionState,
  requestPermission,
} = useNotificationPermission();

async function handleRequestNotification() {
  const granted = await requestPermission();
  if (granted) {
    $q.notify({ type: 'positive', message: '已开启浏览器通知' });
  } else {
    $q.notify({ type: 'warning', message: '通知权限被拒绝' });
  }
}

const darkModeIcon = computed(() => {
  if (themeStore.darkMode === 'auto') return 'brightness_auto';
  return themeStore.darkMode ? 'dark_mode' : 'light_mode';
});

const darkModeTooltip = computed(() => {
  if (themeStore.darkMode === 'auto') return '跟随系统';
  return themeStore.darkMode ? '夜间模式' : '日间模式';
});

const navItems = [
  { to: '/home', icon: 'home', label: '主页', caption: '总览与状态' },
  { to: '/generate', icon: 'brush', label: '图像生成', caption: '调用 NovelAI' },
  { to: '/snippets', icon: 'snippet_folder', label: 'Snippet 编辑', caption: '提示片段库' },
  { to: '/presets', icon: 'person', label: '角色预设', caption: '角色提示词片段' },
  { to: '/lexicon', icon: 'translate', label: '词库', caption: 'Tag分类检索' },
  { to: '/gallery', icon: 'photo_library', label: '画廊', caption: '历史生成' },
];

function statusColor(status: TaskItem['status']) {
  switch (status) {
    case 'pending':
      return 'grey';
    case 'running':
      return 'primary';
    case 'completed':
      return 'positive';
    case 'failed':
      return 'negative';
    default:
      return 'grey';
  }
}

function statusIcon(status: TaskItem['status']) {
  switch (status) {
    case 'pending':
      return 'hourglass_empty';
    case 'running':
      return 'sync';
    case 'completed':
      return 'check_circle';
    case 'failed':
      return 'error';
    default:
      return 'help';
  }
}

function statusText(status: TaskItem['status']) {
  switch (status) {
    case 'pending':
      return '等待中';
    case 'running':
      return '生成中';
    case 'completed':
      return '已完成';
    case 'failed':
      return '失败';
    default:
      return status;
  }
}

function formatDuration(ms: number) {
  const seconds = Math.floor(ms / 1000);
  if (seconds < 60) return `${seconds}秒`;
  const minutes = Math.floor(seconds / 60);
  const remainingSeconds = seconds % 60;
  return `${minutes}分${remainingSeconds}秒`;
}

onMounted(() => {
  taskStore.startPolling();
});
</script>

<template>
  <q-layout view="hHh LpR fFf" class="main-layout">
    <q-header elevated class="bg-primary text-white">
      <q-toolbar>
        <q-btn
          flat
          dense
          round
          icon="menu"
          aria-label="Menu"
          @click="leftDrawerOpen = !leftDrawerOpen"
        />
        <q-toolbar-title>NAI Codex</q-toolbar-title>
        <q-space />
        <!-- 自动补全开关 -->
        <q-btn
          flat
          dense
          round
          :icon="settingsStore.autocompleteEnabled ? 'auto_awesome' : 'auto_awesome_mosaic'"
          :color="settingsStore.autocompleteEnabled ? 'white' : 'grey-4'"
          aria-label="Toggle Autocomplete"
          @click="settingsStore.toggleAutocomplete()"
        >
          <q-tooltip>{{
            settingsStore.autocompleteEnabled ? '自动补全：开' : '自动补全：关'
          }}</q-tooltip>
        </q-btn>
        <!-- 主题选择按钮 -->
        <q-btn
          flat
          dense
          round
          :icon="themeStore.currentTheme?.icon || 'palette'"
          aria-label="Theme"
        >
          <q-tooltip>切换主题</q-tooltip>
          <q-menu anchor="bottom right" self="top right">
            <q-list style="min-width: 180px">
              <q-item-label header>选择主题</q-item-label>
              <q-item
                v-for="theme in presetThemes"
                :key="theme.id"
                clickable
                v-close-popup
                :active="themeStore.currentThemeId === theme.id"
                @click="themeStore.setTheme(theme.id)"
              >
                <q-item-section avatar>
                  <q-icon :name="theme.icon" :style="{ color: theme.colors.primary }" />
                </q-item-section>
                <q-item-section>{{ theme.name }}</q-item-section>
                <q-item-section side v-if="themeStore.currentThemeId === theme.id">
                  <q-icon name="check" color="primary" />
                </q-item-section>
              </q-item>
            </q-list>
          </q-menu>
        </q-btn>
        <q-btn
          flat
          dense
          round
          :icon="darkModeIcon"
          aria-label="Toggle Dark Mode"
          @click="themeStore.toggleDarkMode()"
        >
          <q-tooltip>{{ darkModeTooltip }}</q-tooltip>
        </q-btn>
        <q-btn
          flat
          dense
          round
          icon="task_alt"
          aria-label="Tasks"
          @click="taskDrawerOpen = !taskDrawerOpen"
        />
      </q-toolbar>
    </q-header>

    <q-drawer v-model="leftDrawerOpen" show-if-above bordered class="nav-drawer">
      <q-scroll-area class="fit">
        <q-list padding>
          <q-item
            v-for="item in navItems"
            :key="item.to"
            clickable
            tag="router-link"
            :to="item.to"
            active-class="text-primary"
          >
            <q-item-section avatar>
              <q-icon :name="item.icon" />
            </q-item-section>
            <q-item-section>
              <q-item-label>{{ item.label }}</q-item-label>
              <q-item-label caption>{{ item.caption }}</q-item-label>
            </q-item-section>
          </q-item>
        </q-list>
      </q-scroll-area>
    </q-drawer>

    <q-drawer v-model="taskDrawerOpen" side="right" bordered :width="360">
      <q-scroll-area class="fit">
        <q-list padding>
          <q-item-label header class="row items-center justify-between">
            <span class="text-h6">任务队列</span>
            <q-btn
              v-if="notificationSupported && permissionState !== 'granted'"
              flat
              dense
              size="sm"
              icon="notifications"
              color="primary"
              @click="handleRequestNotification"
            >
              <q-tooltip>开启浏览器通知</q-tooltip>
            </q-btn>
            <q-icon
              v-else-if="notificationSupported && permissionState === 'granted'"
              name="notifications_active"
              color="positive"
              size="sm"
            >
              <q-tooltip>浏览器通知已开启</q-tooltip>
            </q-icon>
          </q-item-label>
          <q-item v-for="task in tasks" :key="task.id" class="task-item">
            <q-item-section avatar>
              <q-avatar :color="statusColor(task.status)" text-color="white" size="36px">
                <q-icon :name="statusIcon(task.status)" size="20px" />
                <q-spinner-orbit
                  v-if="task.status === 'running'"
                  color="white"
                  size="36px"
                  class="absolute"
                />
              </q-avatar>
            </q-item-section>
            <q-item-section>
              <q-item-label class="ellipsis" style="max-width: 180px">
                {{ task.title || '未命名任务' }}
              </q-item-label>
              <q-item-label caption>
                <span :class="'text-' + statusColor(task.status)">
                  {{ statusText(task.status) }}
                </span>
                <span v-if="task.error" class="text-negative q-ml-xs"> · {{ task.error }} </span>
              </q-item-label>
              <q-item-label caption v-if="task.completedAt && task.startedAt">
                耗时: {{ formatDuration(task.completedAt - task.startedAt) }}
              </q-item-label>
            </q-item-section>
            <q-item-section side>
              <q-badge :color="statusColor(task.status)" outline> {{ task.count }} 张 </q-badge>
            </q-item-section>
          </q-item>
          <q-item v-if="tasks.length === 0">
            <q-item-section class="text-center text-grey">
              <q-icon name="inbox" size="48px" class="q-mb-sm" />
              <div>暂无任务</div>
            </q-item-section>
          </q-item>
        </q-list>
      </q-scroll-area>
    </q-drawer>

    <q-page-container class="page-container">
      <q-scroll-area class="fit page-scroll-area">
        <router-view />
      </q-scroll-area>
    </q-page-container>
  </q-layout>
</template>

<style scoped lang="scss">
.main-layout {
  height: 100vh;
  overflow: hidden;
}

.page-container {
  height: 100%;
  overflow: hidden;
}

.page-scroll-area {
  height: 100%;
}

.nav-drawer {
  height: 100%;
}

.task-item {
  border-bottom: 1px solid rgba(0, 0, 0, 0.05);
  transition: background-color 0.2s;

  &:hover {
    background-color: rgba(0, 0, 0, 0.03);
  }
}
</style>
