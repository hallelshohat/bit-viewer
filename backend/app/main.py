import logging
from pathlib import Path

from fastapi import FastAPI, File, HTTPException, Query, Response, UploadFile
from fastapi.middleware.cors import CORSMiddleware

from .filter_jobs import FilterJobManager
from .models import (
    CreateFilterJobResponse,
    FileMetadata,
    FilterConfig,
    FilterJobStatusResponse,
    UploadResponse,
    ViewMetadata,
    ViewportResponse,
)
from .storage import FileStore
from .viewport import build_viewport_response, compute_viewport_slice

BASE_DIR = Path(__file__).resolve().parent.parent
UPLOAD_DIR = BASE_DIR / "uploads"

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s %(levelname)s %(name)s %(message)s",
)

app = FastAPI(title="Bit Viewer API")
app.add_middleware(
    CORSMiddleware,
    allow_origins=["http://localhost:5173", "http://127.0.0.1:5173"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

store = FileStore(UPLOAD_DIR)
filter_jobs = FilterJobManager(store)


@app.on_event("startup")
def startup_cleanup() -> None:
    store.cleanup_expired()


@app.post("/api/files/upload", response_model=UploadResponse)
async def upload_file(file: UploadFile = File(...)) -> UploadResponse:
    metadata = await store.save_upload(file)
    return UploadResponse(
        fileId=metadata.file_id,
        filename=metadata.filename,
        sizeBytes=metadata.size_bytes,
        logicalBitLength=metadata.logical_bit_length,
    )


@app.get("/api/files/{file_id}/metadata", response_model=FileMetadata)
def get_metadata(file_id: str) -> FileMetadata:
    try:
        return store.get_metadata(file_id)
    except FileNotFoundError as exc:
        raise HTTPException(status_code=404, detail=f"Unknown fileId: {file_id}") from exc


@app.get("/api/files/{file_id}/viewport", response_model=ViewportResponse)
def get_viewport(
    file_id: str,
    bit_offset: int = Query(..., alias="bitOffset", ge=0),
    visible_rows: int = Query(..., alias="visibleRows", gt=0, le=1000),
    row_width_bits: int = Query(..., alias="rowWidthBits", gt=0, le=16384),
) -> ViewportResponse:
    try:
        metadata = store.get_metadata(file_id)
    except FileNotFoundError as exc:
        raise HTTPException(status_code=404, detail=f"Unknown fileId: {file_id}") from exc

    slice_info = compute_viewport_slice(
        size_bytes=metadata.size_bytes,
        bit_offset=bit_offset,
        visible_rows=visible_rows,
        row_width_bits=row_width_bits,
    )

    with store.open_file(file_id) as handle:
        handle.seek(slice_info.start_byte)
        data = handle.read(slice_info.byte_length)

    return build_viewport_response(
        metadata=metadata,
        bit_offset=bit_offset,
        visible_rows=visible_rows,
        row_width_bits=row_width_bits,
        data=data,
        slice_info=slice_info,
    )


@app.get("/api/files/{file_id}/chunk")
def get_chunk(
    file_id: str,
    byte_offset: int = Query(..., alias="byteOffset", ge=0),
    byte_length: int = Query(..., alias="byteLength", gt=0, le=1024 * 1024),
) -> Response:
    try:
        metadata = store.get_metadata(file_id)
    except FileNotFoundError as exc:
        raise HTTPException(status_code=404, detail=f"Unknown fileId: {file_id}") from exc

    start = min(byte_offset, metadata.size_bytes)
    end = min(metadata.size_bytes, byte_offset + byte_length)

    with store.open_file(file_id) as handle:
        handle.seek(start)
        data = handle.read(max(0, end - start))

    return Response(
        content=data,
        media_type="application/octet-stream",
        headers={
            "X-Byte-Offset": str(start),
            "X-Byte-Length": str(len(data)),
            "X-File-Size-Bytes": str(metadata.size_bytes),
        },
    )


@app.post("/api/files/{file_id}/filters", response_model=CreateFilterJobResponse)
def create_filter_job(file_id: str, config: FilterConfig) -> CreateFilterJobResponse:
    try:
        return filter_jobs.create_job(file_id, config)
    except FileNotFoundError as exc:
        raise HTTPException(status_code=404, detail=f"Unknown fileId: {file_id}") from exc


@app.get("/api/filter-jobs/{job_id}", response_model=FilterJobStatusResponse)
def get_filter_job(job_id: str) -> FilterJobStatusResponse:
    try:
        return filter_jobs.get_job(job_id)
    except FileNotFoundError as exc:
        raise HTTPException(status_code=404, detail=f"Unknown jobId: {job_id}") from exc


@app.get("/api/views/{view_id}/metadata", response_model=ViewMetadata)
def get_view_metadata(view_id: str) -> ViewMetadata:
    try:
        return store.get_view_metadata(view_id)
    except FileNotFoundError as exc:
        raise HTTPException(status_code=404, detail=f"Unknown viewId: {view_id}") from exc


@app.get("/api/views/{view_id}/chunk")
def get_view_chunk(
    view_id: str,
    byte_offset: int = Query(..., alias="byteOffset", ge=0),
    byte_length: int = Query(..., alias="byteLength", gt=0, le=1024 * 1024),
) -> Response:
    try:
        metadata = store.get_view_metadata(view_id)
    except FileNotFoundError as exc:
        raise HTTPException(status_code=404, detail=f"Unknown viewId: {view_id}") from exc

    start = min(byte_offset, metadata.size_bytes)
    end = min(metadata.size_bytes, byte_offset + byte_length)

    with store.open_view(view_id) as handle:
        handle.seek(start)
        data = handle.read(max(0, end - start))

    return Response(
        content=data,
        media_type="application/octet-stream",
        headers={
            "X-Byte-Offset": str(start),
            "X-Byte-Length": str(len(data)),
            "X-File-Size-Bytes": str(metadata.size_bytes),
        },
    )
