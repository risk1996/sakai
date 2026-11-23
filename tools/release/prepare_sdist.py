# ruff: noqa: INP001
"""Prune the source distribution's Cargo lockfile to its packaged workspace."""

import argparse
import io
import shutil
import subprocess
import tarfile
import tempfile
import tomllib
from pathlib import Path

Package = tuple[str, str, str | None, str | None]


def lock_graph(lockfile: bytes) -> dict[Package, set[Package]]:
  """Resolve Cargo's abbreviated dependency names to locked package identities."""
  packages = tomllib.loads(lockfile.decode())["package"]
  aliases: dict[str, Package] = {}
  graph: dict[Package, set[Package]] = {}
  for entry in packages:
    package = (
      entry["name"],
      entry["version"],
      entry.get("source"),
      entry.get("checksum"),
    )
    name, version, source, _ = package
    aliases[name] = package
    aliases[f"{name} {version}"] = package
    if source is not None:
      aliases[f"{name} {version} ({source})"] = package
    graph[package] = set()

  for entry, dependencies in zip(packages, graph.values(), strict=True):
    dependencies.update(aliases[alias] for alias in entry.get("dependencies", ()))
  return graph


def prepare_sdist(archive: Path) -> None:
  """Rewrite only Cargo.lock after confirming the retained graph is unchanged."""
  cargo = shutil.which("cargo")
  if cargo is None:
    msg = "Cargo is required to prepare the source distribution"
    raise RuntimeError(msg)
  root = archive.name.removesuffix(".tar.gz")
  with tempfile.TemporaryDirectory(dir=archive.parent) as directory:
    work = Path(directory)
    with tarfile.open(archive, "r:gz") as original:
      original.extractall(work, filter="data")
      lock = work / root / "Cargo.lock"
      before = lock_graph(lock.read_bytes())
      # The original workspace dependencies must already be in Cargo's cache.
      subprocess.run(  # noqa: S603
        [cargo, "metadata", "--format-version", "1", "--offline"],
        cwd=lock.parent,
        stdout=subprocess.DEVNULL,
        check=True,
      )
      pruned = lock.read_bytes()
      after = lock_graph(pruned)
      for package, dependencies in after.items():
        if package not in before or not dependencies <= before[package]:
          msg = f"Source distribution resolved a different dependency: {package}"
          raise ValueError(msg)

      candidate = work / archive.name
      with tarfile.open(candidate, "w:gz") as output:
        for member in original.getmembers():
          if member.name == f"{root}/Cargo.lock":
            member.size = len(pruned)
            output.addfile(member, io.BytesIO(pruned))
          else:
            contents = original.extractfile(member) if member.isfile() else None
            output.addfile(member, contents)
    candidate.replace(archive)


if __name__ == "__main__":
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument("archive", type=Path)
  prepare_sdist(parser.parse_args().archive)
