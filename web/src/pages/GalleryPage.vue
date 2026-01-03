<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue';
import { useQuasar } from 'quasar';
import { useClipboard } from '@vueuse/core';
import {
  fetchRecentRecords,
  deleteRecordsBatch,
  fetchArchives,
  fetchArchivableDates,
  createArchive,
  createArchiveSelected,
  deleteArchive,
  getArchiveDownloadUrl,
  type GenerationRecord,
  type ArchiveInfo,
  type ArchivableDate,
} from 'src/services/api';
import { useGalleryNavigation, useImageTools } from 'src/composables';

type GalleryItem = {
  id: string;
  url: string;
  seed: number;
  width: number;
  height: number;
  createdAt: string;
  recordId: string;
  prompt: string;
};

const $q = useQuasar();
const { copy } = useClipboard();
const { fetchImageAsBlob, removeMetadata, downloadBlob, copyImageToClipboard } = useImageTools();

const search = ref('');
const images = ref<GalleryItem[]>([]);
const loading = ref(true);
const showDialog = ref(false);
const selectedImage = ref<GalleryItem | null>(null);

// 多选模式
const selectMode = ref(false);
const selectedIds = ref<Set<string>>(new Set());

// 分页
const page = ref(1);
const pageSize = ref(24);

// 当前选中的日期 Tab
const selectedDateTab = ref<string | null>(null);

// 归档相关状态
const archives = ref<ArchiveInfo[]>([]);
const archivableDates = ref<ArchivableDate[]>([]);
const selectedArchiveDates = ref<Set<string>>(new Set());
const archivesLoading = ref(false);
const archiveCreating = ref(false);
const showArchiveDialog = ref(false);

// 搜索变化时重置页码和日期Tab
watch(search, () => {
  page.value = 1;
  // 重新选择第一个日期
  selectedDateTab.value = allSortedDates.value[0] || null;
});

// filteredImages 需要先定义，供 useGalleryNavigation 使用
const filteredImages = computed(() => {
  if (!search.value.trim()) return images.value;
  const q = search.value.trim().toLowerCase();
  return images.value.filter(
    (img) => String(img.seed).includes(q) || img.prompt.toLowerCase().includes(q),
  );
});

// 画廊导航
const {
  hasPrev,
  hasNext,
  currentIndex,
  total: navTotal,
  goToPrev,
  goToNext,
} = useGalleryNavigation({
  currentItem: selectedImage,
  items: filteredImages,
  isDialogOpen: showDialog,
  getItemId: (item) => item.id,
  loop: false,
  onCopy: () => {
    void copyWithoutMetadata();
  },
});

// 按日期分组所有过滤后的图片（用于生成 Tab）
const allGroupedImages = computed(() => {
  const groups: Record<string, GalleryItem[]> = {};
  for (const img of filteredImages.value) {
    const date = formatDate(img.createdAt);
    if (!groups[date]) {
      groups[date] = [];
    }
    groups[date].push(img);
  }
  return groups;
});

// 所有日期（排序后），用于 Tab 显示
const allSortedDates = computed(() => {
  return Object.keys(allGroupedImages.value).sort((a, b) => {
    // 解析日期字符串进行比较（处理 "今天"、"昨天" 等特殊格式）
    const getDateFromLabel = (label: string) => {
      const match = label.match(/\((\d{4}\/\d{2}\/\d{2})\)/);
      if (match && match[1]) return new Date(match[1]);
      return new Date(label);
    };
    return getDateFromLabel(b).getTime() - getDateFromLabel(a).getTime();
  });
});

// 当前选中日期的图片
const currentDateImages = computed(() => {
  if (!selectedDateTab.value) return [];
  return allGroupedImages.value[selectedDateTab.value] || [];
});

// 当前日期的分页图片
const paginatedImages = computed(() => {
  const start = (page.value - 1) * pageSize.value;
  const end = start + pageSize.value;
  return currentDateImages.value.slice(start, end);
});

const totalPages = computed(() =>
  Math.max(1, Math.ceil(currentDateImages.value.length / pageSize.value)),
);

