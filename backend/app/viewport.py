from __future__ import annotations

from dataclasses import dataclass
from math import ceil
from typing import List, Tuple

from .models import BitRange, ByteRange, FileMetadata, ViewportResponse, ViewportRow


PRINTABLE_LOW = 32
PRINTABLE_HIGH = 126


def extract_bits(data: bytes, base_byte_offset: int, bit_offset: int, bit_length: int) -> str:
    if bit_length <= 0:
        return ""

    parts: List[str] = []
    for absolute_bit in range(bit_offset, bit_offset + bit_length):
        absolute_byte = absolute_bit // 8
        relative_byte = absolute_byte - base_byte_offset
        bit_in_byte = 7 - (absolute_bit % 8)
        value = (data[relative_byte] >> bit_in_byte) & 1
        parts.append("1" if value else "0")
    return "".join(parts)


def bytes_to_hex_ascii(data: bytes) -> Tuple[str, str]:
    hex_value = " ".join(f"{value:02X}" for value in data)
    ascii_value = "".join(
        chr(value) if PRINTABLE_LOW <= value <= PRINTABLE_HIGH else "." for value in data
    )
    return hex_value, ascii_value


@dataclass(frozen=True)
class ViewportSlice:
    start_byte: int
    end_byte: int
    start_bit: int
    end_bit: int
    byte_length: int
    bit_length: int


def compute_viewport_slice(size_bytes: int, bit_offset: int, visible_rows: int, row_width_bits: int) -> ViewportSlice:
    if bit_offset < 0:
        raise ValueError("bitOffset must be >= 0")
    if visible_rows <= 0:
        raise ValueError("visibleRows must be > 0")
    if row_width_bits <= 0:
        raise ValueError("rowWidthBits must be > 0")

    total_bits = size_bytes * 8
    start_bit = min(bit_offset, total_bits)
    end_bit = min(total_bits, bit_offset + (visible_rows * row_width_bits))
    # Arbitrary bit offsets can start or end mid-byte, so the backend widens the
    # read window to the enclosing byte range and trims individual rows afterward.
    start_byte = start_bit // 8
    end_byte = ceil(end_bit / 8) if end_bit else start_byte
    return ViewportSlice(
        start_byte=start_byte,
        end_byte=end_byte,
        start_bit=start_bit,
        end_bit=end_bit,
        byte_length=max(0, end_byte - start_byte),
        bit_length=max(0, end_bit - start_bit),
    )


def build_viewport_response(
    metadata: FileMetadata,
    bit_offset: int,
    visible_rows: int,
    row_width_bits: int,
    data: bytes,
    slice_info: ViewportSlice,
) -> ViewportResponse:
    total_bits = metadata.size_bytes * 8
    rows: List[ViewportRow] = []

    for row_index in range(visible_rows):
        row_bit_offset = bit_offset + (row_index * row_width_bits)
        if row_bit_offset >= total_bits:
            break

        row_end_bit = min(total_bits, row_bit_offset + row_width_bits)
        row_bit_length = row_end_bit - row_bit_offset
        row_byte_start = row_bit_offset // 8
        row_byte_end = ceil(row_end_bit / 8)
        relative_start = row_byte_start - slice_info.start_byte
        relative_end = row_byte_end - slice_info.start_byte
        row_bytes = data[relative_start:relative_end]

        bits = extract_bits(data, slice_info.start_byte, row_bit_offset, row_bit_length)
        hex_value, ascii_value = bytes_to_hex_ascii(row_bytes)
        rows.append(
            ViewportRow(
                rowIndex=row_index,
                bitOffset=row_bit_offset,
                bitLength=row_bit_length,
                byteOffsetStart=row_byte_start,
                byteOffsetEnd=row_byte_end,
                bits=bits,
                hex=hex_value,
                ascii=ascii_value,
            )
        )

    return ViewportResponse(
        fileId=metadata.file_id,
        filename=metadata.filename,
        sizeBytes=metadata.size_bytes,
        requestedBitOffset=bit_offset,
        visibleRows=visible_rows,
        rowWidthBits=row_width_bits,
        actualRows=len(rows),
        bitRange=BitRange(start=slice_info.start_bit, end=slice_info.end_bit, length=slice_info.bit_length),
        byteRange=ByteRange(
            start=slice_info.start_byte,
            end=slice_info.end_byte,
            length=slice_info.byte_length,
        ),
        rows=rows,
    )
