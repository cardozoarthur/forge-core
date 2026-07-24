#!/usr/bin/env python3
"""Create and verify release archives with normalized metadata."""

from __future__ import annotations

import argparse
import gzip
import os
import stat
import struct
import tarfile
import tempfile
import zipfile
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path, PurePosixPath
from typing import BinaryIO, Iterable


ZIP_EPOCH = 315532800


@dataclass(frozen=True)
class Entry:
    name: str
    path: Path
    is_dir: bool
    mode: int

    @property
    def archive_name(self) -> str:
        if self.is_dir:
            return f"{self.name}/"
        return self.name


def normalized_epoch(value: str | None) -> int:
    raw = value or os.environ.get("SOURCE_DATE_EPOCH")
    if raw is None:
        raise ValueError("SOURCE_DATE_EPOCH or --epoch is required")
    epoch = int(raw)
    if epoch < 0:
        raise ValueError("archive epoch must be non-negative")
    return epoch


def normalized_member(value: str) -> PurePosixPath:
    member = PurePosixPath(value)
    if member.is_absolute() or not member.parts or ".." in member.parts:
        raise ValueError(f"unsafe archive member: {value}")
    normalized = PurePosixPath(*(part for part in member.parts if part != "."))
    if not normalized.parts:
        raise ValueError(f"unsafe archive member: {value}")
    return normalized


def normalized_mode(path: Path, is_dir: bool) -> int:
    if is_dir:
        return 0o755
    mode = stat.S_IMODE(path.stat().st_mode)
    return 0o755 if mode & 0o111 else 0o644


def collect_entries(source: Path, members: Iterable[str]) -> list[Entry]:
    source = source.resolve(strict=True)
    paths: dict[str, tuple[Path, bool]] = {}

    for value in members:
        member = normalized_member(value)
        candidate = source.joinpath(*member.parts)
        candidate.lstat()
        if candidate.is_symlink():
            raise ValueError(f"symlinks are not allowed in release archives: {value}")
        resolved = candidate.resolve(strict=True)
        if resolved != source and source not in resolved.parents:
            raise ValueError(f"archive member escapes source directory: {value}")
        if resolved.is_dir():
            paths[member.as_posix()] = (resolved, True)
            for child in sorted(resolved.rglob("*")):
                if child.is_symlink():
                    raise ValueError(
                        f"symlinks are not allowed in release archives: {child}"
                    )
                if not child.is_dir() and not child.is_file():
                    raise ValueError(
                        f"unsupported release archive member type: {child}"
                    )
                child_name = child.relative_to(source).as_posix()
                paths[child_name] = (child, child.is_dir())
        elif resolved.is_file():
            paths[member.as_posix()] = (resolved, False)
        else:
            raise ValueError(f"unsupported release archive member type: {value}")

    for name in tuple(paths):
        for parent in PurePosixPath(name).parents:
            if parent == PurePosixPath("."):
                continue
            parent_path = source.joinpath(*parent.parts)
            paths.setdefault(parent.as_posix(), (parent_path, True))

    return [
        Entry(name, path, is_dir, normalized_mode(path, is_dir))
        for name, (path, is_dir) in sorted(paths.items())
    ]


def write_tar_entry(archive: tarfile.TarFile, entry: Entry, epoch: int) -> None:
    info = tarfile.TarInfo(entry.name)
    info.uid = 0
    info.gid = 0
    info.uname = "root"
    info.gname = "root"
    info.mode = entry.mode
    info.mtime = epoch
    if entry.is_dir:
        info.type = tarfile.DIRTYPE
        info.name = entry.archive_name
        archive.addfile(info)
        return

    info.type = tarfile.REGTYPE
    info.size = entry.path.stat().st_size
    with entry.path.open("rb") as source_file:
        archive.addfile(info, source_file)


def create_tar_gz(output: Path, entries: list[Entry], epoch: int) -> None:
    with output.open("wb") as raw:
        with gzip.GzipFile(
            filename="", mode="wb", fileobj=raw, compresslevel=6, mtime=epoch
        ) as compressed:
            with tarfile.open(
                fileobj=compressed, mode="w", format=tarfile.USTAR_FORMAT
            ) as archive:
                for entry in entries:
                    write_tar_entry(archive, entry, epoch)


def zip_datetime(epoch: int) -> tuple[int, int, int, int, int, int]:
    value = datetime.fromtimestamp(max(epoch, ZIP_EPOCH), timezone.utc)
    return (
        value.year,
        value.month,
        value.day,
        value.hour,
        value.minute,
        value.second & ~1,
    )


def write_zip_entry(
    archive: zipfile.ZipFile,
    entry: Entry,
    date_time: tuple[int, int, int, int, int, int],
) -> None:
    info = zipfile.ZipInfo(entry.archive_name, date_time)
    info.create_system = 3
    info.compress_type = zipfile.ZIP_STORED
    file_type = stat.S_IFDIR if entry.is_dir else stat.S_IFREG
    info.external_attr = (file_type | entry.mode) << 16
    if entry.is_dir:
        info.external_attr |= 0x10
        archive.writestr(info, b"")
    else:
        archive.writestr(info, entry.path.read_bytes())