function formatDate(isoString: string): string {
  const date = new Date(isoString);
  const today = new Date();
  const yesterday = new Date(today);
  yesterday.setDate(yesterday.getDate() - 1);

  const dateStr = date.toLocaleDateString('zh-CN', {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
  });

  if (date.toDateString() === today.toDateString()) {
    return `今天 (${dateStr})`;
  } else if (date.toDateString() === yesterday.toDateString()) {
    return `昨天 (${dateStr})`;
  }
  return dateStr;
}

function formatTime(isoString: string): string {
  return new Date(isoString).toLocaleTimeString('zh-CN', {
    hour: '2-digit',
    minute: '2-digit',
  });
}

function showImage(img: GalleryItem) {
  if (selectMode.value) {
    toggleSelect(img.id);
    return;
  }
  selectedImage.value = img;
  showDialog.value = true;
}

function copyToClipboard(text: string) {
  void copy(text).then(() => {
    $q.notify({ type: 'positive', message: '已复制到剪贴板', timeout: 1500 });
  });
}

// 多选操作
function toggleSelect(id: string) {
  if (selectedIds.value.has(id)) {
    selectedIds.value.delete(id);
  } else {
    selectedIds.value.add(id);
  }
  // 触发响应式更新
  selectedIds.value = new Set(selectedIds.value);
}

function toggleSelectMode() {
  selectMode.value = !selectMode.value;
  if (!selectMode.value) {
    selectedIds.value.clear();
  }
}

function selectAll() {
  filteredImages.value.forEach((img) => selectedIds.value.add(img.id));
  selectedIds.value = new Set(selectedIds.value);
}

function deselectAll() {
  selectedIds.value.clear();
  selectedIds.value = new Set();
}

// 下载原图
async function downloadOriginal() {
  if (!selectedImage.value) return;
  try {
    const blob = await fetchImageAsBlob(selectedImage.value.url);
    downloadBlob(blob, `${selectedImage.value.seed}.png`);
    $q.notify({ type: 'positive', message: '下载已开始' });
  } catch (err) {
    console.error(err);
    $q.notify({ type: 'negative', message: '下载失败' });
  }
}

// 下载去除元数据的图片
async function downloadWithoutMetadata() {
  if (!selectedImage.value) return;
  try {
    const blob = await fetchImageAsBlob(selectedImage.value.url);
    const cleanBlob = await removeMetadata(blob);
    downloadBlob(cleanBlob, `${selectedImage.value.seed}_clean.png`);
    $q.notify({ type: 'positive', message: '下载已开始' });
  } catch (err) {
    console.error(err);
    $q.notify({ type: 'negative', message: '处理失败' });
  }
}

// 复制去除元数据的图片
async function copyWithoutMetadata() {
  if (!selectedImage.value) return;
  try {
    await copyImageToClipboard(selectedImage.value.url);
    $q.notify({ type: 'positive', message: '已复制到剪贴板' });
  } catch (err) {
    console.error(err);
    $q.notify({ type: 'negative', message: '复制失败' });
  }
}

// 批量删除选中的图片
function deleteSelected() {
  if (selectedIds.value.size === 0) return;

  $q.dialog({
    title: '确认删除',
    message: `确定要删除选中的 ${selectedIds.value.size} 张图片吗？此操作不可恢复。`,
    cancel: true,
    persistent: true,
  }).onOk(() => {
    // 获取选中图片对应的 recordId 列表（去重）
    const recordIds = new Set<string>();
    images.value.forEach((img) => {
      if (selectedIds.value.has(img.id)) {
        recordIds.add(img.recordId);
      }
    });

    void (async () => {
      try {
        const deleted = await deleteRecordsBatch(Array.from(recordIds));
        // 从本地列表中移除已删除的图片
        images.value = images.value.filter((img) => !recordIds.has(img.recordId));
        $q.notify({ type: 'positive', message: `已删除 ${deleted} 条生成记录` });
      } catch (err) {
        console.error('Delete failed:', err);
        $q.notify({ type: 'negative', message: '删除失败，请重试' });
      } finally {
        selectedIds.value.clear();
        selectMode.value = false;
      }
    })();
  });
}

