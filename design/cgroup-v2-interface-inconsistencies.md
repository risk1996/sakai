# Cgroup v2 value and file inconsistencies

## Snapshot encoding and parsing

- **Map keys can produce invalid text.** Adding a resource named `two words` to a
  `MiscCurrent` snapshot produces `two words 0` when encoded. The same parser
  rejects that line because it expects one key token and one value token.
  The shared map encoder emits keys verbatim, while keyed write commands check
  that the key is a single token.
  [Map codec](../sakai/src/parse/codec.rs#L67) · [Write key validation](../sakai/src/cgroup/v2/interface.rs#L144)
- **Memory snapshots can encode without required fields.** Both
  `MemoryStat::default()` and `MemoryNumaStat::default()` encode successfully
  as an empty string. Both parsers require `anon` and `file`. Editing the
  public maps can also remove these fields from a previously valid snapshot.
  [Memory stat](../sakai/src/cgroup/v2/memory/stat.rs#L61) · [NUMA stat](../sakai/src/cgroup/v2/memory/numa_stat.rs#L109)
- **Distinct controller keys can collapse on a round trip.** A
  `CgroupStat` subsystem map can contain both `CgroupController::Cpu` and
  `CgroupController::Other("cpu".into())`. The encoder emits two
  `nr_subsys_cpu` rows. Parsing the result yields one entry.
  [Subsystem encoder](../sakai/src/cgroup/v2/core/state.rs#L146)

## Parser entry points

- **`cgroup.type` handles whitespace differently across entry points.**
  `CgroupType::decode("domain\n")` succeeds because the file decoder trims
  input. `"domain\n".parse::<CgroupType>()` fails.
  [File decoder](../sakai/src/cgroup/v2/core.rs#L93)
- **Duplicate keys can hide malformed values in some files.**
  `pids.events` accepts `"max bad\nmax 0"` because the shared keyed parser
  collects rows before parsing values. `memory.stat` rejects
  `"anon bad\nanon 0\nfile 0"` because it parses values as it reads rows.
  [Keyed parser](../sakai/src/parse/mod.rs#L25) · [Memory stat parser](../sakai/src/cgroup/v2/memory/stat.rs#L76)

## File type conventions

- **Write command types vary across interfaces.** `MemoryMax` takes the
  inner `MaxOr<Bytes>`, `CpuMax` takes a borrowed snapshot, and `CgroupType`
  takes `()` to request `threaded`. A borrowed snapshot can also be
  projected to a command. For example, `CpusetPartition` encodes the snapshot
  `root invalid (test reason)` as `root` for a write. Those signatures make
  different promises about what the caller supplies and what gets written.
  [Scalar commands](../sakai/src/scalar.rs#L116) · [Default command macro](../sakai/src/cgroup/v2/interface.rs#L136) · [Topology command](../sakai/src/cgroup/v2/core.rs#L98) · [Partition command](../sakai/src/cgroup/v2/cpuset/partition.rs#L41)
- **Local event files expose different snapshot shapes.** `PidsEventsLocal`
  is a file marker that returns `PidsEvents`. `MemoryEventsLocal` is its own
  snapshot type and shares `MemoryEventCounts` with `MemoryEvents`.
  [PID events](../sakai/src/cgroup/v2/pids.rs#L62) · [Memory events](../sakai/src/cgroup/v2/memory/events.rs#L55)
