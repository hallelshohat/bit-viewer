import pytest

from app.filtering import process_filtered_view
from app.models import FilterConfig


def bits_to_bytes(bit_string: str) -> bytes:
    padded = bit_string + ("0" * ((8 - (len(bit_string) % 8)) % 8))
    return bytes(int(padded[index : index + 8], 2) for index in range(0, len(padded), 8))


def bytes_to_bits(data: bytes, bit_length: int) -> str:
    return "".join(f"{value:08b}" for value in data)[:bit_length]


def test_process_filtered_view_applies_byte_transforms(tmp_path) -> None:
    source_path = tmp_path / "source.bin"
    output_path = tmp_path / "output.bin"
    source_path.write_bytes(bytes([0b00001111, 0b10100000]))

    result = process_filtered_view(
        source_path=source_path,
        output_path=output_path,
        config=FilterConfig(invertBits=True, reverseBitsPerByte=True, xorMask=0x0F),
    )

    assert result.logical_bit_length == 16
    assert result.group_bit_lengths is None
    assert output_path.read_bytes() == bytes([0x00, 0xF5])


def test_process_filtered_view_groups_on_preamble_and_removes_ranges(tmp_path) -> None:
    source_bits = "000000101100111101100000"
    source_path = tmp_path / "grouped-source.bin"
    output_path = tmp_path / "grouped-output.bin"
    source_path.write_bytes(bits_to_bytes(source_bits))

    result = process_filtered_view(
        source_path=source_path,
        output_path=output_path,
        config=FilterConfig(
            preambleBits="101100",
            removeRanges=[{"startBit": 6, "length": 2}],
        ),
    )

    output_bits = bytes_to_bits(output_path.read_bytes(), result.logical_bit_length)

    assert result.group_bit_lengths == [7, 7]
    assert result.logical_bit_length == 14
    assert output_bits == "10110011011000"


def test_process_filtered_view_rejects_overly_dense_preamble(tmp_path) -> None:
    source_path = tmp_path / "dense-source.bin"
    output_path = tmp_path / "dense-output.bin"
    source_path.write_bytes(bytes([0x00]) * (64 * 1024))

    with pytest.raises(ValueError, match="too many groups"):
        process_filtered_view(
            source_path=source_path,
            output_path=output_path,
            config=FilterConfig(preambleBits="0000"),
        )
