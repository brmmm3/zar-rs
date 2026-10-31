#!/usr/bin/env python3
"""Merge the packaged ZAR actions into the current user's Thunar config."""

import os
from pathlib import Path
import tempfile
import xml.etree.ElementTree as ET


def main() -> None:
    source = Path("/usr/share/zar-rs/thunar-uca.xml")
    target = Path.home() / ".config/Thunar/uca.xml"
    parser = ET.XMLParser(target=ET.TreeBuilder(insert_comments=True))
    incoming = ET.parse(source).getroot()
    if target.exists():
        existing = ET.parse(target, parser=parser).getroot()
        if existing.tag != "actions":
            raise ValueError(f"Unexpected Thunar action root in {target}")
    else:
        existing = ET.Element("actions")

    identities = {
        node.findtext("unique-id") for node in existing.findall("action")
    }
    for action in incoming.findall("action"):
        identity = action.findtext("unique-id")
        if identity not in identities:
            existing.append(action)
            identities.add(identity)

    target.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary_name = tempfile.mkstemp(
        prefix="uca.", suffix=".xml", dir=target.parent
    )
    try:
        with os.fdopen(descriptor, "wb") as output:
            ET.ElementTree(existing).write(
                output, encoding="utf-8", xml_declaration=True
            )
        os.replace(temporary_name, target)
    finally:
        if os.path.exists(temporary_name):
            os.unlink(temporary_name)
    print(f"Installed ZAR actions in {target}")


if __name__ == "__main__":
    main()
