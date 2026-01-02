<script setup lang="ts">
import { ref, watch, computed } from 'vue';
import { useDebounceFn } from '@vueuse/core';
import { fetchSnippets, type SnippetSummary } from 'src/services/api';

const props = withDefaults(
  defineProps<{
    /** 是否显示搜索面板 */
    showPanel?: boolean;
  }>(),
  {
    showPanel: false,
  },
);

const emit = defineEmits<{
  (e: 'select', value: string): void;
  (e: 'update:showPanel', value: boolean): void;
}>();

const searchQuery = ref('');
const snippets = ref<SnippetSummary[]>([]);
const loading = ref(false);
const selectedIndex = ref(0);
const inputRef = ref<HTMLInputElement | null>(null);

// 分类过滤
const categories = computed(() => {
  const cats = new Set<string>();
  snippets.value.forEach((s) => cats.add(s.category));
  return Array.from(cats).sort();
});

const selectedCategory = ref<string | null>(null);

const filteredSnippets = computed(() => {
  if (!selectedCategory.value) return snippets.value;
  return snippets.value.filter((s) => s.category === selectedCategory.value);
});

// 搜索
async function search() {
  loading.value = true;
  try {
    const params: { q?: string; limit: number } = { limit: 50 };
    if (searchQuery.value) {
      params.q = searchQuery.value;
    }
    const result = await fetchSnippets(params);
    snippets.value = result.items;
    selectedIndex.value = 0;
  } catch (err) {
    console.error('Failed to search snippets:', err);
  } finally {
    loading.value = false;
  }
}

const debouncedSearch = useDebounceFn(search, 300);

watch(searchQuery, () => {
  void debouncedSearch();
});

// 选择 snippet
function selectSnippet(snippet: SnippetSummary) {
  emit('select', `<snippet:${snippet.name}>`);
  emit('update:showPanel', false);
  searchQuery.value = '';
}

// 键盘导航
function onKeyDown(e: KeyboardEvent) {
  if (e.key === 'ArrowDown') {
    e.preventDefault();
    selectedIndex.value = Math.min(selectedIndex.value + 1, filteredSnippets.value.length - 1);
  } else if (e.key === 'ArrowUp') {
    e.preventDefault();
    selectedIndex.value = Math.max(selectedIndex.value - 1, 0);
  } else if (e.key === 'Enter') {
    e.preventDefault();
    const selected = filteredSnippets.value[selectedIndex.value];
    if (selected) {
      selectSnippet(selected);
    }
  } else if (e.key === 'Escape') {
    emit('update:showPanel', false);
  }
}

// 当面板打开时加载数据
watch(
  () => props.showPanel,
  (show) => {
    if (show) {
      void search();
      setTimeout(() => {
        inputRef.value?.focus();
      }, 100);
    }
  },
);

function close() {
  emit('update:showPanel', false);
}
</script>

<template>
  <q-dialog :model-value="showPanel" @update:model-value="emit('update:showPanel', $event)">
    <q-card style="min-width: 500px; max-width: 90vw; max-height: 80vh">
      <q-card-section class="row items-center q-pb-sm">
        <div class="text-h6">搜索 Snippet</div>
        <q-space />
        <q-btn icon="close" flat round dense @click="close" />
      </q-card-section>

      <q-card-section class="q-pt-none">
        <q-input
          ref="inputRef"
          v-model="searchQuery"
          placeholder="搜索名称、标签或描述..."
          filled
          dense
          clearable
          @keydown="onKeyDown"
        >
          <template #prepend>
            <q-icon name="search" />
          </template>
        </q-input>

        <!-- 分类过滤 -->
        <div class="q-mt-sm q-gutter-xs" v-if="categories.length > 1">
          <q-chip
            :outline="selectedCategory !== null"
            :color="selectedCategory === null ? 'primary' : undefined"
            clickable
            dense
            @click="selectedCategory = null"
          >
            全部
          </q-chip>
          <q-chip
            v-for="cat in categories"
            :key="cat"
            :outline="selectedCategory !== cat"
            :color="selectedCategory === cat ? 'primary' : undefined"
            clickable
            dense
            @click="selectedCategory = cat"
          >
            {{ cat }}
          </q-chip>
        </div>
      </q-card-section>

      <q-separator />

      <q-card-section class="q-pa-none" style="max-height: 400px; overflow-y: auto">
        <!-- 加载状态 -->
        <div v-if="loading" class="flex flex-center q-pa-lg">
          <q-spinner color="primary" size="2em" />
        </div>

        <!-- 空状态 -->
        <div v-else-if="filteredSnippets.length === 0" class="text-center q-pa-lg text-grey-6">
          <q-icon name="search_off" size="3em" class="q-mb-sm" />
          <div>未找到匹配的 Snippet</div>
        </div>

        <!-- 结果列表 -->
        <q-list v-else separator>
          <q-item
            v-for="(snippet, idx) in filteredSnippets"
            :key="snippet.id"
            clickable
            :active="idx === selectedIndex"
            active-class="bg-primary text-white"
            @click="selectSnippet(snippet)"
            @mouseenter="selectedIndex = idx"
          >
            <q-item-section avatar>
              <q-icon name="code" :color="idx === selectedIndex ? 'white' : 'primary'" />
            </q-item-section>
            <q-item-section>
              <q-item-label>
                <span class="text-weight-medium">{{ snippet.name }}</span>
                <q-badge
                  :color="idx === selectedIndex ? 'white' : 'grey'"
                  text-color="dark"
                  class="q-ml-sm"
                >
                  {{ snippet.category }}
                </q-badge>
              </q-item-label>
              <q-item-label caption :class="idx === selectedIndex ? 'text-white' : ''">
                {{ snippet.description || '无描述' }}
              </q-item-label>
              <q-item-label
                caption
                :class="idx === selectedIndex ? 'text-white' : 'text-grey-6'"
                v-if="snippet.tags.length > 0"
              >
                <q-icon name="label" size="xs" class="q-mr-xs" />
                {{ snippet.tags.join(', ') }}
              </q-item-label>
            </q-item-section>
            <q-item-section side>
              <q-icon name="add" />
            </q-item-section>
          </q-item>
        </q-list>
      </q-card-section>

      <q-separator />

      <q-card-section class="q-pa-sm text-caption text-grey-6">
        <q-icon name="keyboard" size="xs" class="q-mr-xs" />
        ↑↓ 导航 · Enter 选择 · Esc 关闭
      </q-card-section>
    </q-card>
  </q-dialog>
</template>

<style scoped lang="scss">
.q-item {
  transition: background-color 0.15s;
}
</style>
