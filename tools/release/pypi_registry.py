# ruff: noqa: INP001
"""Compare the audited release files with the published PyPI version."""

import argparse
import hashlib
import json
import shutil
import time
from http import HTTPStatus
from pathlib import Path
from urllib.error import HTTPError
from urllib.parse import quote
from urllib.request import urlopen


def missing_files(distributions: Path) -> list[Path]:
  """Return the files PyPI still needs, rejecting unexpected or changed files."""
  (sdist,) = distributions.glob("sakai-*.tar.gz")
  version = sdist.name.removeprefix("sakai-").removesuffix(".tar.gz")
  local = {path.name: path for path in distributions.iterdir()}
  url = f"https://pypi.org/pypi/sakai/{quote(version, safe='')}/json"
  try:
    with urlopen(url, timeout=15) as response:
      published = json.load(response)["urls"]
  except HTTPError as error:
    if error.code != HTTPStatus.NOT_FOUND:
      raise
    return list(local.values())

  for file in published:
    name = file["filename"]
    path = local.pop(name, None)
    if path is None or file["yanked"]:
      msg = f"PyPI has an unexpected or yanked file for {version}: {name}"
      raise ValueError(msg)
    with path.open("rb") as archive:
      digest = hashlib.file_digest(archive, "sha256").hexdigest()
    if file["digests"]["sha256"] != digest:
      msg = f"PyPI checksum differs from the audited archive: {name}"
      raise ValueError(msg)
  return list(local.values())


def verify(distributions: Path) -> None:
  """Wait for PyPI to expose all uploaded files and verify their checksums."""
  for _ in range(10):
    if not missing_files(distributions):
      return
    time.sleep(3)
  msg = "PyPI did not return all audited release files"
  raise RuntimeError(msg)


if __name__ == "__main__":
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument("distributions", type=Path)
  parser.add_argument("--stage", type=Path)
  args = parser.parse_args()
  if args.stage is None:
    verify(args.distributions)
  else:
    args.stage.mkdir()
    for missing in missing_files(args.distributions):
      shutil.copyfile(missing, args.stage / missing.name)
