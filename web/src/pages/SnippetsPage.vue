<script setup lang="ts">
import { onMounted, ref, watch, computed } from 'vue';
import { useQuasar } from 'quasar';
import { useClipboard, useDebounceFn, useEventListener } from '@vueuse/core';
import {
  createSnippet,
  updateSnippet,
  fetchSnippets,
  fetchSnippet,
  deleteSnippet,
  deleteSnippetPreview,
  renameSnippet,
  type SnippetSummary,
  previewsBase,
} from 'src/services/api';
import PromptEditor from 'src/components/PromptEditor.vue';
import { useImageUpload } from 'src/composables';

const { notify, dialog } = useQuasar();
const { copy } = useClipboard();

const search = ref('');
const snippets = ref<SnippetSummary[]>([]);
const total = ref(0);
const limit = ref(20);
const page = ref(1);
const loading = ref(false);
const deleting = ref(false);

const openDialog = ref(false);
const editingId = ref<string | null>(null);
const originalName = ref<string>(''); // 保存编辑时的原始名称
const editLoading = ref(false);
const form = ref({
  name: '',
  category: '',
  tags: '',
  description: '',
  content: '',
  existingPreviewPath: null as string | null,
});
const saving = ref(false);

// 图片上传
const {
  previewUrl,
  base64Data,
  loading: imageLoading,
  error: imageError,
  selectFile,
  handleFileChange,
  handleDrop,
  handlePaste,
  clearImage,
  reset: resetImage,
  fileInputRef,
} = useImageUpload();

