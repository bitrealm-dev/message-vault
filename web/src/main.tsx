import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { initFfmpegToolsFromStorage } from "./lib/ffmpeg-tools";
import "./theme.css";

initFfmpegToolsFromStorage();

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
