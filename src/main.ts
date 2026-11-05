import { createPinia } from "pinia";
import { createApp } from "vue";
import App from "./App.vue";
import "vue-sonner/style.css";
import "./styles/main.css";

createApp(App).use(createPinia()).mount("#app");