// 全局粘贴事件监听（对话框打开时生效）
useEventListener('paste', (event: ClipboardEvent) => {
  if (!openDialog.value) return;
  // 避免在输入框中粘贴时触发
  const target = event.target as HTMLElement;
  if (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA') return;
  void handlePaste(event);
});

// 预览大图对话框
const showFullPreview = ref(false);

// 整体页面锁定状态（任何异步操作期间）
const isLocked = computed(
  () => loading.value || saving.value || deleting.value || editLoading.value || imageLoading.value,
);

// 当前预览图 URL（可能是新上传的或已存在的）
const currentPreviewUrl = computed(() => {
  if (previewUrl.value) return previewUrl.value;
  if (form.value.existingPreviewPath) {
    return `${previewsBase}/${form.value.existingPreviewPath}`;
  }
  return null;
});

const dialogTitle = computed(() => (editingId.value ? '编辑 Snippet' : '新建 Snippet'));

// 分组显示的snippets（按分类）
const groupedSnippets = computed(() => {
  const groups: Record<string, SnippetSummary[]> = {};
  for (const snip of snippets.value) {
    const cat = snip.category || '未分类';
    if (!groups[cat]) {
      groups[cat] = [];
    }
    groups[cat].push(snip);
  }
  return groups;
});

const categories = computed(() => Object.keys(groupedSnippets.value).sort());

function getSnippetsForCategory(category: string): SnippetSummary[] {
  return groupedSnippets.value[category] ?? [];
}

async function load() {
  loading.value = true;
  try {
    const params: { q?: string; offset: number; limit: number } = {
      offset: (page.value - 1) * limit.value,
      limit: limit.value,
    };
    const query = search.value.trim();
    if (query) params.q = query;

    const res = await fetchSnippets(params);
    snippets.value = res.items;
    total.value = res.total;
  } catch (err) {
    console.error(err);
    notify({ type: 'negative', message: '加载失败' });
  } finally {
    loading.value = false;
  }
}

function openCreate() {
  editingId.value = null;
  originalName.value = '';
  form.value = {
    name: '',
    category: '',
    tags: '',
    description: '',
    content: '',
    existingPreviewPath: null,
  };
  resetImage();
  openDialog.value = true;
}

async function openEdit(id: string) {
  editLoading.value = true;
  try {
    const snippet = await fetchSnippet(id);
    editingId.value = id;
    originalName.value = snippet.name; // 保存原始名称
    form.value = {
      name: snippet.name,
      category: snippet.category,
      tags: snippet.tags.join(', '),
      description: snippet.description || '',
      content: snippet.content,
      existingPreviewPath: snippet.preview_path || null,
    };
    resetImage();
    openDialog.value = true;
  } catch (err) {
    console.error(err);
    notify({ type: 'negative', message: '加载 Snippet 失败' });
  } finally {
    editLoading.value = false;
  }
}

async function save() {
  if (!form.value.name.trim()) {
    notify({ type: 'warning', message: '请输入名称' });
    return;
  }
  if (!form.value.content.trim()) {
    notify({ type: 'warning', message: '请输入内容' });
    return;
  }

  saving.value = true;
  try {
    const parsedTags = form.value.tags
      .split(',')
      .map((s) => s.trim())
      .filter(Boolean);

    if (editingId.value) {
      // 更新
      const newName = form.value.name.trim();
      const isRenamed = originalName.value && originalName.value !== newName;

      // 如果名称变了，先调用 rename API
      if (isRenamed) {
        const renameResult = await renameSnippet(editingId.value, newName);

        // 构建更新消息
        const messages: string[] = ['已重命名'];
        if (renameResult.updated_presets > 0 || renameResult.updated_settings) {
          const parts: string[] = [];
          if (renameResult.updated_presets > 0) {
            parts.push(`${renameResult.updated_presets} 个角色预设`);
          }
          if (renameResult.updated_settings) {
            parts.push('生成页设置');
          }
          messages.push(`已更新 ${parts.join(' 和 ')} 中的引用`);
        }
        notify({
          type: 'positive',
          message: messages.join('，'),
          timeout: renameResult.updated_presets > 0 || renameResult.updated_settings ? 5000 : 2000,
        });
      }

      // 更新其他字段（如果已重命名，排除 name，因为已经通过 rename 更新了）
      const payload: {
        name?: string;
        category?: string;
        content?: string;
        description?: string;
        tags?: string[];
        preview_base64?: string;
      } = {
        category: form.value.category || '默认',
        content: form.value.content,
      };
      // 只有在没有重命名时才更新 name
      if (!isRenamed) {
        payload.name = newName;
      }
      if (form.value.description) payload.description = form.value.description;
      if (parsedTags.length > 0) payload.tags = parsedTags;
      if (base64Data.value) payload.preview_base64 = base64Data.value;

      await updateSnippet(editingId.value, payload);
      if (!isRenamed) {
        notify({ type: 'positive', message: '已更新' });
      }
    } else {
      // 新建
      const payload: {
        name: string;
        category: string;
        content: string;
        description?: string;
        tags?: string[];
        preview_base64?: string;
      } = {
        name: form.value.name,
        category: form.value.category || '默认',
        content: form.value.content,
      };
      if (form.value.description) payload.description = form.value.description;
      if (parsedTags.length > 0) payload.tags = parsedTags;
      if (base64Data.value) payload.preview_base64 = base64Data.value;

      await createSnippet(payload);
      notify({ type: 'positive', message: '已创建' });
    }
    openDialog.value = false;
    await load();
  } catch (err) {
    console.error(err);
    notify({ type: 'negative', message: '保存失败' });
  } finally {
    saving.value = false;
  }
}

async function removePreview() {
  if (!editingId.value) {
    clearImage();
    form.value.existingPreviewPath = null;
    return;
  }

  try {
    await deleteSnippetPreview(editingId.value);
    form.value.existingPreviewPath = null;
    clearImage();
    notify({ type: 'positive', message: '预览图已删除' });
  } catch (err) {
    console.error(err);
    notify({ type: 'negative', message: '删除预览图失败' });
  }
}

function confirmDelete(snippet: SnippetSummary) {
  dialog({
    title: '确认删除',
    message: `确定要删除 Snippet "${snippet.name}" 吗？此操作不可恢复。`,
    cancel: true,
    persistent: true,
  }).onOk(() => {
    void (async () => {
      deleting.value = true;
      try {
        await deleteSnippet(snippet.id);
        notify({ type: 'positive', message: '已删除' });
        await load();
      } catch (err) {
        console.error(err);
        notify({ type: 'negative', message: '删除失败' });
      } finally {
        deleting.value = false;
      }
    })();
  });
}

function copySnippetTag(name: string) {
  const tag = `<snippet:${name}>`;
  void copy(tag).then(() => {
    notify({ type: 'positive', message: '已复制到剪贴板', timeout: 1500 });
  });
}

const debouncedLoad = useDebounceFn(() => {
  page.value = 1;
  void load();
}, 300);

function onSearchInput() {
  void debouncedLoad();
}

onMounted(() => {
  void load();
});
watch(page, () => {
  void load();
});
</script>

<template>
  <q-page padding class="snippets-page">
    <!-- 头部 -->
    <div class="page-header">
      <div class="text-h5">Snippet 管理</div>
      <q-btn
        color="primary"
        icon="add"
        label="新建"
        @click="openCreate"
        :disable="isLocked"
        :loading="editLoading"
      />
    </div>

    <!-- 搜索栏 -->
    <q-card class="q-mb-md">
      <q-card-section class="q-py-sm">
        <q-input
          v-model="search"
          placeholder="搜索 Snippet（名称、描述、标签）"
          dense
          filled
          clearable
          @update:model-value="onSearchInput"
        >
          <template #prepend>
            <q-icon name="search" />
          </template>
        </q-input>
      </q-card-section>
    </q-card>

    <!-- 内容区 -->
    <q-card class="content-card">
      <!-- 加载状态 -->
      <div v-if="loading" class="flex flex-center q-pa-xl">
        <q-spinner color="primary" size="3em" />
      </div>

      <!-- 空状态 -->
      <div v-else-if="snippets.length === 0" class="empty-state">
        <q-icon name="code" size="4rem" color="grey-4" />
        <div class="text-h6 text-grey-6 q-mt-md">暂无 Snippet</div>
        <div class="text-body2 text-grey-5">
          Snippet 是可复用的提示词片段，使用 &lt;snippet:名称&gt; 语法引用
        </div>
      </div>

      <!-- Snippet 列表 - 按分类分组 -->
      <div v-else class="q-pa-md">
        <template v-for="category in categories" :key="category">
          <div class="category-header">
            <q-icon name="folder" color="primary" size="sm" class="q-mr-sm" />
            <span class="text-subtitle1">{{ category }}</span>
            <q-badge :label="getSnippetsForCategory(category).length" class="q-ml-sm" />
          </div>

          <div class="snippet-grid">
            <q-card
              v-for="snip in getSnippetsForCategory(category)"
              :key="snip.id"
              class="snippet-card"
              flat
              bordered
            >
              <!-- 预览图 -->
              <q-img
                v-if="snip.preview_path"
                :src="`${previewsBase}/${snip.preview_path}`"
                :ratio="16 / 9"
                fit="cover"
                class="snippet-preview"
              >
                <template v-slot:loading>
                  <div class="flex flex-center full-height">
                    <q-spinner-gears color="primary" size="1.5rem" />
                  </div>
                </template>
              </q-img>

              <q-card-section class="q-pb-sm">
                <div class="row items-center no-wrap">
                  <q-icon name="code" color="secondary" size="sm" class="q-mr-sm" />
                  <div class="text-subtitle1 ellipsis snippet-name">{{ snip.name }}</div>
                  <q-space />
                  <q-btn flat round dense icon="more_vert" size="sm">
                    <q-menu>
                      <q-list dense style="min-width: 140px">
                        <q-item clickable v-close-popup @click="copySnippetTag(snip.name)">
                          <q-item-section avatar>
                            <q-icon name="content_copy" size="xs" />
                          </q-item-section>
                          <q-item-section>复制标签</q-item-section>
                        </q-item>
                        <q-item clickable v-close-popup @click="openEdit(snip.id)">
                          <q-item-section avatar>
                            <q-icon name="edit" size="xs" />
                          </q-item-section>
                          <q-item-section>编辑</q-item-section>
                        </q-item>
                        <q-separator />
                        <q-item clickable v-close-popup @click="confirmDelete(snip)">
                          <q-item-section avatar>
                            <q-icon name="delete" size="xs" color="negative" />
                          </q-item-section>
                          <q-item-section class="text-negative">删除</q-item-section>
                        </q-item>
                      </q-list>
                    </q-menu>
                  </q-btn>
                </div>

                <div class="snippet-tag q-mt-xs">
                  <code class="text-caption">&lt;snippet:{{ snip.name }}&gt;</code>
                  <q-btn
                    flat
                    round
                    dense
                    size="xs"
                    icon="content_copy"
                    @click.stop="copySnippetTag(snip.name)"
                  >
                    <q-tooltip>复制标签</q-tooltip>
                  </q-btn>
                </div>

                <div
                  v-if="snip.description"
                  class="text-body2 text-grey-7 q-mt-sm ellipsis-2-lines"
                >
                  {{ snip.description }}
                </div>
              </q-card-section>

              <q-separator />

              <q-card-section class="q-py-sm">
                <div class="row items-center q-gutter-xs">
                  <q-chip
                    v-for="tag in snip.tags.slice(0, 3)"
                    :key="tag"
                    dense
                    size="sm"
                    color="grey-3"
                    text-color="grey-8"
                  >
                    {{ tag }}
                  </q-chip>
                  <q-chip
                    v-if="snip.tags.length > 3"
                    dense
                    size="sm"
                    color="grey-2"
                    text-color="grey-6"
                  >
                    +{{ snip.tags.length - 3 }}
                  </q-chip>
                  <q-space />
                  <q-btn
                    flat
                    dense
                    size="sm"
                    color="primary"
                    icon="edit"
                    @click="openEdit(snip.id)"
                    :disable="isLocked"
                  />
                </div>
              </q-card-section>
            </q-card>
          </div>
        </template>
      </div>

      <!-- 分页 -->
      <q-separator v-if="snippets.length > 0" />
      <q-card-section v-if="snippets.length > 0" class="row items-center justify-between">
        <div class="text-caption text-grey-6">共 {{ total }} 条</div>
        <q-pagination
          v-model="page"
          :max="Math.max(1, Math.ceil(total / limit))"
          max-pages="7"
          size="sm"
          direction-links
          boundary-links
        />
      </q-card-section>
    </q-card>

    <!-- 新建/编辑对话框 -->
    <q-dialog v-model="openDialog" persistent>
      <q-card style="width: 600px; max-width: 90vw">
        <q-card-section class="row items-center q-pb-none">
          <div class="text-h6">{{ dialogTitle }}</div>
          <q-space />
          <q-btn icon="close" flat round dense v-close-popup />
        </q-card-section>

        <q-separator class="q-mt-sm" />

        <q-card-section>
          <q-form class="q-gutter-sm">
            <q-input
              v-model="form.name"
              label="名称 *"
              filled
              dense
              lazy-rules
              :rules="[
                (v) => !!v || '名称不能为空',
                (v) => !/[\s<>,]/.test(v) || '名称不能包含空格或特殊字符',
              ]"
            >
              <template #hint>
                <span class="text-caption">用于 &lt;snippet:名称&gt; 引用</span>
              </template>
            </q-input>

            <q-input v-model="form.category" label="分类" filled dense hint="用于分组显示" />

            <!-- 标签 -->
            <q-input v-model="form.tags" label="标签" filled dense hint="多个标签用逗号分隔" />

            <!-- 描述 -->
            <q-input
              v-model="form.description"
              type="textarea"
              label="描述"
              filled
              dense
              autogrow
              :input-style="{ minHeight: '50px', maxHeight: '100px' }"
              hint="可选，简要说明该 Snippet 的用途"
            />

            <!-- 预览图 -->
            <div class="preview-upload-section" @paste="handlePaste">
              <div class="text-caption text-grey-7 q-mb-sm">
                预览图（可选）
                <span class="text-grey-5">- 支持 Ctrl+V 粘贴</span>
              </div>
              <input
                ref="fileInputRef"
                type="file"
                accept="image/png,image/jpeg,image/webp"
                style="display: none"
                @change="handleFileChange"
              />

              <div
                v-if="!currentPreviewUrl"
                class="upload-area"
                tabindex="0"
                @click="selectFile"
                @drop.prevent="handleDrop"
                @dragover.prevent
                @paste="handlePaste"
              >
                <q-icon name="add_photo_alternate" size="2rem" color="grey-5" />
                <div class="text-caption text-grey-6 q-mt-sm">点击、拖拽或粘贴上传预览图</div>
                <div class="text-caption text-grey-5">支持 PNG/JPEG/WebP，最大 5MB</div>
              </div>

              <div v-else class="preview-container">
                <q-img
                  :src="currentPreviewUrl"
                  fit="contain"
                  class="preview-image cursor-pointer"
                  @click="showFullPreview = true"
                >
                  <q-tooltip>点击查看完整图片</q-tooltip>
                </q-img>
                <div class="preview-actions">
                  <q-btn round flat dense icon="fullscreen" @click="showFullPreview = true">
                    <q-tooltip>查看完整图片</q-tooltip>
                  </q-btn>
                  <q-btn round flat dense icon="edit" @click="selectFile">
                    <q-tooltip>更换图片</q-tooltip>
                  </q-btn>
                  <q-btn round flat dense icon="delete" color="negative" @click="removePreview">
                    <q-tooltip>删除图片</q-tooltip>
                  </q-btn>
                </div>
              </div>

              <div v-if="imageError" class="text-negative text-caption q-mt-xs">
                {{ imageError }}
              </div>
            </div>

            <!-- 内容 -->
            <div>
              <PromptEditor
                v-model="form.content"
                label="内容 *"
                min-height="120px"
                placeholder="Snippet 展开后的实际提示词内容"
              />
            </div>
          </q-form>
        </q-card-section>

        <q-separator />

        <q-card-actions align="right" class="q-pa-md">
          <q-btn flat label="取消" v-close-popup />
          <q-btn color="primary" label="保存" :loading="saving" @click="save" />
        </q-card-actions>
      </q-card>
    </q-dialog>

    <!-- 预览图大图对话框 -->
    <q-dialog v-model="showFullPreview">
      <q-card class="full-preview-card bg-black">
        <q-bar class="bg-dark text-white">
          <q-icon name="image" />
          <span class="q-ml-sm">预览图</span>
          <q-space />
          <q-btn dense flat icon="close" v-close-popup />
        </q-bar>
        <q-card-section class="flex flex-center q-pa-none full-preview-content">
          <q-img :src="currentPreviewUrl ?? undefined" fit="contain" class="full-preview-image" />
        </q-card-section>
      </q-card>
    </q-dialog>
  </q-page>
