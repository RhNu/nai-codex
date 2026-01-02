<template>
  <q-page class="lexicon-page q-pa-md">
    <div class="row q-col-gutter-md" style="height: calc(100vh - 100px)">
      <!-- 左侧分类导航 -->
      <div class="col-3">
        <q-card class="full-height">
          <q-card-section class="q-pb-none">
            <div class="text-h6">分类</div>
          </q-card-section>
          <q-card-section class="scroll" style="max-height: calc(100vh - 200px)">
            <q-list dense>
              <q-item
                v-for="cat in categories"
                :key="cat.name"
                clickable
                :active="selectedCategory === cat.name"
                active-class="bg-primary text-white"
                @click="selectCategory(cat.name)"
              >
                <q-item-section>
                  <q-item-label>{{ cat.name }}</q-item-label>
                  <q-item-label caption>{{ cat.tag_count }} 标签</q-item-label>
                </q-item-section>
                <q-item-section side>
                  <q-icon name="chevron_right" />
                </q-item-section>
              </q-item>
            </q-list>
          </q-card-section>
        </q-card>
      </div>

      <!-- 右侧内容区 -->
      <div class="col-9">
        <div class="column full-height q-gutter-md">
          <!-- 搜索框 -->
          <q-input
            v-model="searchQuery"
            outlined
            dense
            placeholder="搜索标签（中文/英文）..."
            clearable
            @update:model-value="onSearch"
          >
            <template #prepend>
              <q-icon name="search" />
            </template>
          </q-input>

          <!-- 二级分类 chips (非搜索模式) -->
          <q-card v-if="!isSearchMode && categoryData" flat bordered class="subcategory-card">
            <q-card-section class="q-py-sm">
              <div class="row items-center q-mb-xs">
                <div class="text-caption text-grey">子分类</div>
                <q-space />
                <q-btn
                  v-if="subcategories.length > maxVisibleSubcategories"
                  flat
                  dense
                  size="sm"
                  :icon="showAllSubcategories ? 'expand_less' : 'expand_more'"
                  :label="showAllSubcategories ? '收起' : `展开全部 (${subcategories.length})`"
                  @click="showAllSubcategories = !showAllSubcategories"
                />
              </div>
              <div class="row q-gutter-xs subcategory-chips">
                <q-chip
                  v-for="sub in visibleSubcategories"
                  :key="sub"
                  clickable
                  dense
                  :outline="selectedSubcategory !== sub"
                  :color="selectedSubcategory === sub ? 'primary' : 'grey-6'"
                  :text-color="selectedSubcategory === sub ? 'white' : undefined"
                  @click="selectedSubcategory = sub"
                >
                  {{ sub }}
                </q-chip>
              </div>
            </q-card-section>
          </q-card>

          <!-- 标签列表 -->
          <q-card class="col scroll">
            <q-card-section v-if="loading" class="flex flex-center">
              <q-spinner size="lg" />
            </q-card-section>
            <q-card-section v-else-if="displayTags.length === 0">
              <div class="text-center text-grey">
                {{ isSearchMode ? '未找到匹配标签' : '请选择分类' }}
              </div>
            </q-card-section>
            <q-card-section v-else class="q-pa-sm">
              <div class="row q-gutter-sm">
                <q-chip
                  v-for="entry in displayTags"
                  :key="`${entry.category}-${entry.subcategory}-${entry.tag}`"
                  clickable
                  color="primary"
                  text-color="white"
                  @click="addToAssembly(entry)"
                >
                  <q-tooltip>
                    <div v-if="entry.weight">热度: {{ formatWeight(entry.weight) }}</div>
                    <div class="text-caption">{{ entry.category }} / {{ entry.subcategory }}</div>
                  </q-tooltip>
                  <span class="chip-label">
                    <span class="chip-en">{{ entry.tag }}</span>
                    <span v-if="entry.zh && entry.zh !== entry.tag" class="chip-zh">{{
                      entry.zh
                    }}</span>
                  </span>
                  <q-badge
                    v-if="entry.weight"
                    :color="getWeightColor(entry.weight)"
                    floating
                    transparent
                  />
                </q-chip>
              </div>
            </q-card-section>
          </q-card>

          <!-- 组装文本框 -->
          <q-card>
            <q-card-section class="q-py-sm">
              <div class="row items-center q-gutter-sm">
                <div class="text-subtitle2">已组装:</div>
                <q-space />
                <q-btn
                  flat
                  dense
                  icon="content_copy"
                  label="复制"
                  :disable="assembledTags.length === 0"
                  @click="copyAssembly"
                />
                <q-btn
                  flat
                  dense
                  icon="clear"
                  label="清空"
                  :disable="assembledTags.length === 0"
                  @click="clearAssembly"
                />
              </div>
            </q-card-section>
            <q-separator />
            <q-card-section class="q-py-sm">
              <div v-if="assembledTags.length === 0" class="text-grey text-center">
                点击标签添加到此处
              </div>
              <div v-else class="row q-gutter-xs">
                <q-chip
                  v-for="(tag, idx) in assembledTags"
                  :key="idx"
                  removable
                  color="secondary"
                  text-color="white"
                  @remove="removeFromAssembly(idx)"
                >
                  {{ tag }}
                </q-chip>
              </div>
            </q-card-section>
          </q-card>
        </div>
      </div>
    </div>
  </q-page>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, watch } from 'vue';
