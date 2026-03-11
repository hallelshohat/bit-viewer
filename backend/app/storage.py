import json
import os
import time
import uuid
from pathlib import Path

from fastapi import UploadFile

from .models import FileMetadata

UPLOAD_TTL_SECONDS = 60 * 60 * 6
CHUNK_SIZE = 1024 * 1024


class FileStore:
    def __init__(self, root: Path) -> None:
        self.root = root
        self.root.mkdir(parents=True, exist_ok=True)

    def _file_path(self, file_id: str) -> Path:
        return self.root / f"{file_id}.bin"

    def _meta_path(self, file_id: str) -> Path:
        return self.root / f"{file_id}.json"

    def cleanup_expired(self) -> None:
        cutoff = time.time() - UPLOAD_TTL_SECONDS
        for meta_path in self.root.glob("*.json"):
            try:
                if meta_path.stat().st_mtime >= cutoff:
                    continue
                file_id = meta_path.stem
                self._meta_path(file_id).unlink(missing_ok=True)
                self._file_path(file_id).unlink(missing_ok=True)
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

    def _write_metadata(self, metadata: FileMetadata) -> None:
        with self._meta_path(metadata.file_id).open("w", encoding="utf-8") as handle:
            handle.write(metadata.json(by_alias=True))

    @staticmethod
    def _mtime_to_iso(path: Path) -> str:
        return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(os.path.getmtime(path)))
