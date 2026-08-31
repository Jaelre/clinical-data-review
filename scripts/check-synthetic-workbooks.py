#!/usr/bin/env python3
"""Reject unsafe or identifying content in the committed synthetic XLSX fixtures."""

from __future__ import annotations

import re
import sys
import zipfile
from pathlib import Path
from xml.etree import ElementTree as ET

NS = {
    "main": "http://schemas.openxmlformats.org/spreadsheetml/2006/main",
    "rel": "http://schemas.openxmlformats.org/package/2006/relationships",
    "core": "http://schemas.openxmlformats.org/package/2006/metadata/core-properties",
    "dc": "http://purl.org/dc/elements/1.1/",
}
FORBIDDEN_MEMBERS = ("vbaProject.bin", "externalLinks/", "connections.xml", "custom.xml")
EMAIL = re.compile(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}")


def fail(message: str) -> None:
    raise SystemExit(f"synthetic workbook check failed: {message}")


def xml(archive: zipfile.ZipFile, member: str) -> ET.Element:
    try:
        return ET.fromstring(archive.read(member))
    except (KeyError, ET.ParseError) as error:
        fail(f"{archive.filename}: cannot parse {member}: {error}")


def inspect(path: Path) -> None:
    if path.suffix.lower() != ".xlsx":
        fail(f"unsupported workbook type: {path}")

    with zipfile.ZipFile(path) as archive:
        members = archive.namelist()
        for member in members:
            if any(forbidden in member for forbidden in FORBIDDEN_MEMBERS):
                fail(f"{path}: forbidden package member {member}")

        workbook = xml(archive, "xl/workbook.xml")
        sheets = workbook.findall("main:sheets/main:sheet", NS)
        if not sheets:
            fail(f"{path}: no worksheets")
        for sheet in sheets:
            if sheet.attrib.get("state", "visible") != "visible":
                fail(f"{path}: hidden worksheet {sheet.attrib.get('name', '<unnamed>')}")

        for member in members:
            if member.endswith(".rels"):
                relationships = xml(archive, member)
                for relationship in relationships:
                    if relationship.attrib.get("TargetMode") == "External":
                        fail(f"{path}: external relationship in {member}")

        core = xml(archive, "docProps/core.xml")
        for tag in ("dc:creator", "core:lastModifiedBy"):
            value = core.findtext(tag, default="", namespaces=NS).strip()
            if value:
                fail(f"{path}: identifying package property {tag}={value!r}")

        shared_strings: list[str] = []
        if "xl/sharedStrings.xml" in members:
            shared = xml(archive, "xl/sharedStrings.xml")
            for item in shared.findall("main:si", NS):
                shared_strings.append("".join(item.itertext()))

        cell_count = 0
        for member in members:
            if not member.startswith("xl/worksheets/sheet") or not member.endswith(".xml"):
                continue
            worksheet = xml(archive, member)
            if worksheet.find(".//main:f", NS) is not None:
                fail(f"{path}: formula found in {member}")
            for element in worksheet.findall(".//*[@hidden='1']", NS):
                fail(f"{path}: hidden row or column in {member}: {element.tag}")
            for cell in worksheet.findall(".//main:c", NS):
                cell_count += 1
                value = ""
                cell_type = cell.attrib.get("t")
                raw = cell.findtext("main:v", default="", namespaces=NS)
                if cell_type == "s" and raw:
                    try:
                        value = shared_strings[int(raw)]
                    except (IndexError, ValueError) as error:
                        fail(f"{path}: invalid shared-string reference: {error}")
                elif cell_type == "inlineStr":
                    inline = cell.find("main:is", NS)
                    value = "" if inline is None else "".join(inline.itertext())
                else:
                    value = raw
                for address in EMAIL.findall(value):
                    if not address.lower().endswith(".invalid"):
                        fail(f"{path}: non-reserved email address in a cell")

        if cell_count == 0:
            fail(f"{path}: workbook has no cells")


def main() -> None:
    fixture_dir = Path(sys.argv[1]) if len(sys.argv) == 2 else Path("fixtures/synthetic")
    workbooks = sorted(fixture_dir.glob("*.xlsx"))
    if not workbooks:
        fail(f"no XLSX fixtures found under {fixture_dir}")
    for workbook in workbooks:
        inspect(workbook)
    print(f"validated {len(workbooks)} synthetic workbooks")


if __name__ == "__main__":
    main()
