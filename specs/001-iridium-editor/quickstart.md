# Quickstart: Iridium Editor

Get Iridium running in your application in under 5 minutes.

## Prerequisites

- Modern browser with WebGPU support (Chrome 113+, Firefox 141+, Safari 26+, Edge 113+)
- Node.js 18+ (for npm package) or Deno 2.6+ (for JSR package)

## Installation

### npm

```bash
npm install @iridium/editor
```

### Deno (JSR)

```typescript
import { createEditor } from "jsr:@iridium/editor";
```

## Basic Usage (Vanilla JavaScript)

```html
<!DOCTYPE html>
<html>
<head>
  <title>Iridium Editor</title>
  <style>
    #editor-container {
      width: 800px;
      height: 600px;
    }
    canvas {
      width: 100%;
      height: 100%;
    }
  </style>
</head>
<body>
  <div id="editor-container">
    <canvas id="editor"></canvas>
  </div>

  <script type="module">
    import { createEditor, isWebGPUSupported } from '@iridium/editor';

    async function init() {
      // Check WebGPU support first
      if (!await isWebGPUSupported()) {
        document.body.innerHTML = '<p>WebGPU is not supported in this browser.</p>';
        return;
      }

      // Create the editor
      const canvas = document.getElementById('editor');
      const editor = await createEditor({
        canvas,
        content: 'MATCH (n:Person) RETURN n.name',
        language: 'cypher',
      });

      // Listen for content changes
      editor.addEventListener((event) => {
        if (event.type === 'contentChanged') {
          console.log('Content changed:', event.content);
        }
      });
    }

    init();
  </script>
</body>
</html>
```

## React Integration

```tsx
import { useState } from 'react';
import { Iridium } from '@iridium/editor/react';

function App() {
  const [code, setCode] = useState('SELECT * FROM users WHERE active = true');

  return (
    <div style={{ width: '100%', height: '500px' }}>
      <Iridium
        value={code}
        onChange={setCode}
        language="sql"
        config={{
          showLineNumbers: true,
          showMinimap: true,
        }}
      />
    </div>
  );
}
```

## Configuration

### Theme

```typescript
import { createEditor, themes } from '@iridium/editor';

// Use built-in dark theme
const editor = await createEditor({
  canvas,
  theme: themes.dark,
});

// Or customize
const editor = await createEditor({
  canvas,
  theme: {
    name: 'my-theme',
    isDark: true,
    editor: {
      background: { r: 0.1, g: 0.1, b: 0.1, a: 1 },
      foreground: { r: 0.9, g: 0.9, b: 0.9, a: 1 },
      // ... other colors
    },
    syntax: {
      keyword: { r: 0.8, g: 0.4, b: 0.8, a: 1 },
      string: { r: 0.6, g: 0.8, b: 0.4, a: 1 },
      // ... other syntax colors
    },
    typography: {
      fontFamily: 'JetBrains Mono, monospace',
      fontSize: 14,
      lineHeight: 1.5,
      letterSpacing: 0,
    },
  },
});
```

### Editor Options

```typescript
const editor = await createEditor({
  canvas,
  config: {
    tabWidth: 2,              // Spaces per tab
    insertSpaces: true,       // Spaces instead of tabs
    autoIndent: true,         // Auto-indent on newline
    showLineNumbers: true,    // Line number gutter
    showMinimap: true,        // Document minimap
    cursorBlinkMs: 500,       // Cursor blink rate (0 = no blink)
    undoGroupTimeoutMs: 500,  // Undo grouping timeout
  },
});
```

## Common Operations

### Programmatic Editing

```typescript
// Set content
editor.setContent('new content');

// Insert at cursor
editor.insert('inserted text');

// Get content
const content = editor.getContent();

// Get specific line
const line = editor.getLine(0);
```

### Cursor and Selection

```typescript
// Set cursor position
editor.setCursor({ line: 5, column: 10 });

// Get cursor position
const cursor = editor.getCursor();

// Set selection
editor.setSelection({
  anchor: { line: 0, column: 0 },
  head: { line: 0, column: 10 },
});

// Multi-cursor
editor.addCursor({ line: 3, column: 0 });
```

### Undo/Redo

```typescript
// Basic undo/redo
editor.undo();
editor.redo();

// Check availability
if (editor.canUndo()) {
  editor.undo();
}

// Navigate undo tree branches
const treeInfo = editor.getUndoTreeInfo();
if (treeInfo.branchCount > 1) {
  editor.redoBranch(1); // Redo second branch
}

// Jump to specific point
editor.jumpToUndoNode(nodeId);
```

### Search and Replace

```typescript
// Start search
editor.find('pattern', {
  caseSensitive: false,
  wholeWord: false,
  regex: false,
});

// Navigate matches
editor.findNext();
editor.findPrevious();

// Replace
editor.replaceMatch('replacement');
editor.replaceAll('replacement');

// Clear search
editor.clearSearch();
```

### Code Folding

```typescript
// Fold/unfold at line
editor.foldAt(10);
editor.unfoldAt(10);
editor.toggleFoldAt(10);

// Fold/unfold all
editor.foldAll();
editor.unfoldAll();
```

## Supported Languages

```typescript
import { getSupportedLanguages } from '@iridium/editor';

console.log(getSupportedLanguages());
// ['cypher', 'sql', 'rust', 'python', 'typescript']

// Set language
editor.setLanguage('rust');
```

## Events

```typescript
editor.addEventListener((event) => {
  switch (event.type) {
    case 'contentChanged':
      console.log('New content:', event.content);
      break;
    case 'selectionChanged':
      console.log('Selections:', event.selections);
      break;
    case 'scrollChanged':
      console.log('First visible line:', event.firstLine);
      break;
    case 'searchUpdated':
      console.log('Matches:', event.matchCount);
      break;
    case 'error':
      console.error('Error:', event.code, event.message);
      break;
  }
});
```

## Error Handling

```typescript
try {
  const editor = await createEditor({ canvas });
} catch (error) {
  if (error.code === 'gpuInitFailed') {
    // WebGPU not available or failed to initialize
    showFallbackEditor();
  }
}

// Or listen for runtime errors
editor.addEventListener((event) => {
  if (event.type === 'error') {
    // Handle error
    logError(event.code, event.message);
  }
});
```

## Performance Tips

1. **Resize handling**: Call `editor.resize()` when the container size changes
2. **Large files**: Files up to 100k lines are supported with full performance
3. **Cleanup**: Call `editor.destroy()` when unmounting to release GPU resources
4. **Tab visibility**: Rendering automatically pauses when tab is hidden

## TypeScript Support

Full TypeScript definitions are included:

```typescript
import type {
  IridiumEditor,
  Position,
  Selection,
  Range,
  Theme,
  EditorConfig,
  EditorEvent,
} from '@iridium/editor';
```

## Next Steps

- See the [API Reference](./contracts/typescript-api.d.ts) for full API documentation
- Check the [examples/](../../../examples/) directory for more examples
- Read the [Data Model](./data-model.md) to understand the editor's internal structure
