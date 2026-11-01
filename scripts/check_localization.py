"""Validate embedded catalogs and explicit production message references."""
import json
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parent.parent


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"Duplicate message: {key}")
        result[key] = value
    return result


catalogs = {
    code: json.loads(
        (ROOT / "src" / "locales" / f"{code}.json").read_text(encoding="utf-8"),
        object_pairs_hook=unique_object,
    )
    for code in ("en", "ru", "fr", "de", "es", "it")
}
english = catalogs["en"]
for code, catalog in catalogs.items():
    if catalog.keys() != english.keys():
        raise ValueError(f"{code}: catalog keys do not match English")
    for key, value in catalog.items():
        if not isinstance(value, str) or not value.strip():
            raise ValueError(f"{code}: empty or invalid message: {key}")
        if sorted(re.findall(r"\{([^{}]+)\}", key)) != sorted(
            re.findall(r"\{([^{}]+)\}", value)
        ):
            raise ValueError(f"{code}: changed placeholders: {key}")

references = set()
for source in (ROOT / "src" / "main.rs", ROOT / "src" / "capture.rs"):
    text = source.read_text(encoding="utf-8").split("#[cfg(test)]", 1)[0]
    for pattern in (
        r'(?:tr|i18n::format)\(\s*lang,\s*("(?:[^"\\]|\\.)*")',
        r'Message::(?:key|with)\(\s*("(?:[^"\\]|\\.)*")',
    ):
        references.update(json.loads(match) for match in re.findall(pattern, text))
missing = references - english.keys()
if missing:
    raise ValueError(f"Production messages missing from catalogs: {sorted(missing)}")
print(f"Validated {len(english)} messages in six catalogs and {len(references)} production references.")
