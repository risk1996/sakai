"""Pint interoperability checks against the installed native package."""

import pickle
import unittest

import pint

from sakai import Cgroup


class PintTest(unittest.TestCase):
  @classmethod
  def setUpClass(cls) -> None:
    cls.group = Cgroup.current()

  def test_quantities_share_the_application_registry(self) -> None:
    previous = pint.get_application_registry().get()
    registry = pint.UnitRegistry()
    try:
      pint.set_application_registry(registry)
      time = self.group.cpu().stat().time.usage
      memory = self.group.memory().current()
      self.assertIs(type(time), registry.Quantity)
      self.assertIs(type(memory), registry.Quantity)
      self.assertEqual((time + registry.Quantity(1, "second")).units, time.units)
      self.assertEqual(memory.to("bit").magnitude, memory.magnitude * 8)
      self.assertEqual(pickle.loads(pickle.dumps(time)), time)  # noqa: S301
      with self.assertRaises(pint.DimensionalityError):
        time.to("byte")
      with self.assertRaises(pint.DimensionalityError):
        time + memory
    finally:
      pint.set_application_registry(previous)

  def test_snapshot_properties_return_independent_quantities(self) -> None:
    snapshot = self.group.cpu().stat()
    usage = snapshot.time.usage
    original = usage.magnitude
    self.assertIsInstance(original, int)
    usage.ito("second")
    self.assertEqual(snapshot.time.usage.units, pint.Unit("nanosecond"))
    self.assertEqual(snapshot.time.usage.magnitude, original)

    stat = self.group.memory().stat()
    anon = stat.bytes["anon"]
    original = anon.magnitude
    anon.ito("MiB")
    self.assertEqual(stat.bytes["anon"].units, pint.Unit("byte"))
    self.assertEqual(stat.bytes["anon"].magnitude, original)

    quota = self.group.cpu().max().quota
    if not quota.is_max:
      original = quota.value.magnitude
      value = quota.value
      value.ito("second")
      self.assertEqual(quota.value.units, pint.Unit("nanosecond"))
      self.assertEqual(quota.value.magnitude, original)
      self.assertIn("nanosecond", repr(quota))

  def test_pages_have_no_implicit_byte_conversion(self) -> None:
    previous = pint.get_application_registry().get()
    registry = pint.UnitRegistry()
    try:
      pint.set_application_registry(registry)
      stat = self.group.memory().stat()
      if not stat.pages:
        self.skipTest("kernel reported no page quantities")
      pages = next(iter(stat.pages.values()))
      self.assertEqual(pages.units, registry.Unit("sakai_page"))
      self.assertTrue(pages.check("[sakai_page]"))
      for unit in ("byte", "count", "dimensionless"):
        with self.subTest(unit=unit), self.assertRaises(pint.DimensionalityError):
          pages.to(unit)
      for definition in ("sakai_page = byte", "sakai_page = 2 other_page"):
        conflicting = pint.UnitRegistry()
        conflicting.define("other_page = [sakai_page]")
        conflicting.define(definition)
        pint.set_application_registry(conflicting)
        with self.subTest(definition=definition), self.assertRaises(ValueError):
          _ = stat.pages
    finally:
      pint.set_application_registry(previous)

  def test_unit_labels_cover_native_metrics(self) -> None:
    registry = pint.get_application_registry()
    cpu = self.group.cpu()
    stat = cpu.stat()
    for value in (stat.time.usage, stat.time.user, stat.time.system, cpu.max_burst()):
      self.assertEqual(value.units, registry.Unit("nanosecond"))
      self.assertIsInstance(value.magnitude, int)
    self.assertEqual(self.group.memory().current().units, registry.Unit("byte"))
    core = self.group.core().stat()
    self.assertEqual(core.descendants.units, registry.Unit("count"))
    self.assertEqual(cpu.pressure().some.avg10.units, registry.Unit("dimensionless"))


if __name__ == "__main__":
  unittest.main()
