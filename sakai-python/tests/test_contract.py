"""Installed-package contract checks. Run on Linux from outside the checkout."""

import errno
import inspect
import os
import select
import sys
import tempfile
import threading
import unittest
from collections.abc import Callable, Mapping
from functools import partial
from pathlib import Path
from typing import TYPE_CHECKING, Any, TypeVar, assert_type, cast, get_origin

import sakai
import sakai._sakai as native
from sakai import (
  Bytes,
  BytesPerSecond,
  Cgroup,
  CgroupEvents,
  CgroupStat,
  ChangeNotification,
  ConfigurationWatcher,
  Count,
  CpuList,
  CpuMax,
  CpusetPartition,
  CpusetReader,
  CpuStat,
  EventPoller,
  InterfaceMissingError,
  IoDeviceMax,
  IoLatency,
  IoMax,
  IoReader,
  IoStat,
  IoWeight,
  MaxOr,
  MemoryEventCounts,
  MemoryNodeList,
  MemoryNumaStat,
  MemoryStat,
  NotCgroupV2Error,
  OperationsPerSecond,
  Pages,
  PidsEvents,
  PidsReader,
  Pressure,
  Ratio,
  SwapEvents,
  SwapReader,
  Time,
  ZswapReader,
)

if TYPE_CHECKING:

  def check_io_max_types(device: IoDeviceMax) -> None:
    assert_type(device.read_bandwidth, MaxOr[BytesPerSecond])
    assert_type(device.write_bandwidth, MaxOr[BytesPerSecond])
    assert_type(device.read_iops, MaxOr[OperationsPerSecond])
    assert_type(device.write_iops, MaxOr[OperationsPerSecond])


_Result = TypeVar("_Result")
_Quantity = Bytes | BytesPerSecond | Count | OperationsPerSecond | Pages | Ratio | Time


