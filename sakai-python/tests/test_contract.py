"""Installed-package contract checks. Run on Linux from outside the checkout."""

import ast
import errno
import inspect
import os
import tempfile
import unittest
from collections.abc import Callable
from pathlib import Path
from typing import Any, TypeVar, cast, get_origin

import sakai
import sakai._sakai as native
from sakai import (
  Cgroup,
  CgroupEvents,
  CgroupStat,
  CpuMax,
  CpuStat,
  InterfaceMissingError,
  MaxOr,
  MemoryNumaStat,
  MemoryStat,
  NotCgroupV2Error,
  PidsEvents,
  PidsReader,
  Pressure,
  SwapEvents,
  SwapReader,
  ZswapReader,
)

_Result = TypeVar("_Result")


class ContractTest(unittest.TestCase):
  @classmethod
  def setUpClass(cls) -> None:
    cls.group = Cgroup.current()

  def check_optional(
    self, name: str, read: Callable[[], _Result], check: Callable[[_Result], None]
  ) -> None:
    with self.subTest(interface=name):
      try:
        value = read()
      except InterfaceMissingError as error:
        self.assertEqual(error.path, self.group.path / name)
      else:
        check(value)

  def check_unsigned(
    self, value: float, value_type: type[int] | type[float] = int
  ) -> None:
    self.assertIsInstance(value, value_type)
    self.assertGreaterEqual(value, 0)

  def check_limit(
    self,
    limit: MaxOr[int] | MaxOr[float],
    value_type: type[int] | type[float] = int,
    maximum: int | None = None,
  ) -> None:
    self.assertIsInstance(limit, MaxOr)
    if limit.is_max:
      with self.assertRaises(ValueError):
        _ = limit.value
    else:
      self.check_unsigned(limit.value, value_type)
      if maximum is not None:
        self.assertLessEqual(limit.value, maximum)

  def test_native_documentation(self) -> None:
    self.assertIn("Read-only Linux cgroup v2", inspect.getdoc(native) or "")
    expected_docs = {
      "SakaiError": "Base error for Sakai",
      "InterfaceMissingError": "interface file is missing",
      "NotCgroupV2Error": "not on a cgroup v2",
      "DeletedCgroupError": "pinned cgroup has been deleted",
      "CgroupParseError": "could not be parsed",
      "NotSupportedError": "not supported for this cgroup",
    }
    for class_name, phrase in expected_docs.items():
      with self.subTest(class_name=class_name):
        self.assertIn(phrase, inspect.getdoc(getattr(sakai, class_name)) or "")
    # Validate the installed extension against its separately maintained stub.
    stub = ast.parse(Path(sakai.__file__).with_name("_sakai.pyi").read_text())
    classes = [node for node in stub.body if isinstance(node, ast.ClassDef)]
    self.assertTrue(classes)
    self.assertEqual({node.name for node in classes}, set(sakai.__all__))
    for declaration in classes:
      with self.subTest(class_name=declaration.name):
        cls = getattr(sakai, declaration.name)
        self.assertTrue(inspect.getdoc(cls))
        for member in declaration.body:
          if isinstance(member, ast.FunctionDef):
            with self.subTest(member_name=member.name):
              self.assertTrue(inspect.getdoc(getattr(cls, member.name)))

  def test_pinned_handle_and_pathlike(self) -> None:
    self.assertIsInstance(self.group.path, Path)
    self.assertEqual(Cgroup.from_path(self.group.path).path, self.group.path)
    self.assertEqual(
      Cgroup.from_path(os.fsencode(self.group.path)).path, self.group.path
    )
    parent = Cgroup.from_path(self.group.path)
    reader = parent.cpu()
    del parent
    self.assertIsInstance(reader.stat(), CpuStat)
    self.assertIsInstance(self.group.children(), list)

  def test_pid_validation(self) -> None:
    for pid in (-1, 2**100):
      with self.subTest(pid=pid), self.assertRaises((ValueError, OverflowError)):
        Cgroup.from_pid(pid)
    for pid in (0, 2**31):
      with self.subTest(pid=pid), self.assertRaises(OSError):
        Cgroup.from_pid(pid)

  def test_invalid_path_and_child(self) -> None:
    with tempfile.TemporaryDirectory() as path, self.assertRaises(NotCgroupV2Error):
      Cgroup.from_path(path)
    for name in ("", ".", "..", "../other", "a/b"):
      with self.subTest(name=name), self.assertRaises(OSError):
        self.group.child(name)

  def test_cpu_contract(self) -> None:
    cpu = self.group.cpu()
    stat = cpu.stat()
    self.check_unsigned(stat.time.usage_ns)
    with self.assertRaises(AttributeError):
      cast("Any", stat.time).usage_ns = 5
    weight = cpu.weight()
    self.assertEqual(weight.is_idle, weight.shares is None)
    if weight.shares is not None:
      self.assertTrue(1 <= weight.shares <= 10_000)
    self.assertTrue(-20 <= cpu.weight_nice() <= 19)
    self.assertGreaterEqual(cpu.max_burst(), 0)
    self.assertIsInstance(cpu.idle(), bool)
    self.check_optional(
      "cpu.uclamp.min", cpu.uclamp_min, lambda value: self.assertTrue(0 <= value <= 1)
    )
    self.check_optional(
      "cpu.uclamp.max", cpu.uclamp_max, lambda value: self.check_limit(value, float, 1)
    )
    self.assertIsInstance(cpu.pressure(), Pressure)
    self.check_optional(
      "cpu.stat.local",
      cpu.stat_local,
      lambda value: self.assertTrue(
        value.throttled_ns is None or value.throttled_ns >= 0
      ),
    )

  def test_cpu_max_contract(self) -> None:
    maximum = self.group.cpu().max()
    self.assertIsInstance(maximum, CpuMax)
    self.assertGreater(maximum.period_ns, 0)
    self.assertIs(get_origin(MaxOr[int]), MaxOr)
    self.check_limit(maximum.quota_ns)
    self.check_limit(maximum.cpu_count, float)
    self.assertEqual(maximum.quota_ns.is_max, maximum.cpu_count.is_max)
    if not maximum.quota_ns.is_max:
      self.assertAlmostEqual(
        maximum.cpu_count.value, maximum.quota_ns.value / maximum.period_ns
      )

  def test_memory_contract(self) -> None:
    memory = self.group.memory()
    for read in (memory.current, memory.peak, memory.low, memory.min):
      self.assertGreaterEqual(read(), 0)
    for read in (memory.max, memory.high):
      self.check_limit(read())
    stat = memory.stat()
    self.assertIsInstance(stat, MemoryStat)
    self.assertIn("anon", stat.bytes)
    self.assertIn("file", stat.bytes)
    with self.assertRaises(TypeError):
      cast("Any", stat.bytes)["anon"] = 0
    self.assertTrue(all(isinstance(v, int) for v in stat.pages.values()))
    self.assertTrue(all(isinstance(v, int) for v in stat.counts.values()))
    self.assertIsInstance(memory.pressure(), Pressure)

  def test_core_contract(self) -> None:
    self.assertIn(
      self.group.core().kind(),
      {"domain", "domain threaded", "domain invalid", "threaded"},
    )
    self.assertTrue(all(isinstance(v, str) for v in self.group.core().controllers()))
    self.assertTrue(
      all(isinstance(v, str) for v in self.group.core().subtree_control())
    )
    self.assertEqual(Cgroup.from_pid(os.getpid()).path, self.group.path)

  def test_swap_contract(self) -> None:
    swap = self.group.memory().swap()
    self.assertIsInstance(swap, SwapReader)
    for name, read in (("current", swap.current), ("peak", swap.peak)):
      self.check_optional(f"memory.swap.{name}", read, self.check_unsigned)
    for name, read in (("max", swap.max), ("high", swap.high)):
      self.check_optional(f"memory.swap.{name}", read, self.check_limit)

    def check_events(events: SwapEvents) -> None:
      self.assertIsInstance(events, SwapEvents)
      self.assertTrue(events.high is None or events.high >= 0)
      self.check_unsigned(events.max)
      self.check_unsigned(events.fail)
      with self.assertRaises(AttributeError):
        cast("Any", events).fail = 0

    self.check_optional("memory.swap.events", swap.events, check_events)

  def test_zswap_contract(self) -> None:
    zswap = self.group.memory().zswap()
    self.assertIsInstance(zswap, ZswapReader)
    self.check_optional("memory.zswap.current", zswap.current, self.check_unsigned)
    self.check_optional("memory.zswap.max", zswap.max, self.check_limit)
    self.check_optional(
      "memory.zswap.writeback",
      zswap.writeback,
      lambda value: self.assertIsInstance(value, bool),
    )

  def test_pids_contract(self) -> None:
    pids = self.group.pids()
    self.assertIsInstance(pids, PidsReader)
    self.check_optional("pids.current", pids.current, self.check_unsigned)
    self.check_optional("pids.max", pids.max, self.check_limit)

    def check_events(events: PidsEvents) -> None:
      self.assertIsInstance(events, PidsEvents)
      self.check_unsigned(events.max)
      with self.assertRaises(AttributeError):
        cast("Any", events).max = 0

    self.check_optional("pids.events", pids.events, check_events)

  def test_cgroup_state_contract(self) -> None:
    def check_events(events: CgroupEvents) -> None:
      self.assertIsInstance(events, CgroupEvents)
      self.assertIsInstance(events.populated, bool)
      if events.frozen is not None:
        self.assertIsInstance(events.frozen, bool)
      with self.assertRaises(AttributeError):
        cast("Any", events).populated = False

    self.check_optional("cgroup.events", self.group.core().events, check_events)
    stat = self.group.core().stat()
    self.assertIsInstance(stat, CgroupStat)
    self.check_unsigned(stat.descendants)
    self.check_unsigned(stat.dying_descendants)
    for mapping in (stat.subsystems, stat.dying_subsystems):
      self.assertTrue(all(isinstance(key, str) for key in mapping))
      for value in mapping.values():
        self.check_unsigned(value)
      with self.assertRaises(TypeError):
        cast("Any", mapping)["memory"] = 0
    with self.assertRaises(AttributeError):
      cast("Any", stat).descendants = 0

  def test_process_and_core_readers_retain_handle(self) -> None:
    parent = Cgroup.from_path(self.group.path)
    pids = parent.pids()
    core = parent.core()
    del parent
    self.check_optional("pids.current", pids.current, self.check_unsigned)
    self.assertIsInstance(core.stat(), CgroupStat)

  def test_nested_memory_readers_retain_handle(self) -> None:
    parent = Cgroup.from_path(self.group.path)
    memory = parent.memory()
    swap = memory.swap()
    zswap = memory.zswap()
    del parent, memory
    for name, read in (
      ("memory.swap.current", swap.current),
      ("memory.zswap.current", zswap.current),
    ):
      self.check_optional(name, read, lambda value: self.assertIsInstance(value, int))

  def test_numa_contract(self) -> None:
    def check_stat(stat: MemoryNumaStat) -> None:
      self.assertIsInstance(stat, MemoryNumaStat)
      self.assertIn("anon", stat.bytes)
      self.assertIn("file", stat.bytes)
      for values in (stat.bytes, stat.pages, stat.counts):
        with self.assertRaises(TypeError):
          cast("Any", values)["future"] = {}
        for field, nodes in values.items():
          with self.subTest(field=field):
            self.assertTrue(all(isinstance(node, int) for node in nodes))
            self.assertTrue(
              all(isinstance(value, int) and value >= 0 for value in nodes.values())
            )
            with self.assertRaises(TypeError):
              cast("Any", nodes)[0] = 0
      with self.assertRaises(AttributeError):
        cast("Any", stat).bytes = {}

    self.check_optional("memory.numa_stat", self.group.memory().numa_stat, check_stat)

  def test_non_utf8_child_lookup(self) -> None:
    class BytePath(os.PathLike[bytes]):
      def __fspath__(self) -> bytes:
        return b"sakai-nonexistent-\xff"

    for name in (
      b"sakai-nonexistent-\xff",
      os.fsdecode(b"sakai-nonexistent-\xff"),
      BytePath(),
    ):
      with self.subTest(name=name):
        with self.assertRaises(OSError) as caught:
          self.group.child(name)
        self.assertEqual(caught.exception.errno, errno.ENOENT)


if __name__ == "__main__":
  unittest.main()
