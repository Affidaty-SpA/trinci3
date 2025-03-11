import { createApp } from 'vue'
import { createPinia } from 'pinia'

import App from './App.vue'
import router from './router'
import Toast, { type PluginOptions } from "vue-toastification";

import './assets/css/style.css'
import './assets/css/satoshi.css'
import './assets/css/simple-datatables.css'

// Toastifications
import "vue-toastification/dist/index.css";
const options: PluginOptions = {
  // You can set your default options here
};

import { addAuthinCdnImports } from './utils/utils'

const app = createApp(App)

app.use(createPinia())
app.use(router)
.use(Toast, options)

app.mount('#app')

addAuthinCdnImports();
