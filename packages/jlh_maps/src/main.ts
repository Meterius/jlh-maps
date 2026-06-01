import './assets/main.css'
// import './utils/virtual-webgl2'

import { createApp } from 'vue'
import { createPinia } from 'pinia'
import VueMaplibreGl from '@indoorequal/vue-maplibre-gl'
import ui from '@nuxt/ui/vue-plugin'

import App from './App.vue'
import router from './router'
import { registerMaplibreProtocols } from './external/maplibre-protocols.ts'

const app = createApp(App)

registerMaplibreProtocols()

app.use(VueMaplibreGl)
app.use(createPinia())
app.use(router)
app.use(ui)

app.mount('#app')