// 批量下载选中的图片
async function downloadSelected() {
  if (selectedIds.value.size === 0) return;
  const selectedImgs = images.value.filter((img) => selectedIds.value.has(img.id));
  $q.notify({ type: 'info', message: `开始下载 ${selectedImgs.length} 张图片...` });

  for (const img of selectedImgs) {
    try {
      const blob = await fetchImageAsBlob(img.url);
      downloadBlob(blob, `${img.seed}.png`);
      // 稍微延迟避免浏览器阻止
      await new Promise((r) => setTimeout(r, 300));
    } catch (err) {
      console.error('Download failed:', img.seed, err);
    }
  }
}

// 切换日期 Tab 时重置页码
watch(selectedDateTab, () => {
  page.value = 1;
});

// 初始化选中第一个日期
watch(
  allSortedDates,
  (dates) => {
    if (dates.length > 0 && !selectedDateTab.value) {
      selectedDateTab.value = dates[0] ?? null;
    }
  },
  { immediate: true },
);

// ============== 归档功能 ==============

async function loadArchives() {
  archivesLoading.value = true;
  try {
    const [archiveList, dateList] = await Promise.all([fetchArchives(), fetchArchivableDates()]);
    archives.value = archiveList;
    archivableDates.value = dateList;
    // 重置选择
    selectedArchiveDates.value = new Set();
  } catch (err) {
    console.error('Failed to load archives:', err);
  } finally {
    archivesLoading.value = false;
  }
}

function openArchiveDialog() {
  showArchiveDialog.value = true;
  void loadArchives();
}

function toggleArchiveDateSelect(date: string) {
  if (selectedArchiveDates.value.has(date)) {
    selectedArchiveDates.value.delete(date);
  } else {
    selectedArchiveDates.value.add(date);
  }
  selectedArchiveDates.value = new Set(selectedArchiveDates.value);
}

function selectAllArchiveDates() {
  archivableDates.value.forEach((d) => selectedArchiveDates.value.add(d.date));
  selectedArchiveDates.value = new Set(selectedArchiveDates.value);
}

function deselectAllArchiveDates() {
  selectedArchiveDates.value.clear();
  selectedArchiveDates.value = new Set();
}

function confirmCreateArchive() {
  $q.dialog({
    title: '一键归档全部',
    message:
      '将按日期分别压缩今天之前的所有图片（每天一个 zip 文件），压缩后原图片文件夹和数据库记录将被删除。\n\n此操作不可撤销，确定继续吗？',
    cancel: true,
    persistent: true,
  }).onOk(() => {
    void (async () => {
      archiveCreating.value = true;
      try {
        const result = await createArchive();
        const archiveNames = result.archives.map((a) => a.name).join(', ');
        $q.notify({
          type: 'positive',
          message: `归档创建成功: ${archiveNames}（删除了 ${result.deleted_records} 条记录）`,
          timeout: 5000,
        });
        await loadArchives();
        await load(); // 重新加载画廊（被归档的图片已删除）
      } catch (err: unknown) {
        console.error('Failed to create archive:', err);
        const message = err instanceof Error ? err.message : '创建归档失败';
        $q.notify({ type: 'negative', message });
      } finally {
        archiveCreating.value = false;
      }
    })();
  });
}

function confirmCreateArchiveSelected() {
  const dates = Array.from(selectedArchiveDates.value);
  if (dates.length === 0) {
    $q.notify({ type: 'warning', message: '请先选择要归档的日期' });
    return;
  }

  $q.dialog({
    title: '归档选中日期',
    message: `将归档以下 ${dates.length} 个日期的图片：\n${dates.join(', ')}\n\n压缩后原图片文件夹和数据库记录将被删除。此操作不可撤销，确定继续吗？`,
    cancel: true,
    persistent: true,
  }).onOk(() => {
    void (async () => {
      archiveCreating.value = true;
      try {
        const result = await createArchiveSelected(dates);
        const archiveNames = result.archives.map((a) => a.name).join(', ');
        $q.notify({
          type: 'positive',
          message: `归档创建成功: ${archiveNames}（删除了 ${result.deleted_records} 条记录）`,
          timeout: 5000,
        });
        await loadArchives();
        await load(); // 重新加载画廊
      } catch (err: unknown) {
        console.error('Failed to create archive:', err);
        const message = err instanceof Error ? err.message : '创建归档失败';
        $q.notify({ type: 'negative', message });
      } finally {
        archiveCreating.value = false;
      }
    })();
  });
}

