from datetime import datetime
from typing import List, Optional

from pydantic import BaseModel, Field, validator


class BaseMetadata(BaseModel):
    filename: str
    size_bytes: int = Field(alias="sizeBytes")
    logical_bit_length: int = Field(alias="logicalBitLength")
    created_at: datetime = Field(alias="createdAt")
    source_file_id: Optional[str] = Field(default=None, alias="sourceFileId")
    group_bit_lengths: Optional[List[int]] = Field(default=None, alias="groupBitLengths")
    is_filtered: bool = Field(default=False, alias="isFiltered")

    class Config:
        allow_population_by_field_name = True


class FileMetadata(BaseMetadata):
    file_id: str = Field(alias="fileId")

    class Config:
        allow_population_by_field_name = True


class ViewMetadata(BaseMetadata):
    view_id: str = Field(alias="viewId")

    class Config:
        allow_population_by_field_name = True


class UploadResponse(BaseModel):
    file_id: str = Field(alias="fileId")
    filename: str
    size_bytes: int = Field(alias="sizeBytes")
    logical_bit_length: int = Field(alias="logicalBitLength")

    class Config:
        allow_population_by_field_name = True


class ByteRange(BaseModel):
    start: int
    end: int
    length: int


class BitRange(BaseModel):
    start: int
    end: int
    length: int


class ViewportRow(BaseModel):
    row_index: int = Field(alias="rowIndex")
    bit_offset: int = Field(alias="bitOffset")
    bit_length: int = Field(alias="bitLength")
    byte_offset_start: int = Field(alias="byteOffsetStart")
    byte_offset_end: int = Field(alias="byteOffsetEnd")
    bits: str
    hex: str
    ascii: str

    class Config:
        allow_population_by_field_name = True


class ViewportResponse(BaseModel):
    file_id: str = Field(alias="fileId")
    filename: str
    size_bytes: int = Field(alias="sizeBytes")
    requested_bit_offset: int = Field(alias="requestedBitOffset")
    visible_rows: int = Field(alias="visibleRows")
    row_width_bits: int = Field(alias="rowWidthBits")
    actual_rows: int = Field(alias="actualRows")
    bit_range: BitRange = Field(alias="bitRange")
    byte_range: ByteRange = Field(alias="byteRange")
    rows: List[ViewportRow]

    class Config:
        allow_population_by_field_name = True


class FilterBitRange(BaseModel):
    start_bit: int = Field(alias="startBit", ge=0)
    length: int = Field(alias="length", gt=0)

    class Config:
        allow_population_by_field_name = True


class FilterConfig(BaseModel):
    invert_bits: bool = Field(default=False, alias="invertBits")
    reverse_bits_per_byte: bool = Field(default=False, alias="reverseBitsPerByte")
    xor_mask: Optional[int] = Field(default=None, alias="xorMask")
    preamble_bits: Optional[str] = Field(default=None, alias="preambleBits")
    remove_ranges: List[FilterBitRange] = Field(default_factory=list, alias="removeRanges")

    class Config:
        allow_population_by_field_name = True

    @validator("xor_mask")
    def validate_xor_mask(cls, value: Optional[int]) -> Optional[int]:
        if value is None:
            return value
        if value < 0 or value > 255:
            raise ValueError("xorMask must be between 0 and 255")
        return value

    @validator("preamble_bits")
    def validate_preamble_bits(cls, value: Optional[str]) -> Optional[str]:
        if value is None or value == "":
            return None
        if any(character not in {"0", "1"} for character in value):
            raise ValueError("preambleBits must contain only 0 and 1")
        return value

    @validator("remove_ranges")
    def validate_remove_ranges(cls, value: List[FilterBitRange], values: dict) -> List[FilterBitRange]:
        if value and not values.get("preamble_bits"):
            raise ValueError("removeRanges requires preambleBits grouping")
        return value


class CreateFilterJobResponse(BaseModel):
    job_id: str = Field(alias="jobId")

    class Config:
        allow_population_by_field_name = True


class FilterJobStatusResponse(BaseModel):
    job_id: str = Field(alias="jobId")
    source_file_id: str = Field(alias="sourceFileId")
    status: str
    progress: int
    view_id: Optional[str] = Field(default=None, alias="viewId")
    error: Optional[str] = None

    class Config:
        allow_population_by_field_name = True