import { useQuasar } from 'quasar';
import {
  fetchLexiconIndex,
  fetchLexiconCategory,
  searchLexicon,
  type LexiconEntry,
  type CategoryInfo,
  type CategoryData,
} from 'src/services/api';
import { useDebounceFn } from '@vueuse/core';

const $q = useQuasar();

const categories = ref<CategoryInfo[]>([]);
const selectedCategory = ref<string | null>(null);
const selectedSubcategory = ref<string | null>(null);
const categoryData = ref<CategoryData | null>(null);
const searchQuery = ref('');
const searchResults = ref<LexiconEntry[]>([]);
const loading = ref(false);
const assembledTags = ref<string[]>([]);
const showAllSubcategories = ref(false);
const maxVisibleSubcategories = 12; // 默认最多显示的子分类数量

const isSearchMode = computed(() => searchQuery.value.trim().length > 0);

const subcategories = computed(() => {
  if (!categoryData.value) return [];
  return Object.keys(categoryData.value.subcategories);
});

const visibleSubcategories = computed(() => {
  if (showAllSubcategories.value) return subcategories.value;
  return subcategories.value.slice(0, maxVisibleSubcategories);
});

const displayTags = computed(() => {
  if (isSearchMode.value) {
    return searchResults.value;
  }
  if (!categoryData.value || !selectedSubcategory.value) {
    return [];
  }
  return categoryData.value.subcategories[selectedSubcategory.value] || [];
});

onMounted(async () => {
  try {
    const index = await fetchLexiconIndex();
    categories.value = index.categories;
    // 默认选中第一个分类
    if (categories.value.length > 0 && categories.value[0]) {
      await selectCategory(categories.value[0].name);
    }
  } catch (err) {
    console.error('Failed to load lexicon index:', err);
    $q.notify({ type: 'negative', message: '加载词库失败' });
  }
});

async function selectCategory(name: string) {
  if (selectedCategory.value === name) return;
  selectedCategory.value = name;
  showAllSubcategories.value = false; // 重置展开状态
  loading.value = true;
  try {
    categoryData.value = await fetchLexiconCategory(name);
    // 默认选中第一个子分类
    const subs = Object.keys(categoryData.value.subcategories);
    selectedSubcategory.value = subs.length > 0 ? (subs[0] ?? null) : null;
  } catch (err) {
    console.error('Failed to load category:', err);
    $q.notify({ type: 'negative', message: '加载分类失败' });
  } finally {
    loading.value = false;
  }
}

const doSearch = useDebounceFn(async (q: string) => {
  if (!q.trim()) {
    searchResults.value = [];
    return;
  }
  loading.value = true;
  try {
    const result = await searchLexicon({ q: q.trim(), limit: 100 });
    searchResults.value = result.entries;
  } catch (err) {
    console.error('Search failed:', err);
  } finally {
    loading.value = false;
  }
}, 300);

function onSearch(val: string | number | null) {
  void doSearch(String(val || ''));
}

function addToAssembly(entry: LexiconEntry) {
  assembledTags.value.push(entry.tag);
}

function removeFromAssembly(idx: number) {
  assembledTags.value.splice(idx, 1);
}

function clearAssembly() {
  assembledTags.value = [];
}

async function copyAssembly() {
  const text = assembledTags.value.join(', ');
  try {
    await navigator.clipboard.writeText(text);
    $q.notify({ type: 'positive', message: '已复制到剪贴板' });
  } catch (err) {
    console.error('Copy failed:', err);
    $q.notify({ type: 'negative', message: '复制失败，请重试' });
  }
}

function formatWeight(weight: number): string {
  if (weight >= 1000000) return `${(weight / 1000000).toFixed(1)}M`;
  if (weight >= 1000) return `${(weight / 1000).toFixed(1)}K`;
  return weight.toString();
}

function getWeightColor(weight: number): string {
  if (weight >= 1000000) return 'red';
  if (weight >= 100000) return 'orange';
  if (weight >= 10000) return 'yellow';
  return 'grey';
}

// 监听搜索模式切换，清除选中
watch(isSearchMode, (mode) => {
  if (mode) {
    // 进入搜索模式时不清除分类选择，退出时恢复显示
  }
});
</script>

<style scoped lang="scss">
.lexicon-page {
  .full-height {
    height: 100%;
  }
}

.subcategory-card {
  flex-shrink: 0;
}

.subcategory-chips {
  flex-wrap: wrap;
}

.chip-label {
  display: inline-flex;
  align-items: baseline;
  gap: 4px;
}

.chip-en {
  font-size: 0.85em;
}

.chip-zh {
  font-size: 0.75em;
  opacity: 0.85;

  &::before {
    content: '/';
    margin-right: 2px;
    opacity: 0.6;
  }
}
</style>