function downloadArchiveFile(name: string) {
  window.open(getArchiveDownloadUrl(name), '_blank');
}

function confirmDeleteArchive(archive: ArchiveInfo) {
  $q.dialog({
    title: '删除归档',
    message: `确定要删除归档 "${archive.name}" 吗？此操作不可恢复。`,
    cancel: true,
    persistent: true,
  }).onOk(() => {
    void (async () => {
      try {
        await deleteArchive(archive.name);
        $q.notify({ type: 'positive', message: '归档已删除' });
        await loadArchives();
      } catch (err) {
        console.error('Failed to delete archive:', err);
        $q.notify({ type: 'negative', message: '删除归档失败' });
      }
    })();
  });
}

function formatFileSize(bytes: number): string {
  if (bytes < 1024) return bytes + ' B';
  if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + ' KB';
  if (bytes < 1024 * 1024 * 1024) return (bytes / 1024 / 1024).toFixed(1) + ' MB';
  return (bytes / 1024 / 1024 / 1024).toFixed(2) + ' GB';
}

onMounted(() => {
  void load();
});

async function load() {
  loading.value = true;
  try {
    const records = await fetchRecentRecords();
    const flattened: GalleryItem[] = [];
    records.forEach((r: GenerationRecord) => {
      r.images.forEach((img, idx) => {
        flattened.push({
          id: `${r.id}-${idx}`,
          url: img.url,
          seed: img.seed,
          width: img.width,
          height: img.height,
          createdAt: r.created_at,
          recordId: r.id,
          prompt: r.raw_prompt,
        });
      });
    });
    images.value = flattened;
  } catch (err) {
    console.error('Failed to load gallery:', err);
  } finally {
    loading.value = false;
  }
}
</script>