</template>

<style scoped lang="scss">
.snippets-page {
  max-width: 1200px;
  margin: 0 auto;
}

.page-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 16px;
}

.content-card {
  min-height: 400px;
}

.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 64px 24px;
}

.category-header {
  display: flex;
  align-items: center;
  padding: 8px 0;
  margin-bottom: 12px;
  border-bottom: 1px solid rgba(0, 0, 0, 0.08);
}

.snippet-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
  gap: 16px;
  margin-bottom: 24px;
}

.snippet-card {
  transition: box-shadow 0.2s;

  &:hover {
    box-shadow: 0 2px 12px rgba(0, 0, 0, 0.1);
  }
}

.snippet-name {
  flex: 1;
  min-width: 0;
}

.snippet-tag {
  display: flex;
  align-items: center;
  gap: 4px;
  background: rgba(0, 0, 0, 0.03);
  padding: 4px 8px;
  border-radius: 4px;
  width: fit-content;

  code {
    color: var(--q-primary);
  }
}

.snippet-preview {
  border-bottom: 1px solid rgba(0, 0, 0, 0.08);
}

.ellipsis-2-lines {
  display: -webkit-box;
  line-clamp: 2;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

// 预览图上传样式
.preview-upload-section {
  margin-top: 8px;
}

.upload-area {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 24px;
  border: 2px dashed rgba(0, 0, 0, 0.12);
  border-radius: 8px;
  cursor: pointer;
  transition: all 0.2s;

  &:hover,
  &:focus {
    border-color: var(--q-primary);
    background: rgba(25, 118, 210, 0.04);
    outline: none;
  }
}

.preview-container {
  position: relative;
  border-radius: 8px;
  overflow: hidden;
  border: 1px solid rgba(0, 0, 0, 0.12);

  .preview-image {
    max-height: 200px;
    width: 100%;
  }

  .preview-actions {
    position: absolute;
    top: 8px;
    right: 8px;
    display: flex;
    gap: 4px;
    background: rgba(255, 255, 255, 0.9);
    border-radius: 4px;
    padding: 2px;
  }
}

.full-preview-card {
  max-width: 90vw;
  max-height: 90vh;
}

.full-preview-content {
  max-height: calc(90vh - 40px);
  overflow: auto;
}

.full-preview-image {
  max-width: 100%;
  max-height: calc(90vh - 60px);
}
</style>
