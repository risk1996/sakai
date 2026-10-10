# Sakai Python bindings

`sakai` exposes the current read-only Linux cgroup v2 readers from
`sakai-core`. Python 3.11 or newer is required. The native module uses the
`cp311-abi3` stable ABI for regular CPython; free-threaded CPython needs a
separate wheel and is not supported by this build.

```python
from sakai import Cgroup, InterfaceMissingError

group = Cgroup.current()
cpu_time_ns = group.cpu().stat().time.usage_ns
quota = group.cpu().max()
print(cpu_time_ns, "max" if quota.cpu_count.is_max else quota.cpu_count.value)

try:
  memory_bytes = group.memory().current()
except InterfaceMissingError as error:
  print(f"Unavailable interface: {error.path}")
```

Every method opens a fresh read-only kernel interface. Readings from separate
methods are not atomic together. Limits use `MaxOr[int]` or `MaxOr[float]`:
check `.is_max` before reading `.value`, which raises `ValueError` for `max`.
These limits do not account for ancestor limits, affinity, or other policy. The `path`
property is for display and may become stale; live reads continue through the
original open directory handle.

Native classes, methods, and snapshot attributes carry Python `__doc__`
strings generated from their Rust documentation comments. Use `help(Cgroup)`
or inspect an individual method such as `Cgroup.current.__doc__`.

Ruff and Ty are supplied by the devenv Python profile. The Python sources use
two-space indentation and the repository's `ruff.toml` enables Ruff's `ALL`
rule set with narrow project-specific exceptions. Run lint, formatting, and
Python 3.11 type checks with:

```text
devenv --profile python test
```

The devenv Python profile uses Python 3.15, and CI tests installed wheels and
source distributions on Python 3.11 and 3.15.

To build locally on Linux, use an explicit Python 3.11 or newer interpreter:

```text
devenv --profile python shell -- rtk maturin build --manifest-path sakai-python/Cargo.toml --interpreter /path/to/python3.11 --locked
devenv --profile python shell -- rtk maturin sdist --manifest-path sakai-python/Cargo.toml
```

Install the wheel into a clean environment and run
`python -m unittest discover -s sakai-python/tests` from outside the checkout.

For exact assertions against Kubernetes container CPU and memory resources,
run the [Kubernetes resource test](../tools/kubernetes/README.md). It checks the
installed Python bindings inside a Pod, including request mappings with
MemoryQoS enabled.