<template>
  <q-page padding class="gallery-page">
    <!-- 头部 -->
    <div class="page-header">
      <div class="text-h5">画廊</div>
      <div class="row q-gutter-sm">
        <q-btn flat icon="archive" @click="openArchiveDialog">
          <q-tooltip>归档管理</q-tooltip>
        </q-btn>
        <q-btn
          flat
          :icon="selectMode ? 'close' : 'checklist'"
          :label="selectMode ? '取消' : '选择'"
          @click="toggleSelectMode"
        >
          <q-tooltip>{{ selectMode ? '取消选择模式' : '进入选择模式' }}</q-tooltip>
        </q-btn>
        <q-btn flat icon="refresh" @click="load" :loading="loading">
          <q-tooltip>刷新</q-tooltip>
        </q-btn>
      </div>
    </div>

    <!-- 多选工具栏 -->
    <q-card v-if="selectMode" class="q-mb-md bg-blue-1">
      <q-card-section class="q-py-sm row items-center q-gutter-md">
        <span class="text-body2">已选中 {{ selectedIds.size }} 张图片</span>
        <q-space />
        <q-btn flat dense size="sm" label="全选" @click="selectAll" />
        <q-btn flat dense size="sm" label="取消全选" @click="deselectAll" />
        <q-btn
          flat
          dense
          size="sm"
          icon="download"
          label="下载"
          :disable="selectedIds.size === 0"
          @click="downloadSelected"
        />
        <q-btn
          flat
          dense
          size="sm"
          icon="delete"
          label="删除"
          color="negative"
          :disable="selectedIds.size === 0"
          @click="deleteSelected"
        />
      </q-card-section>
    </q-card>

    <!-- 加载状态 -->
    <q-card v-if="loading" class="flex flex-center q-pa-xl">
      <q-spinner color="primary" size="3em" />
      <span class="q-ml-md text-grey-7">加载中...</span>
    </q-card>

    <!-- 空状态 -->
    <q-card v-else-if="images.length === 0" class="empty-state-card">
      <div class="empty-state">
        <q-icon name="photo_library" size="4rem" color="grey-4" />
        <div class="text-h6 text-grey-6 q-mt-md">暂无生成记录</div>
        <div class="text-body2 text-grey-5">去生成页面创建一些图片吧！</div>
      </div>
    </q-card>

    <!-- 图片内容 -->
    <template v-else>
      <!-- 搜索栏 -->
      <q-card class="q-mb-md">
        <q-card-section class="q-py-sm row items-center q-gutter-md">
          <q-input
            v-model="search"
            placeholder="搜索 Seed 或提示词"
            dense
            filled
            clearable
            class="col"
          >
            <template #prepend>
              <q-icon name="search" />
            </template>
          </q-input>
          <div class="text-caption text-grey-6">共 {{ filteredImages.length }} 张图片</div>
        </q-card-section>
      </q-card>

      <!-- 日期 Tab 导航 -->
      <q-tabs
        v-model="selectedDateTab"
        class="date-tabs q-mb-md"
        align="left"
        dense
        narrow-indicator
        active-color="primary"
        indicator-color="primary"
      >
        <q-tab v-for="date in allSortedDates" :key="date" :name="date" :label="date" no-caps>
          <q-badge floating color="primary" :label="allGroupedImages[date]?.length || 0" />
        </q-tab>
      </q-tabs>

      <!-- 当前日期的图片网格 -->
      <div class="gallery-content">
        <div class="image-grid">
          <div v-for="img in paginatedImages" :key="img.id" class="image-item">
            <q-card
              class="image-card cursor-pointer"
              :class="{ selected: selectMode && selectedIds.has(img.id) }"
              flat
              bordered
              @click="showImage(img)"
            >
              <div class="image-wrapper">
                <q-img :src="img.url" :ratio="1" fit="cover" class="image-thumbnail">
                  <template v-slot:loading>
                    <div class="flex flex-center full-height">
                      <q-spinner-gears color="primary" />
                    </div>
                  </template>
                  <template v-slot:error>
                    <div class="flex flex-center full-height bg-negative text-white">
                      <q-icon name="broken_image" size="2rem" />
                    </div>
                  </template>
                </q-img>
                <div class="image-overlay" :class="{ 'select-mode': selectMode }">
                  <div class="overlay-content">
                    <q-icon
                      v-if="selectMode"
                      :name="selectedIds.has(img.id) ? 'check_circle' : 'radio_button_unchecked'"
                      size="2rem"
                    />
                    <q-icon v-else name="zoom_in" size="1.5rem" />
                  </div>
                </div>
                <!-- 选中指示器 -->
                <div v-if="selectMode && selectedIds.has(img.id)" class="selected-indicator">
                  <q-icon name="check_circle" color="primary" size="1.5rem" />
                </div>
              </div>
              <div class="image-info q-pa-xs">
                <div class="row items-center justify-between no-wrap">
                  <span class="text-caption text-grey-7 ellipsis">Seed: {{ img.seed }}</span>
                  <span class="text-caption text-grey-5">{{ formatTime(img.createdAt) }}</span>
                </div>
              </div>
            </q-card>
          </div>
        </div>
      </div>

      <!-- 分页 -->
      <div class="row items-center justify-center q-mt-md q-gutter-md" v-if="totalPages > 1">
        <q-pagination
          v-model="page"
          :max="totalPages"
          max-pages="7"
          direction-links
          boundary-links
        />
        <span class="text-caption text-grey-6"> {{ currentDateImages.length }} 张图片 </span>
      </div>
    </template>

    <!-- 图片预览对话框 -->
    <q-dialog v-model="showDialog" transition-show="fade" transition-hide="fade">
      <q-card class="preview-dialog column bg-black">
        <q-bar class="bg-dark text-white">
          <q-icon name="photo" />
          <span class="q-ml-sm">图片预览</span>
          <q-space />
          <div class="row items-center q-gutter-sm">
            <q-chip dense color="grey-8" text-color="white" icon="tag">
              {{ currentIndex + 1 }} / {{ navTotal }}
            </q-chip>
            <q-chip dense color="grey-8" text-color="white" icon="fingerprint">
              Seed: {{ selectedImage?.seed }}
            </q-chip>
            <q-chip dense color="grey-8" text-color="white" icon="aspect_ratio">
              {{ selectedImage?.width }} × {{ selectedImage?.height }}
            </q-chip>
          </div>
          <q-space />
          <q-btn
            dense
            flat
            icon="content_copy"
            @click="copyToClipboard(String(selectedImage?.seed))"
          >
            <q-tooltip>复制 Seed</q-tooltip>
          </q-btn>
          <q-btn dense flat icon="download" @click="downloadOriginal">
            <q-tooltip>下载原图</q-tooltip>
          </q-btn>
          <q-btn dense flat icon="image_not_supported" @click="downloadWithoutMetadata">
            <q-tooltip>下载无元数据图片</q-tooltip>
          </q-btn>
          <q-btn dense flat icon="file_copy" @click="copyWithoutMetadata">
            <q-tooltip>复制无元数据图片</q-tooltip>
          </q-btn>
          <q-btn dense flat icon="open_in_new" :href="selectedImage?.url" target="_blank">
            <q-tooltip>在新标签页打开</q-tooltip>
          </q-btn>
          <q-btn dense flat icon="close" v-close-popup />
        </q-bar>

        <q-card-section class="col q-pa-none flex flex-center preview-area">
          <!-- 左导航按钮 -->
          <q-btn
            v-if="hasPrev"
            class="nav-btn nav-btn-left"
            round
            flat
            size="lg"
            icon="chevron_left"
            color="white"
            @click="goToPrev"
          >
            <q-tooltip>上一张 (←)</q-tooltip>
          </q-btn>

          <q-img
            v-if="selectedImage"
            :src="selectedImage.url"
            fit="contain"
            class="preview-image"
          />

          <!-- 右导航按钮 -->
          <q-btn
            v-if="hasNext"
            class="nav-btn nav-btn-right"
            round
            flat
            size="lg"
            icon="chevron_right"
            color="white"
            @click="goToNext"
          >
            <q-tooltip>下一张 (→)</q-tooltip>
          </q-btn>
        </q-card-section>
      </q-card>
    </q-dialog>

    <!-- 归档管理对话框 -->
    <q-dialog v-model="showArchiveDialog">
      <q-card style="min-width: 500px; max-width: 700px">
        <q-bar class="bg-primary text-white">
          <q-icon name="archive" />
          <span class="q-ml-sm">归档管理</span>
          <q-space />
          <q-btn dense flat icon="close" v-close-popup />
        </q-bar>

        <q-card-section>
          <div class="text-body2 text-grey-7 q-mb-md">
            归档功能会将图片压缩成 zip 文件（每天一个），压缩后原图片文件夹和数据库记录将被删除。
            归档后的图片无法在画廊中浏览，但可以下载或删除归档文件。
          </div>

          <!-- 可归档日期选择 -->
          <div v-if="archivableDates.length > 0" class="q-mb-md">
            <div class="row items-center q-mb-sm">
              <div class="text-subtitle2">选择要归档的日期</div>
              <q-space />
              <q-btn flat dense size="sm" label="全选" @click="selectAllArchiveDates" />
              <q-btn flat dense size="sm" label="取消" @click="deselectAllArchiveDates" />
            </div>

            <div class="archivable-dates-grid">
              <q-checkbox
                v-for="d in archivableDates"
                :key="d.date"
                :model-value="selectedArchiveDates.has(d.date)"
                @update:model-value="toggleArchiveDateSelect(d.date)"
                :label="`${d.date} (${d.image_count}张, ${formatFileSize(d.total_size)})`"
                dense
                class="archivable-date-item"
              />
            </div>

            <div class="row q-gutter-sm q-mt-md">
              <q-btn
                color="primary"
                icon="archive"
                :label="`归档选中 (${selectedArchiveDates.size})`"
                :loading="archiveCreating"
                :disable="selectedArchiveDates.size === 0"
                @click="confirmCreateArchiveSelected"
              />
              <q-btn
                outline
                color="primary"
                icon="select_all"
                label="一键归档全部"
                :loading="archiveCreating"
                @click="confirmCreateArchive"
              />
            </div>
          </div>

          <div v-else-if="!archivesLoading" class="text-grey-6 text-center q-pa-md">
            没有可归档的日期（仅今天的图片）
          </div>
        </q-card-section>

        <q-separator />

        <q-card-section>
          <div class="text-subtitle2 q-mb-sm">已有归档</div>

          <div v-if="archivesLoading" class="flex flex-center q-pa-md">
            <q-spinner color="primary" size="2em" />
          </div>

          <div v-else-if="archives.length === 0" class="text-grey-6 text-center q-pa-md">
            暂无归档文件
          </div>

          <q-list v-else separator>
            <q-item v-for="archive in archives" :key="archive.name">
              <q-item-section avatar>
                <q-icon name="folder_zip" color="primary" />
              </q-item-section>
              <q-item-section>
                <q-item-label>{{ archive.name }}</q-item-label>
                <q-item-label caption>
                  {{ formatFileSize(archive.size) }} ·
                  {{ new Date(archive.created_at).toLocaleString('zh-CN') }}
                </q-item-label>
              </q-item-section>
              <q-item-section side>
                <div class="row q-gutter-xs">
                  <q-btn
                    flat
                    round
                    dense
                    icon="download"
                    @click="downloadArchiveFile(archive.name)"
                  >
                    <q-tooltip>下载</q-tooltip>
                  </q-btn>
                  <q-btn
                    flat
                    round
                    dense
                    icon="delete"
                    color="negative"
                    @click="confirmDeleteArchive(archive)"
                  >
                    <q-tooltip>删除</q-tooltip>
                  </q-btn>
                </div>
              </q-item-section>
            </q-item>
          </q-list>
        </q-card-section>
      </q-card>
    </q-dialog>
  </q-page>
