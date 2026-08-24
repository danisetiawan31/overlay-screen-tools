import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles.css";

import { commands, events } from "./bindings";

// Expose type-safe bindings on window for automated testing
(window as unknown as { __APP_COMMANDS__: typeof commands; __APP_EVENTS__: typeof events }).__APP_COMMANDS__ = commands;
(window as unknown as { __APP_COMMANDS__: typeof commands; __APP_EVENTS__: typeof events }).__APP_EVENTS__ = events;

const rootElement = document.getElementById("root");
if (rootElement) {
  ReactDOM.createRoot(rootElement).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>
  );
}