def create_zip(output: Path, entries: list[Entry], epoch: int) -> None:
    with zipfile.ZipFile(output, mode="w", allowZip64=True) as archive:
        date_time = zip_datetime(epoch)
        for entry in entries:
            write_zip_entry(archive, entry, date_time)


def verify_tar_gzip_header(stream: BinaryIO, epoch: int) -> None:
    header = stream.read(10)
    if len(header) != 10 or header[:3] != b"\x1f\x8b\x08":
        raise ValueError("archive does not have a valid gzip header")
    if header[3] != 0:
        raise ValueError("gzip header contains non-reproducible optional fields")
    actual_epoch = struct.unpack("<I", header[4:8])[0]
    if actual_epoch != epoch:
        raise ValueError(
            f"gzip mtime is {actual_epoch}; expected SOURCE_DATE_EPOCH {epoch}"
        )
    if header[8:] != b"\x00\xff":
        raise ValueError("gzip compression and operating-system fields are not normalized")


def verify_tar_gz(
    archive_path: Path, entries: list[Entry], epoch: int
) -> None:
    with archive_path.open("rb") as raw:
        verify_tar_gzip_header(raw, epoch)

    expected_names = [entry.name for entry in entries]
    with tarfile.open(archive_path, mode="r:gz") as archive:
        members = archive.getmembers()
        actual_names = [member.name for member in members]
        if actual_names != expected_names:
            raise ValueError(
                f"tar entry order mismatch: {actual_names!r} != {expected_names!r}"
            )

        for entry, member in zip(entries, members, strict=True):
            expected_type = tarfile.DIRTYPE if entry.is_dir else tarfile.REGTYPE
            if member.type != expected_type:
                raise ValueError(f"unexpected tar member type for {entry.name}")
            metadata = (
                member.uid,
                member.gid,
                member.uname,
                member.gname,
                member.mode,
                member.mtime,
                member.pax_headers,
            )
            expected_metadata = (0, 0, "root", "root", entry.mode, epoch, {})
            if metadata != expected_metadata:
                raise ValueError(
                    f"non-normalized tar metadata for {entry.name}: {metadata!r}"
                )
            if entry.is_dir:
                continue
            archived = archive.extractfile(member)
            if archived is None or archived.read() != entry.path.read_bytes():
                raise ValueError(f"tar content mismatch for {entry.name}")


def verify_zip(archive_path: Path, entries: list[Entry], epoch: int) -> None:
    expected_names = [entry.archive_name for entry in entries]
    expected_date_time = zip_datetime(epoch)
    with zipfile.ZipFile(archive_path, mode="r") as archive:
        if archive.comment:
            raise ValueError("zip archive comment must be empty")
        infos = archive.infolist()
        actual_names = [info.filename for info in infos]
        if actual_names != expected_names:
            raise ValueError(
                f"zip entry order mismatch: {actual_names!r} != {expected_names!r}"
            )

        for entry, info in zip(entries, infos, strict=True):
            mode = (info.external_attr >> 16) & 0o777
            file_type = stat.S_IFMT(info.external_attr >> 16)
            expected_file_type = stat.S_IFDIR if entry.is_dir else stat.S_IFREG
            metadata = (
                info.create_system,
                info.date_time,
                mode,
                file_type,
                info.compress_type,
                info.extra,
                info.comment,
                bool(info.flag_bits & 0x1),
            )
            expected_metadata = (
                3,
                expected_date_time,
                entry.mode,
                expected_file_type,
                zipfile.ZIP_STORED,
                b"",
                b"",
                False,
            )
            if metadata != expected_metadata:
                raise ValueError(
                    f"non-normalized zip metadata for {entry.name}: {metadata!r}"
                )
            expected_content = b"" if entry.is_dir else entry.path.read_bytes()
            if archive.read(info) != expected_content:
                raise ValueError(f"zip content mismatch for {entry.name}")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("action", choices=("create", "verify"))
    parser.add_argument("--format", required=True, choices=("tar.gz", "zip"))
    parser.add_argument("--source", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--epoch")
    parser.add_argument("members", nargs="+")
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    epoch = normalized_epoch(args.epoch)
    entries = collect_entries(args.source, args.members)
    if not entries:
        raise ValueError("release archive must contain at least one member")

    if args.action == "create":
        args.output.parent.mkdir(parents=True, exist_ok=True)
        with tempfile.NamedTemporaryFile(
            dir=args.output.parent, prefix=f".{args.output.name}.", delete=False
        ) as temporary:
            temporary_path = Path(temporary.name)
        try:
            if args.format == "tar.gz":
                create_tar_gz(temporary_path, entries, epoch)
            else:
                create_zip(temporary_path, entries, epoch)
            os.replace(temporary_path, args.output)
        finally:
            temporary_path.unlink(missing_ok=True)
        return

    if args.format == "tar.gz":
        verify_tar_gz(args.output, entries, epoch)
    else:
        verify_zip(args.output, entries, epoch)


if __name__ == "__main__":
    main()
