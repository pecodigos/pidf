# PiDF

PiDF is a minimal, fast desktop PDF reader built with Tauri (Rust), Svelte, and PDFium.

## MVP Features

- Open local PDF files with native desktop file picker.
- Smooth scroll-first reading flow.
- Ctrl + wheel zoom.
- Jump to page.
- Optional lightweight page sidebar.
- Keyboard navigation (arrows, PageUp/PageDown, Home/End).
- Dark mode toggle.

## Performance Design

- Virtualized page list: only nearby pages are mounted in the DOM.
- Lazy rendering: pages render only when they enter the active virtual range.
- Render cache: recent rendered page images are cached with LRU eviction.
- Debounced zoom: avoids re-render storms while zooming.
- Scroll updates coalesced with requestAnimationFrame.
- Page rasterization is handled by backend PDFium commands to avoid WebView rendering stalls.

## Project Structure

```text
src/
	lib/
		core/
			pdf.ts            # PDF session + backend render command orchestration
			renderCache.ts    # LRU image cache for rendered pages
		state/
			viewer.ts         # Zoom/page/theme state + helpers
		ui/
			Toolbar.svelte
			Sidebar.svelte
			PageCanvas.svelte
			VirtualPdfViewer.svelte
	routes/
		+page.svelte        # App shell and integration wiring

src-tauri/
	src/lib.rs            # Rust commands: initial path + PDFium open/render stages
	capabilities/default.json
	tauri.conf.json

static/
	pdfjs/
		cmaps/              # Legacy assets from previous PDF.js implementation
		standard_fonts/     # Legacy assets from previous PDF.js implementation
```

## Development

```bash
npm install
npm run tauri dev
```

## Validation

```bash
npm run check
npm run build
cd src-tauri && cargo check
```
