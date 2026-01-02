import { defineStore } from 'pinia';
import { Notify } from 'quasar';
import {
  fetchTaskStatus,
  submitTask,
  type GenerationParams,
  type TaskStatus,
} from 'src/services/api';

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
      if (status.status === 'pending' || status.status === 'running') {
        item.status = status.status;
      } else if (status.status === 'failed') {
        item.status = 'failed';
        item.error = status.error;
        item.completedAt = Date.now();
        // 任务失败通知
        if (prevStatus !== 'failed') {
          Notify.create({
            type: 'negative',
            message: `任务失败: ${item.title}`,
            caption: status.error,
            timeout: 5000,
            position: 'top-right',
          });
        }
      } else if (status.status === 'completed') {
        item.status = 'completed';
        item.completedAt = Date.now();
        // 任务完成通知
        if (prevStatus !== 'completed') {
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
        }
      }
    },
  },
});
