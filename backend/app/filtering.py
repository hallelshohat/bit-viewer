from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Callable, Iterable, List, Optional, Sequence, Tuple

from .models import FilterBitRange, FilterConfig

READ_CHUNK_SIZE = 1024 * 1024
MAX_GROUP_BOUNDARIES_PER_BUFFER = 50000
REVERSE_BITS_TABLE = bytes(int(f"{value:08b}"[::-1], 2) for value in range(256))
BYTE_TO_BITS_TABLE = tuple(f"{value:08b}" for value in range(256))


@dataclass
class ProcessedViewResult:
    size_bytes: int
    logical_bit_length: int
    group_bit_lengths: Optional[List[int]]


class BitFileWriter:
    def __init__(self, path: Path) -> None:
        self.path = path
        self.handle = path.open("wb")
        self.pending_bits = ""
        self.total_bits = 0

    def write_bits(self, bits: str) -> None:
        if not bits:
            return

        combined = self.pending_bits + bits
        complete_length = len(combined) - (len(combined) % 8)
        if complete_length > 0:
            payload = bytes(
                int(combined[index : index + 8], 2)
                for index in range(0, complete_length, 8)
            )
            self.handle.write(payload)
        self.pending_bits = combined[complete_length:]
        self.total_bits += len(bits)

    def close(self) -> None:
        if self.pending_bits:
            self.handle.write(bytes([int(self.pending_bits.ljust(8, "0"), 2)]))
        self.handle.close()


def _normalize_remove_ranges(ranges: Sequence[FilterBitRange]) -> List[Tuple[int, int]]:
    if not ranges:
        return []

    merged: List[Tuple[int, int]] = []
    ordered = sorted((item.start_bit, item.start_bit + item.length) for item in ranges)
    for start, end in ordered:
        if not merged or start > merged[-1][1]:
            merged.append((start, end))
            continue
        merged[-1] = (merged[-1][0], max(merged[-1][1], end))
    return merged


def _apply_byte_transforms(data: bytes, config: FilterConfig) -> bytes:
    if not (config.reverse_bits_per_byte or config.invert_bits or config.xor_mask is not None):
        return data

    result = bytearray(len(data))
    xor_mask = config.xor_mask if config.xor_mask is not None else 0

    for index, value in enumerate(data):
        transformed = value
        if config.reverse_bits_per_byte:
            transformed = REVERSE_BITS_TABLE[transformed]
        if config.invert_bits:
            transformed ^= 0xFF
        if xor_mask:
            transformed ^= xor_mask
        result[index] = transformed

    return bytes(result)


def _iter_transformed_bit_chunks(
    source_path: Path,
    config: FilterConfig,
    progress_callback: Optional[Callable[[int], None]],
) -> Iterable[str]:
    source_size = max(1, source_path.stat().st_size)
    processed_bytes = 0

    with source_path.open("rb") as handle:
        while True:
            chunk = handle.read(READ_CHUNK_SIZE)
            if not chunk:
                break

            transformed = _apply_byte_transforms(chunk, config)
            processed_bytes += len(chunk)
            if progress_callback:
                progress_callback(min(95, int((processed_bytes / source_size) * 95)))

            yield "".join(BYTE_TO_BITS_TABLE[value] for value in transformed)


def _write_group_bits(
    writer: BitFileWriter,
    group_bits: str,
    remove_ranges: Sequence[Tuple[int, int]],
) -> int:
    if not group_bits:
        return 0

    if not remove_ranges:
        writer.write_bits(group_bits)
        return len(group_bits)

    kept_parts: List[str] = []
    cursor = 0

    for start, end in remove_ranges:
        if start >= len(group_bits):
            break
        if cursor < start:
            kept_parts.append(group_bits[cursor:start])
        cursor = min(end, len(group_bits))

    if cursor < len(group_bits):
        kept_parts.append(group_bits[cursor:])

    filtered_bits = "".join(kept_parts)
    writer.write_bits(filtered_bits)
    return len(filtered_bits)


def process_filtered_view(
    source_path: Path,
    output_path: Path,
    config: FilterConfig,
    progress_callback: Optional[Callable[[int], None]] = None,
) -> ProcessedViewResult:
    if not config.preamble_bits:
        with source_path.open("rb") as source_handle, output_path.open("wb") as output_handle:
            source_size = max(1, source_path.stat().st_size)
            processed_bytes = 0

            while True:
                chunk = source_handle.read(READ_CHUNK_SIZE)
                if not chunk:
                    break
                output_handle.write(_apply_byte_transforms(chunk, config))
                processed_bytes += len(chunk)
                if progress_callback:
                    progress_callback(min(100, int((processed_bytes / source_size) * 100)))

        return ProcessedViewResult(
            size_bytes=output_path.stat().st_size,
            logical_bit_length=output_path.stat().st_size * 8,
            group_bit_lengths=None,
        )

    preamble_bits = config.preamble_bits
    remove_ranges = _normalize_remove_ranges(config.remove_ranges)
    overlap_length = max(0, len(preamble_bits) - 1)
    group_bit_lengths: List[int] = []
    search_buffer = ""
    found_first_group = False
    writer = BitFileWriter(output_path)

    try:
        for bit_chunk in _iter_transformed_bit_chunks(source_path, config, progress_callback):
            if not bit_chunk:
                continue

            if not found_first_group:
                search_buffer += bit_chunk
                first_index = search_buffer.find(preamble_bits)
                if first_index == -1:
                    search_buffer = search_buffer[-overlap_length:] if overlap_length else ""
                    continue
                found_first_group = True
                search_buffer = search_buffer[first_index:]
                previous_length = len(preamble_bits)
            else:
                previous_length = len(search_buffer)
                search_buffer += bit_chunk

            if search_buffer.count(preamble_bits) > MAX_GROUP_BOUNDARIES_PER_BUFFER:
                raise ValueError(
                    "Preamble sync produced too many groups in the current chunk. "
                    "Choose a longer or more selective preamble."
                )

            search_from = max(len(preamble_bits), previous_length - len(preamble_bits) + 1)
            while True:
                next_index = search_buffer.find(preamble_bits, search_from)
                if next_index == -1:
                    break

                group_length = _write_group_bits(writer, search_buffer[:next_index], remove_ranges)
                if group_length > 0:
                    group_bit_lengths.append(group_length)

                search_buffer = search_buffer[next_index:]
                search_from = len(preamble_bits)

        if found_first_group and search_buffer:
            final_group_length = _write_group_bits(writer, search_buffer, remove_ranges)
            if final_group_length > 0:
                group_bit_lengths.append(final_group_length)
    finally:
        writer.close()

    if progress_callback:
        progress_callback(100)

    return ProcessedViewResult(
        size_bytes=output_path.stat().st_size,
        logical_bit_length=writer.total_bits,
        group_bit_lengths=group_bit_lengths,
    )
