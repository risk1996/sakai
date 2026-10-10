"""Exact resource assertions for tools/kubernetes/pod.yaml with MemoryQoS enabled."""

import os
import unittest

from sakai import Cgroup

MILLICORES_PER_CPU = 1000
CPU_SHARES_PER_CPU = 1024
CPU_SHARES_MIN = 2
CPU_SHARES_MAX = 262_144
CPU_WEIGHT_MIN = 1
CPU_WEIGHT_MAX = 10_000


@unittest.skipUnless(
  os.environ.get("SAKAI_KUBERNETES_TEST") == "1", "requires the Kubernetes test Pod"
)
class KubernetesResourceTest(unittest.TestCase):
  @classmethod
  def setUpClass(cls) -> None:
    cls.group = Cgroup.current()

  def test_cpu_limit(self) -> None:
    limit_millicores = int(os.environ["CPU_LIMIT_MILLICORES"])
    maximum = self.group.cpu().max()
    self.assertFalse(maximum.cpu_count.is_max)
    self.assertFalse(maximum.quota_ns.is_max)
    self.assertEqual(maximum.cpu_count.value, limit_millicores / MILLICORES_PER_CPU)
    self.assertGreater(maximum.period_ns, 0)
    self.assertEqual(
      maximum.quota_ns.value * MILLICORES_PER_CPU,
      maximum.period_ns * limit_millicores,
    )

  def test_cpu_request_weight(self) -> None:
    request_millicores = int(os.environ["CPU_REQUEST_MILLICORES"])
    # Kubernetes v1.32: milliCPU -> v1 shares; runtime: shares -> v2 weight.
    # This lossy conversion cannot recover the original request exactly.
    shares = max(
      CPU_SHARES_MIN,
      min(
        CPU_SHARES_MAX,
        request_millicores * CPU_SHARES_PER_CPU // MILLICORES_PER_CPU,
      ),
    )
    expected_weight = CPU_WEIGHT_MIN + (
      (shares - CPU_SHARES_MIN)
      * (CPU_WEIGHT_MAX - CPU_WEIGHT_MIN)
      // (CPU_SHARES_MAX - CPU_SHARES_MIN)
    )
    weight = self.group.cpu().weight()
    self.assertFalse(weight.is_idle)
    # The binding calls this field "shares", but it contains cpu.weight (v2).
    self.assertEqual(weight.shares, expected_weight)

  def test_memory_limit(self) -> None:
    limit_bytes = int(os.environ["MEMORY_LIMIT_BYTES"])
    maximum = self.group.memory().max()
    self.assertFalse(maximum.is_max)
    self.assertEqual(maximum.value, limit_bytes)

  def test_memory_request_protection(self) -> None:
    request_bytes = int(os.environ["MEMORY_REQUEST_BYTES"])
    # Kubernetes v1.32 sets memory.min from requests.memory only with MemoryQoS.
    self.assertEqual(self.group.memory().min(), request_bytes)


if __name__ == "__main__":
  unittest.main()
