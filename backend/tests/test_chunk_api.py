from datetime import datetime, timezone

import app.main as main_module
from app.models import FileMetadata
from app.storage import FileStore


def test_chunk_endpoint_returns_requested_slice(tmp_path) -> None:
    original_store = main_module.store
    test_store = FileStore(tmp_path)
    main_module.store = test_store

    try:
        metadata = FileMetadata(
            fileId="chunk-file",
            filename="chunk.bin",
            sizeBytes=6,
            createdAt=datetime.now(timezone.utc),
        )
        (tmp_path / "chunk-file.bin").write_bytes(b"ABCDEF")
        test_store._write_metadata(metadata)

        response = main_module.get_chunk(
            file_id="chunk-file",
            byte_offset=1,
            byte_length=3,
        )

        assert response.status_code == 200
        assert response.body == b"BCD"
        assert response.headers["x-byte-offset"] == "1"
        assert response.headers["x-byte-length"] == "3"
    finally:
        main_module.store = original_store