class ContractTest(unittest.TestCase):
  @classmethod
  def setUpClass(cls) -> None:
    cls.group = Cgroup.current()

  def check_optional(
    self,
    name: str,
    read: Callable[[], _Result],
    check: Callable[[_Result], None] | None = None,
    value_type: type[_Result] | None = None,
  ) -> None:
    with self.subTest(interface=name):
      try:
        value = read()
      except InterfaceMissingError as error:
        self.assertEqual(error.path, self.group.path / name)
      else:
        if value_type is not None:
          self.assertIsInstance(value, value_type)
        if check is not None:
          check(value)

  def check_optional_types(
    self, *cases: tuple[str, Callable[[], object], type[object]]
  ) -> None:
    for filename, read, expected_type in cases:
      self.check_optional(filename, read, value_type=expected_type)

  def check_frozen(self, value: object, *names: str) -> None:
    for name in names:
      with self.subTest(property=name), self.assertRaises(AttributeError):
        setattr(value, name, 0)

  def check_readonly(self, value: Mapping[Any, object], key: object) -> None:
    with self.assertRaises(TypeError):
      cast("Any", value)[key] = 0

  def pinned_reader(self, make: Callable[[Cgroup], _Result]) -> _Result:
    parent = Cgroup.from_path(self.group.path)
    return make(parent)

  def check_unsigned(
    self,
    value: _Quantity,
    value_type: type[int] | type[float] = int,
  ) -> None:
    magnitude: int | float
    match value:
      case Bytes():
        magnitude = value.bytes
      case BytesPerSecond():
        magnitude = value.bytes_per_second
      case Count():
        magnitude = value.count
      case OperationsPerSecond():
        magnitude = value.operations_per_second
      case Pages():
        magnitude = value.pages
      case Ratio():
        magnitude = value.fraction
      case Time():
        magnitude = value.nanoseconds
      case _:
        self.fail(f"Expected a native quantity, got {value!r}")
    self.assertIsInstance(magnitude, value_type)
    self.assertGreaterEqual(magnitude, 0)

  def check_integer(self, value: int) -> None:
    self.assertIsInstance(value, int)
    self.assertGreaterEqual(value, 0)

  def check_limit(
    self,
    limit: MaxOr[_Quantity],
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
        self.assertIsInstance(limit.value, Ratio)
        self.assertLessEqual(cast("Ratio", limit.value).fraction, maximum)

  def check_counters(
    self,
    value: object,
    value_type: type[_Result],
    names: str,
    optional: str = "",
  ) -> None:
    self.assertIsInstance(value, value_type)
    required, maybe = names.split(), optional.split()
    for name in required + maybe:
      count = getattr(value, name)
      if name in required or count is not None:
        self.check_unsigned(count)
    self.check_frozen(value, *required)

  def test_misc_and_device_memory(self) -> None:
    misc = self.pinned_reader(Cgroup.misc)
    dmem = self.pinned_reader(Cgroup.dmem)
    self.assertIsInstance(misc, sakai.MiscReader)
    self.assertIsInstance(dmem, sakai.DmemReader)
    self.check_optional_types(
      ("misc.capacity", misc.capacity, sakai.MiscCapacity),
      ("misc.current", misc.current, sakai.MiscCurrent),
      ("misc.peak", misc.peak, sakai.MiscPeak),
      ("misc.max", misc.max, sakai.MiscMax),
      ("misc.events", misc.events, sakai.MiscEvents),
      ("misc.events.local", misc.events_local, sakai.MiscEvents),
      ("dmem.capacity", dmem.capacity, sakai.DmemCapacity),
      ("dmem.current", dmem.current, sakai.DmemCurrent),
      ("dmem.peak", dmem.peak, sakai.DmemPeak),
      ("dmem.min", dmem.min, sakai.DmemLimits),
      ("dmem.low", dmem.low, sakai.DmemLimits),
      ("dmem.max", dmem.max, sakai.DmemLimits),
    )

  def test_rdma(self) -> None:
    rdma = self.pinned_reader(Cgroup.rdma)
    self.assertIsInstance(rdma, sakai.RdmaReader)
    self.check_optional_types(
      ("rdma.current", rdma.current, sakai.RdmaCurrent),
      ("rdma.peak", rdma.peak, sakai.RdmaCurrent),
      ("rdma.max", rdma.max, sakai.RdmaMax),
      ("rdma.events", rdma.events, sakai.RdmaEvents),
      ("rdma.events.local", rdma.events_local, sakai.RdmaEvents),
    )

  def test_hugetlb(self) -> None:
    huge = self.pinned_reader(Cgroup.hugetlb)
    self.assertIsInstance(huge, sakai.HugetlbReader)
    for size in ["2MB", "1GB"]:
      for suffix, read, result_type in (
        ("current", huge.current, Bytes),
        ("max", huge.max, MaxOr),
        ("rsvd.current", huge.reserved_current, Bytes),
        ("rsvd.max", huge.reserved_max, MaxOr),
        ("events", huge.events, sakai.HugetlbEvents),
        ("events.local", huge.events_local, sakai.HugetlbEvents),
        ("numa_stat", huge.numa_stat, sakai.HugetlbNumaStat),
      ):
        self.check_optional(
          f"hugetlb.{size}.{suffix}",
          partial(read, size),
          value_type=result_type,
        )
    for size in ["../2MB", "2mb", "2048KB", "0KB", "3MB"]:
      with self.subTest(page_size=size), self.assertRaises(ValueError):
        huge.current(size)

  def test_native_documentation(self) -> None:
    self.assertTrue(inspect.getdoc(native))
    # Verify one complete Rust-to-Python docstring; other wording may change.
    self.assertEqual(
      inspect.getdoc(Cgroup),
      "An open, pinned cgroup v2 directory handle.\n\n"
      "Readers retain this handle's ownership, so they remain usable after the\n"
      "Python Cgroup object is released. Each read fetches a fresh kernel value;\n"
      "separate calls do not form an atomic snapshot.",
    )
    package = Path(sakai.__file__).parent
    self.assertTrue((package / "py.typed").is_file())
    self.assertTrue((package / "_sakai.pyi").is_file())
    self.assertEqual(Path(native.__file__).parent, package)
    self.assertEqual(len(sakai.__all__), len(set(sakai.__all__)))
    self.assertEqual(
      {name for name in vars(native) if not name.startswith("_")},
      set(sakai.__all__),
    )
    for name in sakai.__all__:
      with self.subTest(class_name=name):
        cls = getattr(sakai, name)
        self.assertIs(cls, getattr(native, name))
        # Require each class's own docs; inherited Exception docs do not count.
        self.assertTrue((cls.__doc__ or "").strip())
        for member_name, member in vars(cls).items():
          if not member_name.startswith("_") and (
            callable(member) or inspect.isdatadescriptor(member)
          ):
            with self.subTest(member_name=member_name):
              self.assertTrue(inspect.getdoc(member))

  def test_pinned_handle_and_pathlike(self) -> None:
    self.assertIsInstance(self.group.path, Path)
    self.assertEqual(Cgroup.from_path(self.group.path).path, self.group.path)
    self.assertEqual(
      Cgroup.from_path(os.fsencode(self.group.path)).path, self.group.path
    )
    reader = self.pinned_reader(Cgroup.cpu)
    self.assertIsInstance(reader.stat(), CpuStat)
    self.assertIsInstance(self.group.children(), list)

  def test_pid_validation(self) -> None:
    for pid in (-1, 2**100):
      with self.subTest(pid=pid), self.assertRaises((ValueError, OverflowError)):
        Cgroup.from_pid(pid)
    for pid in (0, 2**31):
      with self.subTest(pid=pid), self.assertRaises(OSError):
        Cgroup.from_pid(pid)

  def test_disappeared_process_retains_errno_and_path(self) -> None:
    # Linux limits PIDs below this valid i32 value, so it cannot exist.
    with self.assertRaises(FileNotFoundError) as caught:
      Cgroup.from_pid(2**31 - 1)
    self.assertEqual(caught.exception.errno, errno.ENOENT)
    self.assertEqual(caught.exception.filename, Path("/proc/2147483647"))

  def test_missing_directory_retains_errno_and_filename(self) -> None:
    with tempfile.TemporaryDirectory() as directory:
      path = Path(directory) / "missing"
      with self.assertRaises(FileNotFoundError) as caught:
        Cgroup.from_path(path)
      self.assertEqual(caught.exception.errno, errno.ENOENT)
      self.assertEqual(caught.exception.filename, path)

  def test_invalid_path_and_child(self) -> None:
    with tempfile.TemporaryDirectory() as path, self.assertRaises(NotCgroupV2Error):
      Cgroup.from_path(path)
    for name in ("", ".", "..", "../other", "a/b"):
      with self.subTest(name=name), self.assertRaises(OSError):
        self.group.child(name)

  def test_notification_interface_validation(self) -> None:
    for create in (self.group.poll_events, self.group.watch_configuration):
      for invalid in ("", "../cpu.max", "/cpu.max", "cpu.pressure", "memory.current"):
        with (
          self.subTest(method=create.__name__, interface=invalid),
          self.assertRaises(ValueError),
        ):
          create(invalid)

  def test_event_monitors_and_timeouts(self) -> None:
    def check_monitor(monitor: EventPoller) -> None:
      self.assertIsInstance(monitor, EventPoller)
      self.check_integer(monitor.fileno())
      os.fstat(monitor.fileno())
      poll = select.poll()
      poll.register(monitor, select.POLLPRI | select.POLLERR)
      self.assertIsInstance(poll.poll(0), list)
      self.assertIsInstance(monitor.wait(0), bool)
      for invalid in (-1, float("nan"), float("inf"), float("-inf"), 1e19, 1e100):
        with self.subTest(timeout=invalid), self.assertRaises(ValueError):
          monitor.wait(invalid)

    for interface in (
      "cgroup.events",
      "memory.events",
      "memory.events.local",
      "memory.swap.events",
      "pids.events",
      "cpuset.cpus.partition",
    ):
      self.check_optional(
        interface,
        lambda interface=interface: self.group.poll_events(interface),
        check_monitor,
      )

  def test_configuration_monitors_and_timeouts(self) -> None:
    def check_monitor(monitor: ConfigurationWatcher) -> None:
      self.assertIsInstance(monitor, ConfigurationWatcher)
      self.check_integer(monitor.fileno())
      os.fstat(monitor.fileno())
      self.assertIsInstance(select.select([monitor], [], [], 0), tuple)
      notification = monitor.wait(0)
      if notification is not None:
        self.assertIsInstance(notification, ChangeNotification)
        for name in ("modified", "removed", "overflowed"):
          self.assertIsInstance(getattr(notification, name), bool)
          with self.assertRaises(AttributeError):
            setattr(notification, name, False)
      for invalid in (-1, float("nan"), float("inf"), float("-inf"), 1e19, 1e100):
        with self.subTest(timeout=invalid), self.assertRaises(ValueError):
          monitor.wait(invalid)

    for interface in (
      "cgroup.type",
      "cgroup.subtree_control",
      "cpu.max",
      "cpu.weight",
      "cpu.uclamp.max",
      "cpu.uclamp.min",
      "cpu.max.burst",
      "cpu.weight.nice",
      "cpu.idle",
      "memory.max",
      "memory.high",
      "memory.low",
      "memory.min",
      "memory.oom.group",
      "memory.swap.max",
      "memory.swap.high",
      "memory.zswap.max",
      "memory.zswap.writeback",
      "pids.max",
      "io.weight",
      "io.max",
      "io.latency",
      "cpuset.cpus",
      "cpuset.mems",
      "cpuset.cpus.partition",
    ):
      self.check_optional(
        interface,
        lambda interface=interface: self.group.watch_configuration(interface),
        check_monitor,
      )

  def test_notification_descriptor_ownership(self) -> None:
    for method, interface in (
      ("poll_events", "cgroup.events"),
      ("watch_configuration", "cgroup.type"),
    ):
      with self.subTest(method=method):
        parent = Cgroup.from_path(self.group.path)
        try:
          monitor = getattr(parent, method)(interface)
        except InterfaceMissingError as error:
          self.assertEqual(error.path, self.group.path / interface)
          continue
        descriptor = monitor.fileno()
        del parent
        os.fstat(descriptor)
        monitor.wait(0)
        del monitor
        with self.assertRaises(OSError) as caught:
          os.fstat(descriptor)
        self.assertEqual(caught.exception.errno, errno.EBADF)

  def test_configuration_wait_releases_gil(self) -> None:
    try:
      monitor = self.group.watch_configuration("cgroup.type")
    except InterfaceMissingError:
      self.skipTest("cgroup.type is unavailable")
    started = threading.Event()
    progress = 0

    def worker() -> None:
      nonlocal progress
      started.wait()
      for _ in range(10_000):
        progress += 1

    thread = threading.Thread(target=worker)
    thread.start()
    previous_interval = sys.getswitchinterval()
    try:
      # Prevent an interpreter switch after a GIL-holding wait returns from
      # making the worker appear to have progressed during the wait.
      sys.setswitchinterval(1.0)
      started.set()
      before = progress
      result = monitor.wait(0.1)
      after = progress
      self.assertIsNone(result)
      self.assertGreater(after, before)
    finally:
      sys.setswitchinterval(previous_interval)
      started.set()
      thread.join()

  def test_cpu_contract(self) -> None:
    def check_uclamp(value: Ratio) -> None:
      self.check_unsigned(value, float)
      self.assertLessEqual(value.fraction, 1)

    cpu = self.group.cpu()
    stat = cpu.stat()
    self.check_unsigned(stat.time.usage)
    self.check_frozen(stat.time, "usage")
    weight = cpu.weight()
    self.assertEqual(weight.is_idle, weight.weight is None)
    if weight.weight is not None:
      self.check_integer(weight.weight)
      self.assertTrue(1 <= weight.weight <= 10_000)
    self.assertTrue(-20 <= cpu.weight_nice() <= 19)
    self.check_unsigned(cpu.max_burst())
    self.assertIsInstance(cpu.idle(), bool)
    self.check_optional("cpu.uclamp.min", cpu.uclamp_min, check_uclamp)
    self.check_optional(
      "cpu.uclamp.max", cpu.uclamp_max, lambda value: self.check_limit(value, float, 1)
    )
    self.assertIsInstance(cpu.pressure(), Pressure)
    self.check_optional(
      "cpu.stat.local",
      cpu.stat_local,
      lambda value: self.check_counters(value, sakai.CpuStatLocal, "", "throttled"),
    )

  def test_cpu_max_contract(self) -> None:
    maximum = self.group.cpu().max()
    self.assertIsInstance(maximum, CpuMax)
    self.check_unsigned(maximum.period)
    self.assertGreater(maximum.period.nanoseconds, 0)
    self.assertIs(get_origin(MaxOr[Time]), MaxOr)
    self.check_limit(assert_type(maximum.quota, MaxOr[Time]))
    self.check_limit(assert_type(maximum.cpu_count, MaxOr[Ratio]), float)
    self.assertEqual(maximum.quota.is_max, maximum.cpu_count.is_max)
    if not maximum.quota.is_max:
      self.assertAlmostEqual(
        maximum.cpu_count.value.fraction,
        maximum.quota.value.nanoseconds / maximum.period.nanoseconds,
      )

  def test_memory_contract(self) -> None:
    memory = self.group.memory()
    for read in (memory.current, memory.peak):
      self.check_unsigned(read())
    for read in (memory.max, memory.high, memory.low, memory.min):
      self.check_limit(read())
    stat = memory.stat()
    self.assertIsInstance(stat, MemoryStat)
    self.assertIn("anon", stat.bytes)
    self.assertIn("file", stat.bytes)
    self.check_readonly(stat.bytes, "anon")
    for values in (stat.bytes, stat.pages, stat.counts):
      for value in values.values():
        self.check_unsigned(value)
    self.assertIsInstance(memory.pressure(), Pressure)
    self.check_optional("memory.numa_stat", memory.numa_stat, value_type=MemoryNumaStat)

  def test_io_optional_readers(self) -> None:
    reader = self.group.io()
    self.check_optional_types(
      ("io.stat", reader.stat, IoStat),
      ("io.weight", reader.weight, IoWeight),
      ("io.max", reader.max, IoMax),
      ("io.latency", reader.latency, IoLatency),
      ("io.cost.qos", reader.cost_qos, sakai.IoCostQos),
      ("io.cost.model", reader.cost_model, sakai.IoCostModel),
    )
    self.check_optional(
      "io.prio.class",
      reader.priority_class,
      lambda value: self.assertIn(
        value, {"no-change", "promote-to-rt", "none-to-rt", "restrict-to-be", "idle"}
      ),
    )

  def test_io_pressure_contract(self) -> None:
    def check_pressure(pressure: Pressure) -> None:
      self.assertIsInstance(pressure, Pressure)
      for line in (pressure.some, pressure.full):
        if line is not None:
          for average in (line.avg10, line.avg60, line.avg300):
            self.check_unsigned(average, float)
            self.assertLessEqual(average.fraction, 1)
          self.check_unsigned(line.total)

    self.check_optional("io.pressure", self.group.io().pressure, check_pressure)

  def test_io_reader_retains_handle(self) -> None:
    reader = self.pinned_reader(Cgroup.io)
    self.assertIsInstance(reader, IoReader)
    self.check_optional("io.stat", reader.stat, value_type=IoStat)

  def test_cpuset_lists_and_reader_ownership(self) -> None:
    reader = self.pinned_reader(Cgroup.cpuset)
    self.assertIsInstance(reader, CpusetReader)

    def check_list(value: CpuList | MemoryNodeList) -> None:
      self.assertIsInstance(value.ranges, tuple)
      previous = -1
      for pair in value.ranges:
        self.assertIsInstance(pair, tuple)
        self.assertEqual(len(pair), 2)
        start, end = pair
        self.check_integer(start)
        self.check_integer(end)
        self.assertLessEqual(start, end)
        self.assertLessEqual(end, 2**32 - 1)
        self.assertGreater(start, previous)
        self.assertTrue(value.contains(start))
        self.assertTrue(value.contains(end))
        previous = end
      if not value.ranges:
        self.assertFalse(value.contains(0))
      self.check_frozen(value, "ranges")
      with self.assertRaises(TypeError):
        cast("Any", value.ranges)[0] = (0, 0)
      for invalid in (-1, 2**32):
        with self.subTest(id=invalid), self.assertRaises(OverflowError):
          value.contains(invalid)

    for name, read, value_type in (
      ("cpuset.cpus", reader.cpus, CpuList),
      ("cpuset.cpus.effective", reader.cpus_effective, CpuList),
      ("cpuset.mems", reader.mems, MemoryNodeList),
      ("cpuset.mems.effective", reader.mems_effective, MemoryNodeList),
      ("cpuset.cpus.exclusive", reader.cpus_exclusive, CpuList),
      ("cpuset.cpus.exclusive.effective", reader.cpus_exclusive_effective, CpuList),
      ("cpuset.cpus.isolated", reader.cpus_isolated, CpuList),
    ):
      self.check_optional(name, read, check_list, value_type)

  def test_cpuset_partition_contract(self) -> None:
    def check_partition(value: CpusetPartition) -> None:
      self.assertIsInstance(value, CpusetPartition)
      self.assertIn(value.state, {"member", "root", "isolated"})
      self.assertIsInstance(value.is_invalid, bool)
      if value.reason is not None:
        self.assertIsInstance(value.reason, str)
        self.assertTrue(value.reason)
        self.assertTrue(value.is_invalid)
      if value.state == "member":
        self.assertFalse(value.is_invalid)
      self.check_frozen(value, "state", "is_invalid", "reason")

    self.check_optional(
      "cpuset.cpus.partition", self.group.cpuset().partition, check_partition
    )

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
    core = self.group.core()
    self.check_optional_types(
      ("cgroup.freeze", core.freeze, bool),
      ("cgroup.pressure", core.pressure_enabled, bool),
    )
    for filename, read in (
      ("cgroup.max.depth", core.max_depth),
      ("cgroup.max.descendants", core.max_descendants),
    ):
      self.check_optional(filename, read, self.check_limit)
    self.check_optional(
      "cgroup.stat.local",
      core.stat_local,
      lambda value: self.check_unsigned(value.frozen_time),
      sakai.CgroupStatLocal,
    )
    self.check_optional(
      "irq.pressure",
      self.group.irq_pressure,
      lambda value: self.check_unsigned(value.full.total),
      sakai.IrqPressure,
    )

  def test_memory_events_and_oom_policy(self) -> None:
    memory = self.group.memory()

    for name, read in (
      ("memory.events", memory.events),
      ("memory.events.local", memory.events_local),
    ):
      self.check_optional(
        name,
        read,
        lambda value: self.check_counters(
          value,
          MemoryEventCounts,
          "low high max oom oom_kill",
          "oom_group_kill sock_throttled",
        ),
      )
    self.check_optional(
      "memory.oom.group",
      memory.oom_group,
      lambda value: self.assertIsInstance(value, bool),
    )

  def test_swap_contract(self) -> None:
    swap = self.group.memory().swap()
    self.assertIsInstance(swap, SwapReader)
    for name, read in (("current", swap.current), ("peak", swap.peak)):
      self.check_optional(f"memory.swap.{name}", read, self.check_unsigned)
    for name, read in (("max", swap.max), ("high", swap.high)):
      self.check_optional(f"memory.swap.{name}", read, self.check_limit)

    self.check_optional(
      "memory.swap.events",
      swap.events,
      lambda value: self.check_counters(value, SwapEvents, "max fail", "high"),
    )

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
    self.check_optional("pids.peak", pids.peak, self.check_unsigned)
    self.check_optional("pids.max", pids.max, self.check_limit)

    for name, read in (("events", pids.events), ("events.local", pids.events_local)):
      self.check_optional(
        f"pids.{name}",
        read,
        lambda value: self.check_counters(value, PidsEvents, "max"),
      )

  def test_cgroup_state_contract(self) -> None:
    def check_events(events: CgroupEvents) -> None:
      self.assertIsInstance(events, CgroupEvents)
      self.assertIsInstance(events.populated, bool)
      if events.frozen is not None:
        self.assertIsInstance(events.frozen, bool)
      self.check_frozen(events, "populated")

    self.check_optional("cgroup.events", self.group.core().events, check_events)
    stat = self.group.core().stat()
    self.check_counters(stat, CgroupStat, "descendants dying_descendants")
    for mapping in (stat.subsystems, stat.dying_subsystems):
      self.assertTrue(all(isinstance(key, str) for key in mapping))
      for value in mapping.values():
        self.check_unsigned(value)
      self.check_readonly(mapping, "memory")

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
      self.check_optional(name, read, self.check_unsigned, Bytes)

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
        with self.assertRaises(FileNotFoundError) as caught:
          self.group.child(name)
        self.assertEqual(caught.exception.errno, errno.ENOENT)
        self.assertEqual(
          os.fsencode(caught.exception.filename),
          os.fsencode(self.group.path) + b"/sakai-nonexistent-\xff",
        )


if __name__ == "__main__":
  unittest.main()
