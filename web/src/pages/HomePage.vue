<script setup lang="ts">
import { onMounted, ref } from 'vue';
import { fetchQuota, checkHealth } from 'src/services/api';

const anlas = ref(0);
const loading = ref(false);
const error = ref(false);
const serverStatus = ref<'online' | 'offline'>('offline');

function formatNumber(num: number): string {
  return num.toLocaleString('zh-CN');
}

async function loadQuota() {
  loading.value = true;
  error.value = false;
  try {
    const res = await fetchQuota();
    anlas.value = res.anlas;
  } catch (e) {
    console.error('Failed to fetch quota:', e);
    error.value = true;
  } finally {
    loading.value = false;
  }
}

async function checkServerHealth() {
  try {
    await checkHealth();
    serverStatus.value = 'online';
  } catch {
    serverStatus.value = 'offline';
  }
}

onMounted(() => {
  loadQuota().catch(console.error);
  checkServerHealth().catch(console.error);
});
</script>

<template>
  <q-page padding class="home-page">
    <!-- 头部 -->
    <div class="page-header q-mb-md">
      <div class="text-h5">概览</div>
      <q-btn flat round icon="refresh" :loading="loading" @click="loadQuota">
        <q-tooltip>刷新</q-tooltip>
      </q-btn>
    </div>

    <!-- 主卡片 -->
    <q-card>
      <!-- Anlas 余额区域 -->
      <q-card-section class="anlas-section">
        <div class="row items-center no-wrap">
          <div class="q-mr-md">
            <q-avatar size="56px" color="amber" text-color="white" icon="monetization_on" />
          </div>
          <div class="flex-grow">
            <div class="text-caption text-grey">Anlas 余额</div>
            <div class="text-h4 text-weight-bold">
              <q-skeleton v-if="loading" type="text" width="80px" />
              <template v-else-if="error">
                <span class="text-negative text-body1">获取失败</span>
              </template>
              <template v-else>
                {{ formatNumber(anlas) }}
              </template>
            </div>
          </div>
        </div>
      </q-card-section>

      <q-separator />

      <!-- 系统状态区域 -->
      <q-card-section>
        <div class="text-subtitle2 q-mb-sm">系统状态</div>
        <div class="row items-center">
          <q-icon
            name="circle"
            :color="serverStatus === 'online' ? 'positive' : 'negative'"
            size="12px"
            class="q-mr-sm"
          />
          <span>服务器: {{ serverStatus === 'online' ? '在线' : '离线' }}</span>
        </div>
      </q-card-section>

      <q-separator />

      <!-- 快速操作区域 -->
      <q-card-section>
        <div class="text-subtitle2 q-mb-md">快速操作</div>
        <div class="row q-gutter-md">
          <q-btn
            color="primary"
            icon="brush"
            label="开始生成"
            to="/generate"
            padding="sm lg"
            unelevated
          />
          <q-btn
            outline
            color="primary"
            icon="photo_library"
            label="画廊"
            to="/gallery"
            padding="sm lg"
          />
        </div>
      </q-card-section>
    </q-card>
  </q-page>
</template>

<style scoped lang="scss">
.home-page {
  max-width: 1200px;
  margin: 0 auto;
}

.page-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.anlas-section {
  background: linear-gradient(135deg, #fff9e6 0%, #fff3cc 100%);
}

:deep(.body--dark) .anlas-section {
  background: linear-gradient(135deg, #3d3a2e 0%, #2e2b1f 100%);
}
</style>
