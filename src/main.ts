import { createPinia } from "pinia";
import { createApp } from "vue";
import App from "./App.vue";
import { installLayout } from "./lib/layout";
import "vue-sonner/style.css";
import "./styles/main.css";
import "./styles/desktop.css";

installLayout();
createApp(App).use(createPinia()).mount("#app");
