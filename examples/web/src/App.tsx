/**
 * Example React application demonstrating the Iridium editor.
 */

import { useRef, useState } from "react";
import { Iridium, IridiumHandle } from "./Iridium";
import { useWebGPUSupport } from "./hooks";

const SAMPLE_CODE = `// Iridium Editor Demo
// A GPU-accelerated text editor built with Rust and WebGPU

fn main() {
    println!("Hello, Iridium!");

    // Features:
    // - 120fps rendering
    // - Multi-cursor editing
    // - Tree-sitter syntax highlighting
    // - Code folding
    // - Search & replace

    let editor = Editor::new();
    editor.set_content("Hello, World!");

    loop {
        editor.render();
        editor.handle_input();
    }
}

struct Editor {
    content: String,
    cursor: Position,
    selection: Option<Selection>,
}

impl Editor {
    fn new() -> Self {
        Self {
            content: String::new(),
            cursor: Position::zero(),
            selection: None,
        }
    }

    fn render(&self) {
        // GPU-accelerated rendering at 120fps
    }

    fn handle_input(&mut self) {
        // Low-latency input handling
    }
}
`;

function App() {
  const editorRef = useRef<IridiumHandle>(null);
  const [content, setContent] = useState(SAMPLE_CODE);
  const [cursorInfo, setCursorInfo] = useState({ line: 0, column: 0 });
  const [isDark, setIsDark] = useState(true);

  const webgpu = useWebGPUSupport();

  const handleFoldAll = () => {
    editorRef.current?.foldAll();
  };

  const handleUnfoldAll = () => {
    editorRef.current?.unfoldAll();
  };

  // Show loading state for WebGPU check
  if (!webgpu.checked) {
    return (
      <div style={styles.container}>
        <div style={styles.loading}>Checking WebGPU support...</div>
      </div>
    );
  }

  if (!webgpu.supported) {
    return (
      <div style={styles.container}>
        <div style={styles.error}>
          <h2>WebGPU Not Supported</h2>
          <p>
            Iridium requires WebGPU to run. Please use a browser that supports
            WebGPU, such as Chrome 113+ or Edge 113+.
          </p>
        </div>
      </div>
    );
  }

  return (
    <div style={styles.container}>
      {/* Header */}
      <header style={styles.header}>
        <h1 style={styles.title}>Iridium Editor</h1>
        <span style={styles.subtitle}>
          GPU: {webgpu.adapterName ?? "Unknown"}
        </span>
      </header>

      {/* Toolbar */}
      <div style={styles.toolbar}>
        {/* Theme Toggle */}
        <button
          style={styles.button}
          onClick={() => setIsDark(!isDark)}
        >
          {isDark ? "Light Theme" : "Dark Theme"}
        </button>

        {/* Folding Controls */}
        <button style={styles.button} onClick={handleFoldAll}>
          Fold All
        </button>
        <button style={styles.button} onClick={handleUnfoldAll}>
          Unfold All
        </button>
      </div>

      {/* Editor */}
      <div style={styles.editorContainer}>
        <Iridium
          ref={editorRef}
          content={content}
          language="rust"
          darkTheme={isDark}
          onChange={setContent}
          onSelectionChange={(sel) => {
            setCursorInfo({ line: sel.head.line, column: sel.head.column });
          }}
          style={styles.editor}
        />
      </div>

      {/* Status Bar */}
      <footer style={styles.statusBar}>
        <span>
          Ln {cursorInfo.line + 1}, Col {cursorInfo.column + 1}
        </span>
        <span>|</span>
        <span>{content.split("\n").length} lines</span>
        <span>|</span>
        <span>Rust</span>
        <span>|</span>
        <span>UTF-8</span>
      </footer>
    </div>
  );
}

const styles: Record<string, React.CSSProperties> = {
  container: {
    display: "flex",
    flexDirection: "column",
    height: "100vh",
    backgroundColor: "#1a1a1a",
    color: "#e0e0e0",
    fontFamily:
      '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif',
  },
  loading: {
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    height: "100%",
    fontSize: "1.2rem",
  },
  error: {
    display: "flex",
    flexDirection: "column",
    alignItems: "center",
    justifyContent: "center",
    height: "100%",
    color: "#ff6b6b",
    textAlign: "center",
    padding: "2rem",
  },
  header: {
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
    padding: "0.75rem 1rem",
    borderBottom: "1px solid #333",
    backgroundColor: "#252525",
  },
  title: {
    margin: 0,
    fontSize: "1.25rem",
    fontWeight: 600,
  },
  subtitle: {
    fontSize: "0.875rem",
    color: "#888",
  },
  toolbar: {
    display: "flex",
    alignItems: "center",
    gap: "0.5rem",
    padding: "0.5rem 1rem",
    borderBottom: "1px solid #333",
    backgroundColor: "#252525",
  },
  button: {
    padding: "0.375rem 0.75rem",
    border: "1px solid #444",
    borderRadius: "4px",
    backgroundColor: "#333",
    color: "#e0e0e0",
    fontSize: "0.875rem",
    cursor: "pointer",
  },
  editorContainer: {
    flex: 1,
    overflow: "hidden",
  },
  editor: {
    width: "100%",
    height: "100%",
  },
  statusBar: {
    display: "flex",
    alignItems: "center",
    gap: "1rem",
    padding: "0.375rem 1rem",
    borderTop: "1px solid #333",
    backgroundColor: "#252525",
    fontSize: "0.75rem",
    color: "#888",
  },
};

export default App;
