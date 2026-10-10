# Kubernetes resource assertions through Python

This test builds the current checkout's Python wheel and runs it inside an
unprivileged, single-container Pod on K3s with cgroup v2, using k3d and Docker.
The Pod manifest is the single source of resource values. Kubernetes passes the
admitted container resources to environment variables through the Downward API;
the test compares Sakai's native readers against those values. No cgroup files
are written by the test.

| Pod container resource | Downward API environment variable | Python reading |
| --- | --- | --- |
| `limits.cpu` | `CPU_LIMIT_MILLICORES` | `cpu().max().cpu_count.value` |
| `limits.memory` | `MEMORY_LIMIT_BYTES` | `memory().max().value` |
| `requests.cpu` | `CPU_REQUEST_MILLICORES` | `cpu().weight().shares` |
| `requests.memory` | `MEMORY_REQUEST_BYTES` | `memory().min()` |

CPU limits also assert that quota divided by period matches the supplied limit,
without assuming a particular CPU period. The CPU request is converted to a
relative scheduling weight: Kubernetes v1.32 converts millicores to v1 shares,
and the runtime converts those to a v2 weight. The conversion constants in the
test describe the kernel interface ranges and Kubernetes unit conversion;
resource values themselves appear only in the Pod manifest.
Sakai's `CpuWeight.shares` property exposes this v2 weight;
the conversion is lossy and cannot recover an exact Kubernetes CPU request.
This expectation targets the runtime bundled with the pinned K3s image;
newer runtimes may use a different shares-to-weight conversion.

Memory requests are scheduling metadata on a default cluster and cannot generally
be recovered from cgroups. This fixture explicitly enables the alpha
`MemoryQoS` feature in an isolated Kubernetes v1.32 cluster, where kubelet sets
`memory.min` to the memory request. It is a validation configuration, not a
recommendation to enable the feature in production. See the
[v1.32 kubelet implementation](https://github.com/kubernetes/kubernetes/blob/v1.32.0/pkg/kubelet/kuberuntime/kuberuntime_container_linux.go)
and [runtime weight conversion](https://github.com/opencontainers/runc/blob/v1.2.4/libcontainer/cgroups/utils.go).

With Docker running, run from the repository root. The locked devenv provides
`k3d` and `kubectl`; the cluster configuration pins K3s and disables Traefik,
ServiceLB, metrics-server, and the extra k3d load-balancer container:

```text
devenv tasks run check:kubernetes
```

All four tests must pass and the Pod must reach `Succeeded` (exit code zero).
Missing interfaces or an incorrect resource value fail the test. The test is
skipped in ordinary installed-package test discovery unless
`SAKAI_KUBERNETES_TEST=1` is set by the fixture. The task prints Pod logs and
status and cleans up its cluster on success or failure. It uses a separate
kubeconfig and leaves the current kubectl context alone. A pre-existing cluster
with the same name makes creation fail without deleting it.

CI runs this task in one Linux job; there is no Kubernetes version matrix.
The task is deliberately separate from `devenv test`, which does not require
a running Docker daemon.
