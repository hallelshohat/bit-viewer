# Bit Viewer

Bit Viewer is a full-stack MVP for exploring binary files through synchronized bit, hex, and ASCII views. It uses viewport-based fetching so the browser only loads the rows currently on screen, while the backend performs random-access reads against the uploaded file on disk.

## Directory structure

```text
bit-viewier/
├── backend/
│   ├── app/
│   │   ├── main.py
│   │   ├── models.py
│   │   ├── storage.py
│   │   └── viewport.py
│   ├── tests/
│   │   └── test_viewport.py
│   └── requirements.txt
├── frontend/
│   ├── src/
│   │   ├── components/
│   │   │   ├── BitCanvas.tsx
│   │   │   ├── FileUpload.tsx
│   │   │   ├── SettingsPanel.tsx
│   │   │   └── Viewer.tsx
│   │   ├── hooks/
│   │   │   └── useViewportData.ts
│   │   ├── lib/
│   │   │   ├── format.ts
│   │   │   └── lru.ts
│   │   ├── api.ts
│   │   ├── App.tsx
│   │   ├── main.tsx
│   │   ├── styles.css
│   │   └── types.ts
│   ├── index.html
│   ├── package.json
│   ├── tsconfig.app.json
│   ├── tsconfig.json
│   ├── tsconfig.node.json
│   └── vite.config.ts
└── README.md
```

## Features

- Multipart upload to FastAPI with temporary disk storage.
- Random-access chunk endpoint that serves fixed 500 KB byte ranges from disk.
- Client-side row decoding with correct handling for arbitrary bit offsets and non-byte-aligned row widths.
- Canvas-based bit grid where `1` is blue and `0` is white.
- Synchronized hex and ASCII panes with independent vertical scroll sync and horizontal scroll only where needed.
- Fixed-size chunk caching with simple LRU retention and previous/next chunk prefetch.
- Basic keyboard navigation with arrow keys, page up/down, home, and end.
- Expired upload cleanup on startup and before new uploads.

## Backend

### Run locally

```bash
cd backend
python3 -m venv .venv
source .venv/bin/activate
pip install -r requirements.txt
uvicorn app.main:app --reload
```

The API is available at `http://localhost:8000`.

If your system Python does not include `venv`, install it first or use:

```bash
cd backend
python3 -m pip install --user -r requirements.txt
python3 -m uvicorn app.main:app --reload
```

### Tests

```bash
cd backend
source .venv/bin/activate
pytest
```

## Frontend

### Run locally

```bash
cd frontend
npm install
npm run dev
```

The Vite dev server runs at `http://localhost:5173` and proxies API requests to the backend.

### Production build

```bash
cd frontend
npm run build
```

## API summary

### `POST /api/files/upload`

Accepts a multipart upload and returns:

```json
{
  "fileId": "abc123",
  "filename": "sample.bin",
  "sizeBytes": 104857600
}
```

### `GET /api/files/{fileId}/metadata`

Returns filename, size, and creation time for a previously uploaded file.

### `GET /api/files/{fileId}/viewport`

Query parameters:

- `bitOffset`
- `visibleRows`
- `rowWidthBits`

Returns row-level data for the current viewport, including exact bit strings, corresponding byte ranges, hex strings, and ASCII strings.

### `GET /api/files/{fileId}/chunk`

Query parameters:

- `byteOffset`
- `byteLength`

Returns raw `application/octet-stream` bytes for the requested slice. The frontend uses fixed `500 * 1024` byte chunks and derives visible rows locally from cached chunk data.

## Performance decisions

- The backend stores uploads on disk and only reads the requested 500 KB chunk slices.
- The frontend virtualizes by row count and renders only the visible slice plus overscan.
- Row decoding happens on the client from cached raw chunks, so scrolling within a chunk does not trigger more requests.
- The bit grid is drawn on a `<canvas>` with one pixel per bit, then scaled with CSS. That keeps the drawing buffer small even when the user increases the square size.
- Raw chunks are cached in an in-memory LRU cache on the client, and adjacent chunks are prefetched to reduce hitching during scroll.

## Notes

- The hex and ASCII panes show the bytes touched by each rendered row. With non-byte-aligned row widths, neighboring rows can intentionally overlap at the byte level.
- Uploads are kept temporarily in `backend/uploads` and cleaned up after a six-hour TTL.
