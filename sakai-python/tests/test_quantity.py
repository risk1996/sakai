"""Installed-package checks for native quantities and conversion accessors."""

import unittest
from typing import assert_type

from sakai import Bytes, Cgroup, Count, InterfaceMissingError, MaxOr, Pages, Time


class QuantityTest(unittest.TestCase):
  def test_duration_and_memory_conversions(self) -> None:
    group = Cgroup.current()
    usage = assert_type(group.cpu().stat().time.usage, Time)
    self.assertIsInstance(usage, Time)
    self.assertIsInstance(usage.nanoseconds, int)
    self.assertEqual(usage.seconds, usage.nanoseconds / 1_000_000_000)
    self.assertRaises(AttributeError, setattr, usage, "nanoseconds", 0)

    try:
      memory = assert_type(group.memory().current(), Bytes)
    except InterfaceMissingError as error:
      self.skipTest(str(error))
    self.assertIsInstance(memory, Bytes)
    self.assertIsInstance(memory.bytes, int)
    self.assertEqual(memory.bits, memory.bytes * 8)
    self.assertEqual(memory.mebibytes, memory.bytes / 2**20)

  def test_mapping_and_limit_keep_their_quantity_types(self) -> None:
    group = Cgroup.current()
    try:
      stat = group.memory().stat()
      limit = assert_type(group.memory().max(), MaxOr[Bytes])
    except InterfaceMissingError as error:
      self.skipTest(str(error))

    self.assertIsInstance(assert_type(stat.bytes["anon"], Bytes), Bytes)
    for count in stat.counts.values():
      self.assertIsInstance(assert_type(count, Count), Count)
    for pages in stat.pages.values():
      self.assertIsInstance(assert_type(pages, Pages), Pages)
      self.assertFalse(hasattr(pages, "bytes"))
    if not limit.is_max:
      self.assertIsInstance(limit.value, Bytes)


if __name__ == "__main__":
  unittest.main()
