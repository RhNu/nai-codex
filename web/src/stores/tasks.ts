import { defineStore } from 'pinia';
import { Notify } from 'quasar';
import { useWebNotification } from '@vueuse/core';
import {
  fetchTaskStatus,
  submitTask,
  type GenerationParams,
  type TaskStatus,
} from 'src/services/api';

// 浏览器通知实例
const { isSupported: notificationSupported, show: showNotification } = useWebNotification({
  title: '',
  dir: 'auto',
  lang: 'zh-CN',
  renotify: true,
  tag: 'codex-task',
});

export type TaskItem = {
  id: string;
  title: string;
  count: number;
  status: 'pending' | 'running' | 'completed' | 'failed';
  error?: string;
  startedAt?: number;
  completedAt?: number;
};

export const useTaskStore = defineStore('tasks', {
  state: () => ({
    items: [] as TaskItem[],
    polling: false,
    timer: null as ReturnType<typeof setInterval> | null,
  }),
  actions: {
    async submit(payload: {
      raw_prompt: string;
      negative_prompt: string;
      count: number;
      title?: string;
      params?: GenerationParams;
      preset_id?: string | null;
    }) {
      const id = await submitTask({
        raw_prompt: payload.raw_prompt,
        negative_prompt: payload.negative_prompt,
        count: payload.count,
        ...(payload.params && { params: payload.params }),
        ...(payload.preset_id && { preset_id: payload.preset_id }),
      });
      this.items.unshift({
        id,
        title: payload.title || payload.raw_prompt.slice(0, 32),
        count: payload.count,
        status: 'pending',
        startedAt: Date.now(),
      });
      this.startPolling();
      return id;
    },

    startPolling() {
      if (this.polling) return;
      this.polling = true;
      this.timer = setInterval(() => {
        void this.refreshStatuses();
      }, 4000);
    },

    stopPolling() {
      if (this.timer) {
        clearInterval(this.timer);
        this.timer = null;
      }
      this.polling = false;
    },

    async refreshStatuses() {
      const ids = this.items.map((t) => t.id);
      for (const id of ids) {
        try {
          const status = await fetchTaskStatus(id);
          this.applyStatus(id, status);
        } catch (err) {
          console.error('poll status error', err);
        }
      }
    },

    applyStatus(id: string, status: TaskStatus) {
      const item = this.items.find((t) => t.id === id);
      if (!item) return;
      const prevStatus = item.status;

      // 状态转换为 running 时记录实际开始时间
      if (status.status === 'running' && prevStatus === 'pending') {
        item.status = 'running';
        // 更新为实际开始运行的时间
        item.startedAt = Date.now();
      } else if (status.status === 'pending') {
        item.status = 'pending';
      } else if (status.status === 'failed') {
        // 仅在状态首次变为 failed 时记录完成时间
        if (prevStatus !== 'failed') {
          item.status = 'failed';
          item.error = status.error;
          item.completedAt = Date.now();
          // 应用内通知
          Notify.create({
            type: 'negative',
            message: `任务失败: ${item.title}`,
            caption: status.error,
            timeout: 5000,
            position: 'top-right',
          });
          // 浏览器通知（页面不在前台时）
          if (notificationSupported.value && document.hidden) {
            void showNotification({
              title: '任务失败',
              body: `${item.title}\n${status.error}`,
            });
          }
        }
      } else if (status.status === 'completed') {
        // 仅在状态首次变为 completed 时记录完成时间
        if (prevStatus !== 'completed') {
          item.status = 'completed';
          item.completedAt = Date.now();
          // 应用内通知
          Notify.create({
            type: 'positive',
            message: `任务完成: ${item.title}`,
            caption: `已生成 ${item.count} 张图片`,
            timeout: 3000,
            position: 'top-right',
            actions: [
              {
                label: '查看画廊',
                color: 'white',
                handler: () => {
                  window.location.href = '/#/gallery';
                },
              },
            ],
          });
          // 浏览器通知（页面不在前台时）
          if (notificationSupported.value && document.hidden) {
            void showNotification({
              title: '任务完成',
              body: `${item.title}\n已生成 ${item.count} 张图片`,
            });
          }
        }
      }
    },
  },
});
