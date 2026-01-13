import { createRoot } from "react-dom/client";
import App from "./App";

// StrictMode disabled temporarily for testing - it double-mounts effects
// which conflicts with WASM initialization
createRoot(document.getElementById("root")!).render(<App />);
