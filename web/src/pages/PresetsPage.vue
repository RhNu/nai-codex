<script setup lang="ts">
import { onMounted, ref, watch, computed } from 'vue';
import { useQuasar } from 'quasar';
import {
  createPreset,
  fetchPresets,
  fetchPreset,
  updatePreset,
  deletePreset,
  deletePresetPreview,
  fetchMainPresets,
  fetchMainPreset,
  createMainPreset,
  updateMainPreset,
  deleteMainPreset,
  type PresetSummary,
  type MainPreset,
  previewsBase,
} from 'src/services/api';
import PromptEditor from 'src/components/PromptEditor.vue';
import { useImageUpload } from 'src/composables';

const { notify, dialog } = useQuasar();

// Tab 状态
const activeTab = ref<'character' | 'main'>('character');

// ============== 角色预设状态 ==============
const presets = ref<PresetSummary[]>([]);
const total = ref(0);
const limit = ref(20);
const page = ref(1);
const loading = ref(false);
const deleting = ref(false);

const openDialog = ref(false);
const editingId = ref<string | null>(null);
const editLoading = ref(false);
const form = ref({
  name: '',
  description: '',
  before: '',
  after: '',
  replace: '',
  uc_before: '',
  uc_after: '',
  uc_replace: '',
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

// ============== 主预设状态 ==============
const mainPresets = ref<MainPreset[]>([]);
const mainTotal = ref(0);
const mainLimit = ref(20);
const mainPage = ref(1);
const mainLoading = ref(false);
const mainDeleting = ref(false);

const mainOpenDialog = ref(false);
const mainEditingId = ref<string | null>(null);
const mainEditLoading = ref(false);
const mainForm = ref({
  name: '',
  description: '',
  before: '',
  after: '',
  replace: '',
  uc_before: '',
  uc_after: '',
  uc_replace: '',
});
const mainSaving = ref(false);

// ============== 计算属性 ==============

// 角色预设锁定状态
const isLocked = computed(
  () => loading.value || saving.value || deleting.value || editLoading.value || imageLoading.value,
);

// 主预设锁定状态
const isMainLocked = computed(
  () => mainLoading.value || mainSaving.value || mainDeleting.value || mainEditLoading.value,
);

// 当前预览图 URL（可能是新上传的或已存在的）
const currentPreviewUrl = computed(() => {
  if (previewUrl.value) return previewUrl.value;
  if (form.value.existingPreviewPath) {
    return `${previewsBase}/${form.value.existingPreviewPath}`;
  }
  return null;
});

const dialogTitle = computed(() => (editingId.value ? '编辑角色预设' : '新建角色预设'));
const mainDialogTitle = computed(() => (mainEditingId.value ? '编辑主预设' : '新建主预设'));

// ============== 角色预设方法 ==============

async function load() {
  loading.value = true;
  try {
    const res = await fetchPresets({ offset: (page.value - 1) * limit.value, limit: limit.value });
    presets.value = res.items;
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
  form.value = {
    name: '',
    description: '',
    before: '',
    after: '',
    replace: '',
    uc_before: '',
    uc_after: '',
    uc_replace: '',
    existingPreviewPath: null,
  };
  resetImage();
  openDialog.value = true;
}

async function openEdit(id: string) {
  editLoading.value = true;
  try {
    const preset = await fetchPreset(id);
    editingId.value = id;
    form.value = {
      name: preset.name,
      description: preset.description || '',
      before: preset.before || '',
      after: preset.after || '',
      replace: preset.replace || '',
      uc_before: preset.uc_before || '',
      uc_after: preset.uc_after || '',
      uc_replace: preset.uc_replace || '',
      existingPreviewPath: preset.preview_path || null,
    };
    resetImage();
    openDialog.value = true;
  } catch (err) {
    console.error(err);
    notify({ type: 'negative', message: '加载预设失败' });
  } finally {
    editLoading.value = false;
  }
}

async function save() {
  if (!form.value.name.trim()) {
    notify({ type: 'warning', message: '请输入名称' });
    return;
  }

  saving.value = true;
  try {
    const payload: {
      name: string;
      description?: string;
      before?: string;
      after?: string;
      replace?: string;
      uc_before?: string;
      uc_after?: string;
      uc_replace?: string;
      preview_base64?: string;
    } = {
      name: form.value.name,
    };
    if (form.value.description) payload.description = form.value.description;
    if (form.value.before) payload.before = form.value.before;
    if (form.value.after) payload.after = form.value.after;
    if (form.value.replace) payload.replace = form.value.replace;
    if (form.value.uc_before) payload.uc_before = form.value.uc_before;
    if (form.value.uc_after) payload.uc_after = form.value.uc_after;
    if (form.value.uc_replace) payload.uc_replace = form.value.uc_replace;
    if (base64Data.value) payload.preview_base64 = base64Data.value;

    if (editingId.value) {
      await updatePreset(editingId.value, payload);
      notify({ type: 'positive', message: '已更新' });
    } else {
      await createPreset(payload);
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
    await deletePresetPreview(editingId.value);
    form.value.existingPreviewPath = null;
    clearImage();
    notify({ type: 'positive', message: '预览图已删除' });
  } catch (err) {
    console.error(err);
    notify({ type: 'negative', message: '删除预览图失败' });
  }
}

function confirmDelete(preset: PresetSummary) {
  dialog({
    title: '确认删除',
    message: `确定要删除角色预设"${preset.name}"吗？此操作不可恢复。`,
    cancel: true,
    persistent: true,
  }).onOk(() => {
    void (async () => {
      deleting.value = true;
      try {
        await deletePreset(preset.id);
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

// ============== 主预设方法 ==============

async function loadMainPresets() {
  mainLoading.value = true;
  try {
    const res = await fetchMainPresets({
      offset: (mainPage.value - 1) * mainLimit.value,
      limit: mainLimit.value,
    });
    mainPresets.value = res.items;
    mainTotal.value = res.total;
  } catch (err) {
    console.error(err);
    notify({ type: 'negative', message: '加载主预设失败' });
  } finally {
    mainLoading.value = false;
  }
}

function openMainCreate() {
  mainEditingId.value = null;
  mainForm.value = {
    name: '',
    description: '',
    before: '',
    after: '',
    replace: '',
    uc_before: '',
    uc_after: '',
    uc_replace: '',
  };
  mainOpenDialog.value = true;
}

async function openMainEdit(id: string) {
  mainEditLoading.value = true;
  try {
    const preset = await fetchMainPreset(id);
    mainEditingId.value = id;
    mainForm.value = {
      name: preset.name,
      description: preset.description || '',
      before: preset.before || '',
      after: preset.after || '',
      replace: preset.replace || '',
      uc_before: preset.uc_before || '',
      uc_after: preset.uc_after || '',
      uc_replace: preset.uc_replace || '',
    };
    mainOpenDialog.value = true;
  } catch (err) {
    console.error(err);
    notify({ type: 'negative', message: '加载主预设失败' });
  } finally {
    mainEditLoading.value = false;
  }
}

async function saveMainPreset() {
  if (!mainForm.value.name.trim()) {
    notify({ type: 'warning', message: '请输入名称' });
    return;
  }

  mainSaving.value = true;
  try {
    const payload: {
      name: string;
      description?: string;
      before?: string;
      after?: string;
      replace?: string;
      uc_before?: string;
      uc_after?: string;
      uc_replace?: string;
    } = {
      name: mainForm.value.name,
    };
    if (mainForm.value.description) payload.description = mainForm.value.description;
    if (mainForm.value.before) payload.before = mainForm.value.before;
    if (mainForm.value.after) payload.after = mainForm.value.after;
    if (mainForm.value.replace) payload.replace = mainForm.value.replace;
    if (mainForm.value.uc_before) payload.uc_before = mainForm.value.uc_before;
    if (mainForm.value.uc_after) payload.uc_after = mainForm.value.uc_after;
    if (mainForm.value.uc_replace) payload.uc_replace = mainForm.value.uc_replace;

    if (mainEditingId.value) {
      await updateMainPreset(mainEditingId.value, payload);
      notify({ type: 'positive', message: '已更新' });
    } else {
      await createMainPreset(payload);
      notify({ type: 'positive', message: '已创建' });
    }
    mainOpenDialog.value = false;
    await loadMainPresets();
  } catch (err) {
    console.error(err);
    notify({ type: 'negative', message: '保存失败' });
  } finally {
    mainSaving.value = false;
  }
}

function confirmDeleteMainPreset(preset: MainPreset) {
  dialog({
    title: '确认删除',
    message: `确定要删除主预设"${preset.name}"吗？此操作不可恢复。`,
    cancel: true,
    persistent: true,
  }).onOk(() => {
    void (async () => {
      mainDeleting.value = true;
      try {
        await deleteMainPreset(preset.id);
        notify({ type: 'positive', message: '已删除' });
        await loadMainPresets();
      } catch (err) {
        console.error(err);
        notify({ type: 'negative', message: '删除失败' });
      } finally {
        mainDeleting.value = false;
      }
    })();
  });
}

// ============== 生命周期 ==============

onMounted(() => {
  void load();
  void loadMainPresets();
});

watch(page, () => {
  void load();
});

watch(mainPage, () => {
  void loadMainPresets();
});
</script>

<template>
  <q-page padding class="presets-page">
    <!-- 头部 -->
    <div class="page-header">
      <div class="text-h5">预设管理</div>
      <q-btn
        v-if="activeTab === 'character'"
        color="primary"
        icon="add"
        label="新建角色预设"
        @click="openCreate"
        :disable="isLocked"
        :loading="editLoading"
      />
      <q-btn
        v-else
        color="primary"
        icon="add"
        label="新建主预设"
        @click="openMainCreate"
        :disable="isMainLocked"
        :loading="mainEditLoading"
      />
    </div>

    <!-- Tab 导航 -->
    <q-tabs
      v-model="activeTab"
      dense
      class="text-grey"
      active-color="primary"
      indicator-color="primary"
      align="left"
      narrow-indicator
    >
      <q-tab name="character" label="角色预设" icon="person" />
      <q-tab name="main" label="主预设" icon="tune" />
    </q-tabs>

    <q-separator />

    <!-- Tab 内容面板 -->
    <q-tab-panels v-model="activeTab" animated class="tab-panels">
      <!-- 角色预设面板 -->
      <q-tab-panel name="character" class="q-pa-none">
        <q-card class="content-card" flat>
          <!-- 加载状态 -->
          <div v-if="loading" class="flex flex-center q-pa-xl">
            <q-spinner color="primary" size="3em" />
          </div>

          <!-- 空状态 -->
          <div v-else-if="presets.length === 0" class="empty-state">
            <q-icon name="person" size="4rem" color="grey-4" />
            <div class="text-h6 text-grey-6 q-mt-md">暂无角色预设</div>
            <div class="text-body2 text-grey-5">
              点击上方"新建角色预设"按钮创建你的第一个角色预设
            </div>
          </div>

          <!-- 预设列表 - 卡片网格 -->
          <div v-else class="preset-grid q-pa-md">
            <q-card v-for="p in presets" :key="p.id" class="preset-card" flat bordered>
              <!-- 预览图 -->
              <q-img
                v-if="p.preview_path"
                :src="`${previewsBase}/${p.preview_path}`"
                :ratio="16 / 9"
                fit="cover"
                class="preset-preview"
              >
                <template v-slot:loading>
                  <div class="flex flex-center full-height">
                    <q-spinner-gears color="primary" size="1.5rem" />
                  </div>
                </template>
              </q-img>

              <q-card-section>
                <div class="row items-center no-wrap">
                  <q-icon name="person" color="primary" size="sm" class="q-mr-sm" />
                  <div class="text-subtitle1 ellipsis">{{ p.name }}</div>
                  <q-space />
                  <q-btn flat round dense icon="more_vert" size="sm">
                    <q-menu>
                      <q-list dense style="min-width: 120px">
                        <q-item clickable v-close-popup @click="openEdit(p.id)">
                          <q-item-section avatar>
                            <q-icon name="edit" size="xs" />
                          </q-item-section>
                          <q-item-section>编辑</q-item-section>
                        </q-item>
                        <q-item clickable v-close-popup @click="confirmDelete(p)">
                          <q-item-section avatar>
                            <q-icon name="delete" size="xs" color="negative" />
                          </q-item-section>
                          <q-item-section class="text-negative">删除</q-item-section>
                        </q-item>
                      </q-list>
                    </q-menu>
                  </q-btn>
                </div>
                <div v-if="p.description" class="text-body2 text-grey-7 q-mt-sm ellipsis-2-lines">
                  {{ p.description }}
                </div>
                <div v-else class="text-body2 text-grey-5 q-mt-sm">暂无描述</div>
              </q-card-section>

              <q-separator />

              <q-card-actions>
                <q-btn
                  flat
                  dense
                  size="sm"
                  color="primary"
                  icon="edit"
                  label="编辑"
                  @click="openEdit(p.id)"
                  :disable="isLocked"
                />
              </q-card-actions>
            </q-card>
          </div>

          <!-- 分页 -->
          <q-separator v-if="presets.length > 0" />
          <q-card-section v-if="presets.length > 0" class="row items-center justify-between">
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
      </q-tab-panel>

      <!-- 主预设面板 -->
      <q-tab-panel name="main" class="q-pa-none">
        <q-card class="content-card" flat>
          <!-- 加载状态 -->
          <div v-if="mainLoading" class="flex flex-center q-pa-xl">
            <q-spinner color="primary" size="3em" />
          </div>

          <!-- 空状态 -->
          <div v-else-if="mainPresets.length === 0" class="empty-state">
            <q-icon name="tune" size="4rem" color="grey-4" />
            <div class="text-h6 text-grey-6 q-mt-md">暂无主预设</div>
            <div class="text-body2 text-grey-5">点击上方"新建主预设"按钮创建你的第一个主预设</div>
          </div>

          <!-- 主预设列表 -->
          <div v-else class="preset-grid q-pa-md">
            <q-card v-for="p in mainPresets" :key="p.id" class="preset-card" flat bordered>
              <q-card-section>
                <div class="row items-center no-wrap">
                  <q-icon name="tune" color="secondary" size="sm" class="q-mr-sm" />
                  <div class="text-subtitle1 ellipsis">{{ p.name }}</div>
                  <q-space />
                  <q-btn flat round dense icon="more_vert" size="sm">
                    <q-menu>
                      <q-list dense style="min-width: 120px">
                        <q-item clickable v-close-popup @click="openMainEdit(p.id)">
                          <q-item-section avatar>
                            <q-icon name="edit" size="xs" />
                          </q-item-section>
                          <q-item-section>编辑</q-item-section>
                        </q-item>
                        <q-item clickable v-close-popup @click="confirmDeleteMainPreset(p)">
                          <q-item-section avatar>
                            <q-icon name="delete" size="xs" color="negative" />
                          </q-item-section>
                          <q-item-section class="text-negative">删除</q-item-section>
                        </q-item>
                      </q-list>
                    </q-menu>
                  </q-btn>
                </div>
                <div v-if="p.description" class="text-body2 text-grey-7 q-mt-sm ellipsis-2-lines">
                  {{ p.description }}
                </div>
                <div v-else class="text-body2 text-grey-5 q-mt-sm">暂无描述</div>
              </q-card-section>

              <q-separator />

              <q-card-actions>
                <q-btn
                  flat
                  dense
                  size="sm"
                  color="primary"
                  icon="edit"
                  label="编辑"
                  @click="openMainEdit(p.id)"
                  :disable="isMainLocked"
                />
              </q-card-actions>
            </q-card>
          </div>

          <!-- 分页 -->
          <q-separator v-if="mainPresets.length > 0" />
          <q-card-section v-if="mainPresets.length > 0" class="row items-center justify-between">
            <div class="text-caption text-grey-6">共 {{ mainTotal }} 条</div>
            <q-pagination
              v-model="mainPage"
              :max="Math.max(1, Math.ceil(mainTotal / mainLimit))"
              max-pages="7"
              size="sm"
              direction-links
              boundary-links
            />
          </q-card-section>
        </q-card>
      </q-tab-panel>
    </q-tab-panels>

    <!-- 角色预设 新建/编辑对话框 -->
    <q-dialog v-model="openDialog" persistent>
      <q-card style="min-width: 500px; max-width: 90vw">
        <q-card-section class="row items-center">
          <div class="text-h6">{{ dialogTitle }}</div>
          <q-space />
          <q-btn icon="close" flat round dense v-close-popup />
        </q-card-section>

        <q-separator />

        <q-card-section class="q-gutter-md" style="max-height: 70vh; overflow-y: auto">
          <q-input
            v-model="form.name"
            label="名称"
            filled
            dense
            :rules="[(v) => !!v || '名称不能为空']"
          />
          <q-input
            v-model="form.description"
            type="textarea"
            label="描述"
            filled
            dense
            autogrow
            :input-style="{ minHeight: '60px' }"
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
              <q-img :src="currentPreviewUrl" fit="contain" class="preview-image" />
              <div class="preview-actions">
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

          <q-separator />

          <div class="text-subtitle2 text-grey-7">正向提示词修改规则</div>
          <div class="text-caption text-grey-5 q-mb-sm">
            设置 Before/After 会在原提示词前后添加内容；设置 Replace 会完全替换原提示词
          </div>

          <PromptEditor v-model="form.before" label="Before (添加到提示词之前)" min-height="50px" />
          <PromptEditor v-model="form.after" label="After (添加到提示词之后)" min-height="50px" />
          <PromptEditor v-model="form.replace" label="Replace (完全替换提示词)" min-height="50px" />

          <q-separator class="q-mt-md" />

          <div class="text-subtitle2 text-grey-7 q-mt-sm">负面提示词 (UC) 修改规则</div>
          <div class="text-caption text-grey-5 q-mb-sm">同样的规则应用于负面提示词，可选填</div>

          <PromptEditor
            v-model="form.uc_before"
            label="UC Before (添加到负面提示词之前)"
            min-height="50px"
          />
          <PromptEditor
            v-model="form.uc_after"
            label="UC After (添加到负面提示词之后)"
            min-height="50px"
          />
          <PromptEditor
            v-model="form.uc_replace"
            label="UC Replace (完全替换负面提示词)"
            min-height="50px"
          />
        </q-card-section>

        <q-separator />

        <q-card-actions align="right" class="q-pa-md">
          <q-btn flat label="取消" v-close-popup />
          <q-btn color="primary" label="保存" :loading="saving" @click="save" />
        </q-card-actions>
      </q-card>
    </q-dialog>

    <!-- 主预设 新建/编辑对话框 -->
    <q-dialog v-model="mainOpenDialog" persistent>
      <q-card style="min-width: 500px; max-width: 90vw">
        <q-card-section class="row items-center">
          <div class="text-h6">{{ mainDialogTitle }}</div>
          <q-space />
          <q-btn icon="close" flat round dense v-close-popup />
        </q-card-section>

        <q-separator />

        <q-card-section class="q-gutter-md" style="max-height: 70vh; overflow-y: auto">
          <q-input
            v-model="mainForm.name"
            label="名称"
            filled
            dense
            :rules="[(v) => !!v || '名称不能为空']"
          />
          <q-input
            v-model="mainForm.description"
            type="textarea"
            label="描述"
            filled
            dense
            autogrow
            :input-style="{ minHeight: '60px' }"
          />

          <q-separator />

          <div class="text-subtitle2 text-grey-7">正向提示词修改规则</div>
          <div class="text-caption text-grey-5 q-mb-sm">
            设置 Before/After 会在原提示词前后添加内容；设置 Replace 会完全替换原提示词
          </div>

          <PromptEditor
            v-model="mainForm.before"
            label="Before (添加到提示词之前)"
            min-height="50px"
          />
          <PromptEditor
            v-model="mainForm.after"
            label="After (添加到提示词之后)"
            min-height="50px"
          />
          <PromptEditor
            v-model="mainForm.replace"
            label="Replace (完全替换提示词)"
            min-height="50px"
          />

          <q-separator class="q-mt-md" />

          <div class="text-subtitle2 text-grey-7 q-mt-sm">负面提示词 (UC) 修改规则</div>
          <div class="text-caption text-grey-5 q-mb-sm">同样的规则应用于负面提示词，可选填</div>

          <PromptEditor
            v-model="mainForm.uc_before"
            label="UC Before (添加到负面提示词之前)"
            min-height="50px"
          />
          <PromptEditor
            v-model="mainForm.uc_after"
            label="UC After (添加到负面提示词之后)"
            min-height="50px"
          />
          <PromptEditor
            v-model="mainForm.uc_replace"
            label="UC Replace (完全替换负面提示词)"
            min-height="50px"
          />
        </q-card-section>

        <q-separator />

        <q-card-actions align="right" class="q-pa-md">
          <q-btn flat label="取消" v-close-popup />
          <q-btn color="primary" label="保存" :loading="mainSaving" @click="saveMainPreset" />
        </q-card-actions>
      </q-card>
    </q-dialog>
  </q-page>
</template>

<style scoped lang="scss">
.presets-page {
  max-width: 1200px;
  margin: 0 auto;
}

.page-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 16px;
}

.tab-panels {
  background: transparent;
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

.preset-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
  gap: 16px;
}

.preset-card {
  transition: box-shadow 0.2s;

  &:hover {
    box-shadow: 0 2px 12px rgba(0, 0, 0, 0.1);
  }
}

.preset-preview {
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
</style>