</template>

<style scoped lang="scss">
.gallery-page {
  max-width: 1200px;
  margin: 0 auto;
}

.page-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 16px;
}

.archivable-dates-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
  gap: 4px;
  max-height: 200px;
  overflow-y: auto;
  padding: 8px;
  background: rgba(0, 0, 0, 0.02);
  border-radius: 4px;
}

.archivable-date-item {
  font-size: 13px;
}

.empty-state-card {
  min-height: 400px;
}

.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 64px 24px;
}

.gallery-content {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.date-tabs {
  background: var(--q-dark-page, white);
  border-radius: 8px;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);

  :deep(.q-tab) {
    padding-right: 24px;
  }
}

.body--light .date-tabs {
  background: white;
}

.body--dark .date-tabs {
  background: var(--q-dark);
}

.image-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
  gap: 12px;

  @media (min-width: 600px) {
    grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
  }

  @media (min-width: 1024px) {
    grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
  }
}

.image-item {
  aspect-ratio: 1;
}

.image-card {
  height: 100%;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  transition:
    transform 0.2s,
    box-shadow 0.2s,
    border-color 0.2s;

  &:hover {
    transform: translateY(-2px);
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);

    .image-overlay {
      opacity: 1;
    }
  }

  &.selected {
    border: 2px solid var(--q-primary);
    box-shadow: 0 0 0 2px rgba(25, 118, 210, 0.2);
  }
}

