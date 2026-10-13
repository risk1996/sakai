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
from pathlib import Path
from typing import Any, TypeVar, assert_type, cast, get_origin

import pint

import sakai
import sakai._sakai as native
from sakai import (
  Cgroup,
  CgroupEvents,
  CgroupStat,
  ChangeNotification,
  ConfigurationWatcher,
  CpuList,
  CpuMax,
  CpusetPartition,
  CpusetReader,
  CpuStat,
  EventPoller,
  InterfaceMissingError,
  IoDeviceMax,
  IoDeviceStat,
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
  PidsEvents,
  PidsReader,
  Pressure,
  SwapEvents,
  SwapReader,
  ZswapReader,
)

_Result = TypeVar("_Result")
_Key = TypeVar("_Key")


class ContractTest(unittest.TestCase):
  @classmethod
  def setUpClass(cls) -> None:
    cls.group = Cgroup.current()

  def check_optional(
    self,
    name: str,
    read: Callable[[], _Result],
    check: Callable[[_Result], None],
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
        check(value)

  def check_frozen(self, value: object, *names: str) -> None:
    for name in names:
      with self.subTest(property=name), self.assertRaises(AttributeError):
        setattr(value, name, 0)

  def check_readonly(self, value: Mapping[_Key, object], key: _Key) -> None:
    with self.assertRaises(TypeError):
      cast("Any", value)[key] = 0

  def pinned_reader(self, make: Callable[[Cgroup], _Result]) -> _Result:
    parent = Cgroup.from_path(self.group.path)
    return make(parent)

  def check_unsigned(
    self,
    value: pint.Quantity[int] | pint.Quantity[float],
    value_type: type[int] | type[float] = int,
  ) -> None:
    self.assertIsInstance(value, pint.Quantity)
    self.assertIsInstance(value.magnitude, value_type)
    self.assertGreaterEqual(value.magnitude, 0)

  def check_integer(self, value: int) -> None:
    self.assertIsInstance(value, int)
    self.assertGreaterEqual(value, 0)

  def check_limit(
    self,
    limit: MaxOr[pint.Quantity[int]] | MaxOr[pint.Quantity[float]],
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
    def check_values(
      snapshot: object, attribute: str, check: Callable[[Any], None]
    ) -> None:
      values = getattr(snapshot, attribute)
      self.assertIsInstance(values, Mapping)
      self.check_frozen(snapshot, attribute)
      self.check_readonly(values, "unused")
      for name, value in values.items():
        self.assertIsInstance(name, str)
        check(value)

    misc = self.pinned_reader(Cgroup.misc)
    self.assertIsInstance(misc, sakai.MiscReader)
    for method, result_type in [
      ("capacity", sakai.MiscCapacity),
      ("current", sakai.MiscCurrent),
      ("peak", sakai.MiscPeak),
      ("events", sakai.MiscEvents),
      ("events_local", sakai.MiscEvents),
    ]:
      self.check_optional(
        "misc." + method.replace("_", "."),
        getattr(misc, method),
        lambda value: check_values(value, "resources", self.check_unsigned),
        result_type,
      )
    self.check_optional(
      "misc.max",
      misc.max,
      lambda value: check_values(value, "resources", self.check_limit),
      sakai.MiscMax,
    )
    dmem = self.pinned_reader(Cgroup.dmem)
    self.assertIsInstance(dmem, sakai.DmemReader)
    self.check_optional(
      "dmem.capacity",
      dmem.capacity,
      lambda value: check_values(value, "regions", self.check_unsigned),
      sakai.DmemCapacity,
    )
    for method in ["current", "peak", "min", "low", "max"]:
      self.check_optional(
        "dmem." + method,
        getattr(dmem, method),
        lambda value: check_values(value, "regions", self.check_limit),
        {"current": sakai.DmemCurrent, "peak": sakai.DmemPeak}.get(
          method, sakai.DmemLimits
        ),
      )

  def test_rdma(self) -> None:
    rdma = self.pinned_reader(Cgroup.rdma)
    self.assertIsInstance(rdma, sakai.RdmaReader)

    def check_rdma(snapshot: Any, kind: str) -> None:  # noqa: ANN401
      self.check_frozen(snapshot, "devices")
      self.check_readonly(snapshot.devices, ("unused", None))
      for key, device in snapshot.devices.items():
        self.assertIsInstance(key, tuple)
        self.assertEqual(len(key), 2)
        name, index = key
        self.assertIsInstance(name, str)
        if index is not None:
          self.check_integer(index)
          self.assertLessEqual(index, 2**32 - 1)
        self.check_frozen(device, "hca_handle", "hca_object")
        for value in [device.hca_handle, device.hca_object]:
          if kind == "max":
            self.check_limit(value)
          elif kind == "events":
            self.check_counters(value, sakai.RdmaResourceEvents, "max alloc_fail")
          else:
            self.check_unsigned(value)

    for method, result_type, kind in [
      ("current", sakai.RdmaCurrent, "current"),
      ("peak", sakai.RdmaCurrent, "current"),
      ("max", sakai.RdmaMax, "max"),
      ("events", sakai.RdmaEvents, "events"),
      ("events_local", sakai.RdmaEvents, "events"),
    ]:
      self.check_optional(
        "rdma." + method.replace("_", "."),
        getattr(rdma, method),
        lambda value, kind=kind: check_rdma(value, kind),
        result_type,
      )

  def test_hugetlb(self) -> None:
    huge = self.pinned_reader(Cgroup.hugetlb)
    self.assertIsInstance(huge, sakai.HugetlbReader)
    for size in ["2MB", "1GB"]:
      for suffix, read, check, result_type in [
        ("current", huge.current, self.check_unsigned, pint.Quantity),
        ("max", huge.max, self.check_limit, MaxOr),
        ("rsvd.current", huge.reserved_current, self.check_unsigned, pint.Quantity),
        ("rsvd.max", huge.reserved_max, self.check_limit, MaxOr),
        (
          "events",
          huge.events,
          lambda value: self.check_counters(value, sakai.HugetlbEvents, "max"),
          sakai.HugetlbEvents,
        ),
        (
          "events.local",
          huge.events_local,
          lambda value: self.check_counters(value, sakai.HugetlbEvents, "max"),
          sakai.HugetlbEvents,
        ),
        (
          "numa_stat",
          huge.numa_stat,
          lambda value: self.check_unsigned(value.total),
          sakai.HugetlbNumaStat,
        ),
      ]:
        self.check_optional(
          f"hugetlb.{size}.{suffix}",
          lambda read=read, size=size: read(size),
          cast("Callable[[Any], None]", check),
          result_type,
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
    def check_uclamp(value: pint.Quantity[float]) -> None:
      self.check_unsigned(value, float)
      self.assertLessEqual(value, 1)

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
    self.assertGreater(maximum.period, 0)
    self.assertIs(get_origin(MaxOr[pint.Quantity[int]]), MaxOr)
    self.check_limit(assert_type(maximum.quota, MaxOr[pint.Quantity[int]]))
    self.check_limit(assert_type(maximum.cpu_count, MaxOr[pint.Quantity[float]]), float)
    self.assertEqual(maximum.quota.is_max, maximum.cpu_count.is_max)
    if not maximum.quota.is_max:
      self.assertAlmostEqual(
        maximum.cpu_count.value.magnitude,
        (maximum.quota.value / maximum.period).to("dimensionless").magnitude,
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

  def check_device_key(self, key: tuple[int, int]) -> None:
    self.assertIsInstance(key, tuple)
    self.assertEqual(len(key), 2)
    for number in key:
      self.check_integer(number)

  def test_io_stat_contract(self) -> None:
    def check_stat(stat: IoStat) -> None:
      self.assertIsInstance(stat, IoStat)
      for key, device in stat.devices.items():
        self.check_device_key(key)
        self.assertIsInstance(device, IoDeviceStat)
        for name in (
          "read_bytes",
          "write_bytes",
          "discard_bytes",
          "read_operations",
          "write_operations",
          "discard_operations",
        ):
          value = getattr(device, name)
          if value is not None:
            self.check_unsigned(value)
          self.check_frozen(device, name)
      self.check_readonly(stat.devices, (8, 0))
      self.check_frozen(stat, "devices")

    self.check_optional("io.stat", self.group.io().stat, check_stat)

  def test_io_weight_contract(self) -> None:
    def check_weight(weight: IoWeight) -> None:
      self.assertIsInstance(weight, IoWeight)
      self.check_integer(weight.default_weight)
      self.assertTrue(1 <= weight.default_weight <= 10_000)
      for key, value in weight.devices.items():
        self.check_device_key(key)
        self.check_integer(value)
        self.assertTrue(1 <= value <= 10_000)
      self.check_readonly(weight.devices, (8, 0))
      self.check_frozen(weight, "default_weight")

    self.check_optional("io.weight", self.group.io().weight, check_weight)

  def test_io_max_contract(self) -> None:
    def check_max(maximum: IoMax) -> None:
      self.assertIsInstance(maximum, IoMax)
      for key, device in maximum.devices.items():
        self.check_device_key(key)
        self.assertIsInstance(device, IoDeviceMax)
        for limit in (
          device.read_bandwidth,
          device.write_bandwidth,
          device.read_iops,
          device.write_iops,
        ):
          self.check_limit(limit)
        self.check_frozen(device, "read_bandwidth")
      self.check_readonly(maximum.devices, (8, 0))
      self.check_frozen(maximum, "devices")

    self.check_optional("io.max", self.group.io().max, check_max)

  def test_io_latency_contract(self) -> None:
    def check_latency(latency: IoLatency) -> None:
      self.assertIsInstance(latency, IoLatency)
      for key, value in latency.targets.items():
        self.check_device_key(key)
        self.check_unsigned(value)
        self.assertGreater(value, 0)
        self.assertEqual(value.to("nanosecond").magnitude % 1000, 0)
      self.check_readonly(latency.targets, (8, 0))
      self.check_frozen(latency, "targets")

    self.check_optional("io.latency", self.group.io().latency, check_latency)

  def test_io_cost_and_priority_contract(self) -> None:
    def check_qos(value: sakai.IoCostQos) -> None:
      self.check_readonly(value.devices, (8, 0))
      for key, device in value.devices.items():
        self.check_device_key(key)
        self.assertIsInstance(device.enabled, bool)
        self.assertIn(device.control, {"auto", "user"})
        for percentile in (device.read_percentile, device.write_percentile):
          self.check_unsigned(percentile, float)
          self.assertTrue(0 <= percentile <= 1)
        for scaling in (device.min_scaling, device.max_scaling):
          self.check_unsigned(scaling, float)
        for latency in (device.read_latency, device.write_latency):
          self.check_unsigned(latency)
          self.assertEqual(latency.to("nanosecond").magnitude % 1000, 0)
        self.check_frozen(device, "enabled", "control")

    def check_model(value: sakai.IoCostModel) -> None:
      self.check_readonly(value.devices, (8, 0))
      for key, device in value.devices.items():
        self.check_device_key(key)
        self.assertIn(device.control, {"auto", "user"})
        self.assertEqual(device.model, "linear")
        for name in (
          "read_bandwidth",
          "write_bandwidth",
          "read_sequential_iops",
          "read_random_iops",
          "write_sequential_iops",
          "write_random_iops",
        ):
          self.check_unsigned(getattr(device, name))
        self.check_frozen(device, "control", "read_bandwidth")

    reader = self.group.io()
    self.check_optional("io.cost.qos", reader.cost_qos, check_qos)
    self.check_optional("io.cost.model", reader.cost_model, check_model)
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
            self.assertLessEqual(average, 1)
          self.check_unsigned(line.total)

    self.check_optional("io.pressure", self.group.io().pressure, check_pressure)

  def test_io_reader_retains_handle(self) -> None:
    reader = self.pinned_reader(Cgroup.io)
    self.assertIsInstance(reader, IoReader)
    self.check_optional(
      "io.stat", reader.stat, lambda value: self.assertIsInstance(value, IoStat)
    )

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
    for filename, read in (
      ("cgroup.freeze", core.freeze),
      ("cgroup.pressure", core.pressure_enabled),
    ):
      self.check_optional(
        filename, read, lambda value: self.assertIsInstance(value, bool)
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
      self.check_optional(
        name, read, lambda value: self.assertIsInstance(value, pint.Quantity)
      )

  def test_numa_contract(self) -> None:
    def check_stat(stat: MemoryNumaStat) -> None:
      self.assertIsInstance(stat, MemoryNumaStat)
      self.assertIn("anon", stat.bytes)
      self.assertIn("file", stat.bytes)
      for values in (stat.bytes, stat.pages, stat.counts):
        self.check_readonly(values, "future")
        for field, nodes in values.items():
          with self.subTest(field=field):
            self.assertTrue(all(isinstance(node, int) for node in nodes))
            self.assertTrue(
              all(
                isinstance(value, pint.Quantity) and value.magnitude >= 0
                for value in nodes.values()
              )
            )
            self.check_readonly(nodes, 0)
      self.check_frozen(stat, "bytes")

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
        with self.assertRaises(FileNotFoundError) as caught:
          self.group.child(name)
        self.assertEqual(caught.exception.errno, errno.ENOENT)
        self.assertEqual(
          os.fsencode(caught.exception.filename),
          os.fsencode(self.group.path) + b"/sakai-nonexistent-\xff",
        )


if __name__ == "__main__":
  unittest.main()
