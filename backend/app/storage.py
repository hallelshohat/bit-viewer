import json
import os
import time
import uuid
from pathlib import Path
from typing import BinaryIO, Tuple

from fastapi import UploadFile

from .models import FileMetadata, ViewMetadata

UPLOAD_TTL_SECONDS = 60 * 60 * 6
CHUNK_SIZE = 1024 * 1024


class FileStore:
    def __init__(self, root: Path) -> None:
        self.root = root
        self.files_root = self.root / "files"
        self.views_root = self.root / "views"
        self.files_root.mkdir(parents=True, exist_ok=True)
        self.views_root.mkdir(parents=True, exist_ok=True)

    def _file_path(self, file_id: str) -> Path:
        return self.files_root / f"{file_id}.bin"

    def _meta_path(self, file_id: str) -> Path:
        return self.files_root / f"{file_id}.json"

    def _view_path(self, view_id: str) -> Path:
        return self.views_root / f"{view_id}.bin"

    def _view_meta_path(self, view_id: str) -> Path:
        return self.views_root / f"{view_id}.json"

    def cleanup_expired(self) -> None:
        cutoff = time.time() - UPLOAD_TTL_SECONDS
        for metadata_dir, metadata_suffix, payload_suffix in (
            (self.files_root, ".json", ".bin"),
            (self.views_root, ".json", ".bin"),
        ):
            for meta_path in metadata_dir.glob(f"*{metadata_suffix}"):
                try:
                    if meta_path.stat().st_mtime >= cutoff:
                        continue
                    resource_id = meta_path.stem
                    meta_path.unlink(missing_ok=True)
                    (metadata_dir / f"{resource_id}{payload_suffix}").unlink(missing_ok=True)
                except OSError:
                    continue

    async def save_upload(self, upload: UploadFile) -> FileMetadata:
        self.cleanup_expired()
        file_id = uuid.uuid4().hex
        file_path = self._file_path(file_id)
        size_bytes = 0

        with file_path.open("wb") as target:
            while True:
                chunk = await upload.read(CHUNK_SIZE)
                if not chunk:
                    break
                size_bytes += len(chunk)
                target.write(chunk)

        metadata = FileMetadata(
            fileId=file_id,
            filename=upload.filename or "upload.bin",
            sizeBytes=size_bytes,
            logicalBitLength=size_bytes * 8,
            createdAt=self._mtime_to_iso(file_path),
        )
        self._write_metadata(metadata)
        await upload.close()
        return metadata

    def get_metadata(self, file_id: str) -> FileMetadata:
        meta_path = self._meta_path(file_id)
        file_path = self._file_path(file_id)
        if not meta_path.exists() or not file_path.exists():
            raise FileNotFoundError(file_id)

        with meta_path.open("r", encoding="utf-8") as handle:
            raw = json.load(handle)
        return FileMetadata.parse_obj(raw)

    def open_file(self, file_id: str):
        file_path = self._file_path(file_id)
        if not file_path.exists():
            raise FileNotFoundError(file_id)
        return file_path.open("rb")

    def get_view_metadata(self, view_id: str) -> ViewMetadata:
        meta_path = self._view_meta_path(view_id)
        view_path = self._view_path(view_id)
        if not meta_path.exists() or not view_path.exists():
            raise FileNotFoundError(view_id)

        with meta_path.open("r", encoding="utf-8") as handle:
            raw = json.load(handle)
        return ViewMetadata.parse_obj(raw)

    def open_view(self, view_id: str) -> BinaryIO:
        view_path = self._view_path(view_id)
        if not view_path.exists():
            raise FileNotFoundError(view_id)
        return view_path.open("rb")

    def create_view(self) -> Tuple[str, Path]:
        view_id = uuid.uuid4().hex
        return view_id, self._view_path(view_id)

    def save_view_metadata(self, metadata: ViewMetadata) -> None:
        with self._view_meta_path(metadata.view_id).open("w", encoding="utf-8") as handle:
            handle.write(metadata.json(by_alias=True))

    def delete_view(self, view_id: str) -> None:
        self._view_meta_path(view_id).unlink(missing_ok=True)
        self._view_path(view_id).unlink(missing_ok=True)

    def _write_metadata(self, metadata: FileMetadata) -> None:
        with self._meta_path(metadata.file_id).open("w", encoding="utf-8") as handle:
            handle.write(metadata.json(by_alias=True))

    @staticmethod
    def _mtime_to_iso(path: Path) -> str:
        return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(os.path.getmtime(path)))
