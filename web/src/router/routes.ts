import type { RouteRecordRaw } from 'vue-router';

const routes: RouteRecordRaw[] = [
  {
    path: '/',
    component: () => import('layouts/MainLayout.vue'),
    children: [
      { path: '', redirect: '/home' },
      { path: 'home', component: () => import('pages/HomePage.vue') },
      { path: 'generate', component: () => import('pages/GeneratePage.vue') },
      { path: 'snippets', component: () => import('pages/SnippetsPage.vue') },
      { path: 'presets', component: () => import('pages/PresetsPage.vue') },
      { path: 'gallery', component: () => import('pages/GalleryPage.vue') },
      { path: 'lexicon', component: () => import('pages/LexiconPage.vue') },
      { path: 'metadata', component: () => import('pages/MetadataPage.vue') },
    ],
  },

  // Always leave this as last one,
  // but you can also remove it
  {
    path: '/:catchAll(.*)*',
    component: () => import('pages/ErrorNotFound.vue'),
  },
];

export default routes;
