import { createRouter, createWebHistory } from 'vue-router'
import MapView from '@/views/MapView.vue'

const routerBase = new URL(import.meta.env.BASE_URL, window.location.href).pathname

const router = createRouter({
  history: createWebHistory(routerBase),
  routes: [
    {
      path: '/',
      name: 'map',
      component: MapView,
    },
  ],
})

export default router
