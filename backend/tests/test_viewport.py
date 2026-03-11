from datetime import datetime, timezone

from app.models import FileMetadata
from app.storage import FileStore
from app.viewport import build_viewport_response, compute_viewport_slice, extract_bits


def make_metadata(size_bytes: int) -> FileMetadata:
    return FileMetadata(
        fileId="file-1",
        filename="sample.bin",
        sizeBytes=size_bytes,
        createdAt=datetime.now(timezone.utc),
    )


def test_extract_bits_crosses_byte_boundaries() -> None:
    data = bytes([0b10101100, 0b01011110])
    assert extract_bits(data, base_byte_offset=0, bit_offset=3, bit_length=7) == "0110001"


def test_compute_viewport_slice_for_non_aligned_range() -> None:
    slice_info = compute_viewport_slice(size_bytes=8, bit_offset=5, visible_rows=3, row_width_bits=10)
    assert slice_info.start_byte == 0
    assert slice_info.end_byte == 5
    assert slice_info.start_bit == 5
    assert slice_info.end_bit == 35


def test_build_viewport_response_handles_non_byte_aligned_rows() -> None:
    raw = b"ABCDEF"
    metadata = make_metadata(size_bytes=len(raw))
    slice_info = compute_viewport_slice(size_bytes=len(raw), bit_offset=4, visible_rows=2, row_width_bits=9)
    response = build_viewport_response(
        metadata=metadata,
        bit_offset=4,
        visible_rows=2,
        row_width_bits=9,
        data=raw[slice_info.start_byte : slice_info.end_byte],
        slice_info=slice_info,
    )

    assert response.actual_rows == 2
    assert response.rows[0].bits == "000101000"
    assert response.rows[0].hex == "41 42"
    assert response.rows[0].ascii == "AB"
    assert response.rows[1].bits == "010010000"
    assert response.rows[1].hex == "42 43"
    assert response.rows[1].ascii == "BC"


def test_metadata_round_trip_is_json_serializable(tmp_path) -> None:
    store = FileStore(tmp_path)
    metadata = make_metadata(size_bytes=6)
    (tmp_path / f"{metadata.file_id}.bin").write_bytes(b"ABCDEF")

    store._write_metadata(metadata)
    restored = store.get_metadata(metadata.file_id)

    assert restored.file_id == metadata.file_id
    assert restored.filename == metadata.filename
    assert restored.size_bytes == metadata.size_bytes