.image-wrapper {
  position: relative;
  flex: 1;
  min-height: 0;
  overflow: hidden;
}

.image-thumbnail {
  width: 100%;
  height: 100%;
}

.image-overlay {
  position: absolute;
  inset: 0;
  background: rgba(0, 0, 0, 0.4);
  display: flex;
  align-items: center;
  justify-content: center;
  opacity: 0;
  transition: opacity 0.2s;
  color: white;

  &.select-mode {
    background: rgba(0, 0, 0, 0.3);
  }
}

.selected-indicator {
  position: absolute;
  top: 8px;
  right: 8px;
  background: white;
  border-radius: 50%;
  padding: 2px;
}

.image-info {
  flex-shrink: 0;
  background: rgba(0, 0, 0, 0.02);
  border-top: 1px solid rgba(0, 0, 0, 0.05);
}

.preview-dialog {
  width: 90vw;
  max-width: 1200px;
  height: 90vh;
}

.preview-area {
  background: #000;
  overflow: hidden;
  position: relative;
}

.preview-image {
  max-height: calc(100vh - 80px);
  max-width: 100%;
}

.nav-btn {
  position: absolute;
  top: 50%;
  transform: translateY(-50%);
  z-index: 10;
  background: rgba(0, 0, 0, 0.5);
  transition: all 0.2s;

  &:hover {
    background: rgba(0, 0, 0, 0.7);
  }
}

.nav-btn-left {
  left: 16px;
}

.nav-btn-right {
  right: 16px;
}
</style>
