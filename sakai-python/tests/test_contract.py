"""Installed-package contract checks. Run on Linux from outside the checkout."""

import errno
import inspect
import os
import tempfile
import unittest
from pathlib import Path
from typing import Any, cast, get_origin

import sakai
import sakai._sakai as native
from sakai import (
  Cgroup,
  CpuMax,
  CpuStat,
  InterfaceMissingError,
  MaxOr,
  MemoryNumaStat,
  MemoryStat,
  NotCgroupV2Error,
  Pressure,
  SwapEvents,
  SwapReader,
  ZswapReader,
)


class ContractTest(unittest.TestCase):
  @classmethod
  def setUpClass(cls) -> None:
    cls.group = Cgroup.current()

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
    public_members = {
      "Cgroup": (
        "current",
        "from_pid",
        "from_path",
        "path",
        "child",
        "children",
        "cpu",
        "memory",
        "core",
      ),
      "CpuReader": (
        "stat",
        "stat_local",
        "max",
        "weight",
        "weight_nice",
        "max_burst",
        "idle",
        "uclamp_min",
        "uclamp_max",
        "pressure",
      ),
      "CpuStat": ("time", "bandwidth"),
      "CpuStatLocal": ("throttled_ns",),
      "CpuTimeStat": ("usage_ns", "user_ns", "system_ns"),
      "CpuBandwidthStat": ("nr_periods", "nr_throttled", "throttled_ns", "burst"),
      "CpuBurstStat": ("nr_bursts", "burst_ns"),
      "CpuMax": ("quota_ns", "period_ns", "cpu_count"),
      "MaxOr": ("is_max", "value"),
      "CpuWeight": ("is_idle", "shares"),
      "Pressure": ("some", "full"),
      "PressureLine": ("avg10", "avg60", "avg300", "total_ns"),
      "MemoryReader": (
        "swap",
        "zswap",
        "current",
        "peak",
        "max",
        "high",
        "low",
        "min",
        "stat",
        "numa_stat",
        "pressure",
      ),
      "MemoryStat": ("bytes", "pages", "counts"),
      "SwapReader": ("current", "peak", "max", "high", "events"),
      "SwapEvents": ("high", "max", "fail"),
      "ZswapReader": ("current", "max", "writeback"),
      "MemoryNumaStat": ("bytes", "pages", "counts"),
      "CoreReader": ("kind", "controllers", "subtree_control"),
    }
    for class_name, members in public_members.items():
      with self.subTest(class_name=class_name):
        cls = getattr(sakai, class_name)
        self.assertTrue(inspect.getdoc(cls))
        for member_name in members:
          with self.subTest(class_name=class_name, member_name=member_name):
            self.assertTrue(inspect.getdoc(getattr(cls, member_name)))

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
    self.assertIsInstance(stat.time.usage_ns, int)
    self.assertGreaterEqual(stat.time.usage_ns, 0)
    with self.assertRaises(AttributeError):
      cast("Any", stat.time).usage_ns = 5
    weight = cpu.weight()
    self.assertEqual(weight.is_idle, weight.shares is None)
    if weight.shares is not None:
      self.assertTrue(1 <= weight.shares <= 10_000)
    self.assertTrue(-20 <= cpu.weight_nice() <= 19)
    self.assertGreaterEqual(cpu.max_burst(), 0)
    self.assertIsInstance(cpu.idle(), bool)
    try:
      uclamp_min = cpu.uclamp_min()
    except InterfaceMissingError as error:
      self.assertEqual(error.path.name, "cpu.uclamp.min")
    else:
      self.assertTrue(0 <= uclamp_min <= 1)
    try:
      uclamp_max = cpu.uclamp_max()
    except InterfaceMissingError as error:
      self.assertEqual(error.path.name, "cpu.uclamp.max")
    else:
      self.assertIsInstance(uclamp_max, MaxOr)
      if uclamp_max.is_max:
        with self.assertRaises(ValueError):
          _ = uclamp_max.value
      else:
        self.assertIsInstance(uclamp_max.value, float)
        self.assertTrue(0 <= uclamp_max.value <= 1)
    self.assertIsInstance(cpu.pressure(), Pressure)
    try:
      local = cpu.stat_local()
    except InterfaceMissingError as error:
      self.assertEqual(error.path.name, "cpu.stat.local")
    else:
      self.assertTrue(local.throttled_ns is None or local.throttled_ns >= 0)

  def test_cpu_max_contract(self) -> None:
    maximum = self.group.cpu().max()
    self.assertIsInstance(maximum, CpuMax)
    self.assertGreater(maximum.period_ns, 0)
    self.assertIs(get_origin(MaxOr[int]), MaxOr)
    self.assertIsInstance(maximum.quota_ns, MaxOr)
    self.assertIsInstance(maximum.cpu_count, MaxOr)
    self.assertEqual(maximum.quota_ns.is_max, maximum.cpu_count.is_max)
    if maximum.quota_ns.is_max:
      with self.assertRaises(ValueError):
        _ = maximum.quota_ns.value
      with self.assertRaises(ValueError):
        _ = maximum.cpu_count.value
    else:
      self.assertIsInstance(maximum.quota_ns.value, int)
      self.assertIsInstance(maximum.cpu_count.value, float)
      self.assertAlmostEqual(
        maximum.cpu_count.value,
        maximum.quota_ns.value / maximum.period_ns,
      )

  def test_memory_contract(self) -> None:
    memory = self.group.memory()
    for read in (memory.current, memory.peak, memory.low, memory.min):
      self.assertGreaterEqual(read(), 0)
    for read in (memory.max, memory.high):
      limit = read()
      self.assertIsInstance(limit, MaxOr)
      if limit.is_max:
        with self.assertRaises(ValueError):
          _ = limit.value
      else:
        self.assertIsInstance(limit.value, int)
        self.assertGreaterEqual(limit.value, 0)
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
      with self.subTest(name=name):
        try:
          value = read()
        except InterfaceMissingError as error:
          self.assertEqual(error.path, self.group.path / f"memory.swap.{name}")
        else:
          self.assertIsInstance(value, int)
          self.assertGreaterEqual(value, 0)
    for name, read in (("max", swap.max), ("high", swap.high)):
      with self.subTest(name=name):
        try:
          limit = read()
        except InterfaceMissingError as error:
          self.assertEqual(error.path, self.group.path / f"memory.swap.{name}")
        else:
          self.assertIsInstance(limit, MaxOr)
          if limit.is_max:
            with self.assertRaises(ValueError):
              _ = limit.value
          else:
            self.assertIsInstance(limit.value, int)
            self.assertGreaterEqual(limit.value, 0)
    try:
      events = swap.events()
    except InterfaceMissingError as error:
      self.assertEqual(error.path, self.group.path / "memory.swap.events")
    else:
      self.assertIsInstance(events, SwapEvents)
      self.assertTrue(events.high is None or events.high >= 0)
      self.assertIsInstance(events.max, int)
      self.assertIsInstance(events.fail, int)
      self.assertGreaterEqual(events.max, 0)
      self.assertGreaterEqual(events.fail, 0)
      with self.assertRaises(AttributeError):
        cast("Any", events).fail = 0

  def test_zswap_contract(self) -> None:
    zswap = self.group.memory().zswap()
    self.assertIsInstance(zswap, ZswapReader)
    try:
      usage = zswap.current()
    except InterfaceMissingError as error:
      self.assertEqual(error.path, self.group.path / "memory.zswap.current")
    else:
      self.assertIsInstance(usage, int)
      self.assertGreaterEqual(usage, 0)
    try:
      limit = zswap.max()
    except InterfaceMissingError as error:
      self.assertEqual(error.path, self.group.path / "memory.zswap.max")
    else:
      self.assertIsInstance(limit, MaxOr)
      if limit.is_max:
        with self.assertRaises(ValueError):
          _ = limit.value
      else:
        self.assertIsInstance(limit.value, int)
        self.assertGreaterEqual(limit.value, 0)
    try:
      writeback = zswap.writeback()
    except InterfaceMissingError as error:
      self.assertEqual(error.path, self.group.path / "memory.zswap.writeback")
    else:
      self.assertIsInstance(writeback, bool)

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
      with self.subTest(name=name):
        try:
          value = read()
        except InterfaceMissingError as error:
          self.assertEqual(error.path, self.group.path / name)
        else:
          self.assertIsInstance(value, int)

  def test_numa_contract(self) -> None:
    try:
      stat = self.group.memory().numa_stat()
    except InterfaceMissingError as error:
      self.assertEqual(error.path, self.group.path / "memory.numa_stat")
      return
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
